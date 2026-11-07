# Settings and themes on the Mac (the design of 6 November)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** [The Mac's Settings and themes design](2026-11-06-jaw-mac-settings-and-themes-design.md), settled with the owner on 6 November, built: Settings leaves its sheet for a window of its own whose panes are icon tabs in its toolbar, as Redlamp's are, opened with ⌘, and with no Done; and the Appearance pane gets a slot for dark and one for light, filled from a gallery of Jaw's Tokyo Night, Omarchy's 23 from jawd's `themes` and Redlamp's 22 families, each drawn as a small chat in its colours. Omarchy's are mapped onto the Mac's roles as `crates/jaw/src/theme.rs` maps them onto the terminal's and held readable on both grounds the Mac draws on; Redlamp's are copied into jaw, relicensed by their owner under jaw's MIT, kept in step by `scripts/sync-themes` and held by the same bands; and the slots' themes' colours are remembered, so the first frame never waits on jawd.

**Architecture:** JawThemes gains the theme model, free of AppKit and tested on its own: `Contrast`, `theme.rs`'s rule ported as it is onto `0xRRGGBB` colours, so the Mac lands on the terminal's; `Omarchy`, which maps a theme's colours by Omarchy's keys and reads a `colors.toml` as jaw-proto does; Redlamp's catalog, copied (`RedlampCatalog.swift`) and mapped (`RedlampRoles`); and `Theme`, `Slot`, `Slots`, `OmarchyTheme` and `SystemAccent`, with the catalog's list of every theme and the roles a slot draws. Every theme's roles then go through one hold, `Roles.held()`, which keeps each colour drawn as text or as a mark at 3:1 wherever JawUI draws it, the tints it lays over a ground included, the accent drawn as text in a role of its own. JawKit asks `themes` once a connection and hands the answer up as `JawUpdate.themes`. JawUI's `Preferences` keeps the slots, jawd's themes and the remembered copies, and works out the roles the app draws in; `SettingsTabs`, an `NSTabViewController` in its toolbar style, hosts the three SwiftUI panes; and the Appearance pane draws the slots and the gallery from `ThemeCard`s. The app keeps only `SettingsWindowController` and the wiring: ⌘, and the palette's Choose a theme, asking jawd on each hello, and the bench's `--keys settings`.

**Tech Stack:** Swift 6 (AppKit, SwiftUI, Swift Testing), Tuist, xcodebuild on the build Mac through `mac/scripts/build-mac`; Python 3 and git for `scripts/sync-themes`.

## Global Constraints

- The spec is the design, `docs/plans/2026-11-06-jaw-mac-settings-and-themes-design.md`, with the owner's answers of 6 November marked "Owner, 6 Nov" there and the Defaults he left as they are; the wireframes canvas (`mac-settings-themes-wireframes.canvas.tsx` in the project's canvases) draws it. Where this plan and the design differ, the design wins, except where the Decisions below say this plan departs from it on purpose: stop and say so.
- The branch is `mac-settings-themes`, in its own worktree, `/Users/pedrogomes/src/jaw-mac-settings`, made from main at d4ca8bca and rebased onto origin/main at 9f4c298b (main's panes' outline fix and parity step 2.5, attachments) after Task 4 and Task 1's fix; it holds the design, the owner's answers and this plan. Work only there. Before the final report, run `git fetch origin && git log --oneline HEAD..origin/main`; if main has moved again, rebase (`git rebase origin/main`), which may conflict where main changes the files this branch does: resolve each conflict keeping both sides, and bring any draw site main adds in the accent as text or as a mark, in the dim text, or on a ground the hold doesn't name into the hold, Task 4b's table and its tests, as Task 4b did main's attachments. Line numbers are d4ca8bca's, but Task 4b's, which are 5e55e068's: once rebased, find each place by the symbol or the text quoted.
- Never push, never pass `--no-verify`, never rewrite commits that aren't yours. Never edit, build in, check out in, or run a git command that changes anything in another agent's worktree or in the owner's checkout, `/Users/pedrogomes/src/jaw`; only read them. Redlamp's checkout, `/Users/pedrogomes/src/darkroom`, is read only: `scripts/sync-themes` reads it with `git show` and `GIT_OPTIONAL_LOCKS=0`, never checks anything out and never fetches, and nothing else runs a command there.
- Build and test only on the build Mac with `mac/scripts/build-mac`: `test JawKit`, `test JawThemes`, `test JawUITests`, `test JawHarnessTests`, `build` (the `jaw` app), `shot` and `sweep` (the harness's pictures), `snapshot` (the app's picture, `--demo` on a folder of its own there, which it removes) and `references` (the reference pictures the tests wrote there, copied back here). `mac/scripts/check` exits 1 when a build or the tests fail, so both a run's exit status and its printed `** BUILD SUCCEEDED **` / `** TEST SUCCEEDED **` and `Test run with …` lines count. For the whole log: `ssh -o BatchMode=yes build-mac 'tail -150 ~/src/build-mac/jaw/jaw-mac-settings/mac/build/check.log'`. Before each run, check that the build Mac answers: `ssh -o BatchMode=yes -o ConnectTimeout=5 build-mac true`. If it doesn't, or `build-mac` prints `doesn't answer; running here` and falls back to building on this Mac, stop (interrupt the run you started) and ask the controller. Never build or test on this Mac.
- Never build Rust or Go. JawKit's live tests need `target/debug/jawd` in this worktree, which `build-mac` copies to the build Mac with the checkout: it's there, jawd at 660701c5, which serves `themes`. Leave it as it is.
- Never run jawd, the Go engine or the app on this Mac, and nothing here names `~/Library/Application Support/jaw`. On the build Mac, jawd runs only in JawKit's live tests (`--engine demo`, each in a folder of its own) and in `snapshot`'s `--demo`; the Go engine never runs anywhere. Never stop a process by name.
- Swift 6 language mode, macOS 26, warnings are errors. `mac/scripts/check-components`, which every build runs, refuses a font, a corner radius or a line width set by hand outside `JawUI/Sources/Components`: the code here takes them from `TextStyle`, `Space`, `Radius` and `Stroke`, and uses SwiftUI's semantic fonts and colours (`.callout`, `.secondary`, `Color.accentColor`) only where the system's own look is meant. `mac/scripts/check-harness` refuses a view or a view controller defined in the app target, which is why `SettingsTabs` is JawUI's, and `Jawd` or `Client` named in the harness.
- Every commit passes `make check-parity`. The Mac sending `themes`, which the terminal never sends (it reads Omarchy's themes from jaw-proto), is no gap; `mac/parity.toml`'s rows for step 3.5, `settings.set` and the `settings` push, stay, since they're its other half, the settings jawd keeps, which comes after this.
- No audit: nothing here is on jaw's data paths. `scripts/check-data-audit.py`'s `DATA_PATHS` name paths under `crates/`, `engine/` and `proto/engine/` and none under `mac/`, `scripts/`, `protocol/` or `docs/`, which is everything this plan changes; the app never opens jaw.db, and `themes` only reads what jawd serves. `make check-audit` passes on every commit, as there's nothing for it to find.
- Commit messages are one long sentence in the project's voice (read `git log -5`), with no prefixes, each with the trailer `Co-authored-by: Cursor <cursoragent@cursor.com>`, as `git commit -F- --trailer "Co-authored-by: Cursor <cursoragent@cursor.com>"` writes it. Code matches its surroundings: doc comments in plain sentences, British spelling, no comments that narrate a change, 4-space indentation, and trailing commas in multi-line argument lists.
- Scratch goes in the worktree's ignored `mac/build/settings/` (pictures to look at with the Read tool) and the build Mac's copy of the checkout. Before the final report, run `rm -rf mac/build/settings` and `make prune-builds`. Once the branch has landed and its worktree is removed, its copy on the build Mac goes too: `ssh -o BatchMode=yes build-mac 'rm -rf ~/src/build-mac/jaw/jaw-mac-settings'`.
- **Overlaps.** Three open Mac branches touch files this plan changes: `mac-menus` (AppModel, Bench, Commands, MainWindowController, the harness), `mac-panes` (AppModel and JawKit's `Incoming.swift` and `Jawd.swift`) and `mac-perf` (AppModel, Bench, MainWindowController, `Jawd.swift`, the Mac README). None has had a commit since 31 October. If one lands first, rebase and keep both sides: their cases beside this plan's in `Expect`, `Reply`, `Asked`, `JawUpdate` and `AppModel.apply`, their steps beside `settings:` in `Bench.press`; then run every test again and `make check-parity`.

## Decisions

The design's decisions hold: the owner's of 6 November and its Defaults he left as they are ("Decisions" there). These are this plan's own, where the design leaves the how to it; eight depart from the design on purpose, each saying where and why: `ThemeFamily` kept beside `Theme`; the slots' tiles as wide as the form gives them; served themes kept when jawd goes; the Appearance pane a scroll view of open sections rather than a grouped form; only the slot chosen for's card ticked; the harness's pictures not compared with references; the budgets measured after this plan; and the board left to the parity workstream.

- **The window comes first** (the plan's). It answers the owner's first words on its own ("the done button and the top segmented controls are not looking great"), and every later task changes only the Appearance pane inside it. Until Task 6 brings the gallery, the pane chooses with menus: the one theme menu there is now (Task 1), then a menu for each slot (Tasks 4 and 5), which Task 6 replaces.
- **`ThemeFamily` and `Catalog.jaw` stay as they are** (the plan's). The design has `Theme` take `ThemeFamily`'s place; about thirty places, nearly all tests, read `Catalog.jaw.dark`, `.light` and `.roles(dark:)`, so `Theme` comes beside it instead, and `Catalog.jawTheme` is Jaw's Tokyo Night as a `Theme`. `Catalog.families` and `Catalog.family(_:)` stay for the harness until Task 7 moves it to every theme.
- **`SettingsTabs` is JawUI's** (the plan's). An `NSTabViewController` subclass is a view controller, which `check-harness` keeps out of the app target, and it needs one: to size the window as its tab is chosen and title it. The app keeps `SettingsWindowController`, a window controller, which the check allows. Its `transitionOptions` are none, so a tab shows at once while the window's frame moves.
- **⌘W closes Settings** (the plan's). The design closes it with ⌘W, which jaw's menu bar gives to Close Pane, the main window's, enabled while there are several panes. `SettingsWindow`, JawUI's, takes ⌘W before the menu bar does, as AppKit offers a key equivalent to the key window first, and closes; ⇧⌘W stays the menu's Close Window.
- **Each pane has a fixed size, and the window takes it from its content view** (the plan's). Appearance is 500 by 640 points (180 with Task 1's one menu and 230 with Task 4's two, until Task 6 gives it the gallery), Messages 500 by 360 and Panes 500 by 220, as the design has them, each the frame of its SwiftUI form, with the hosting controller's own sizing off and no preferred content size, so AppKit doesn't size the window itself as a tab is chosen, keeping its bottom edge. Choosing a tab grows or shrinks the window by the difference between its content view and the pane, keeping its top edge, which holds whatever height the toolbar has. A form taller than its pane scrolls, as a grouped form does; if the snapshots show one cut, raise its height in `SettingsPane.size`.
- **The cards and the tiles take the pane's width** (the plan's; the tiles' width departs from the design). The design's 144 points is three across 460; the pane's padding, 20 points each side as Messages' and Panes' grouped forms have it, leaves 460 in a 500-point window, so the gallery's rows hold three cards of equal width, each about 148 points, and a card is as wide as its share and 104 points tall. The reference pictures draw them 144 wide, three across, as the design does. The design's tiles are 224 by 64; two side by side with the space between them take what the row gives them, about 226 each, at least 64 tall and as tall as each other, their headings level, so they fill the row whatever a scroller or the insets take from it.
- **Colours by Omarchy's keys, in two types** (the plan's). JawKit and JawThemes don't depend on each other, so JawKit decodes `themes` into its own `ServedTheme` (`id`, `name`, `mode`, `colors` as `[String: String]`), mirroring jaw-proto, and the app hands each to JawThemes as an `OmarchyTheme` (`id`, `name`, `dark`, `colours`). Both keep the colours as jawd sends them, `#rrggbb` by key, which is also how the app remembers them and how `Omarchy.file` reads a `colors.toml`.
- **The slots' Omarchy themes remembered as dictionaries** (the plan's). `themeDarkColours` and `themeLightColours` each hold that theme's colours by Omarchy's keys, with its ID under `id` and its name under `name`, which no colour's key is, so `defaults read dev.badcoffee.jaw` shows them as they are. A copy is read only while its slot holds its ID, written each time jawd serves the slot's theme, and dropped when jawd serves themes without it, so the next launch draws what this one does from its first frame.
- **What the app draws is worked out once until something it rests on changes** (the plan's). `Preferences` keeps each appearance's roles and the list of every theme until the slots, jawd's themes or the system's accent change, since `AppModel.style` is read on every layout. The system's accent, orange, green and red are read in both appearances (`SystemAccent`), for Neutral's two halves, and again when AppKit says the system's colours changed (`NSColor.systemColorsDidChangeNotification`), which the app now listens for beside the appearance's. jawd's answer to `themes` tells the app only when a slot's roles change, so its first answer, which comes as a launch's first rows do, restyles no window while the slots hold Jaw's or Redlamp's themes or a copy jawd serves as it was; Settings redraws through Observation either way.
- **A snapshot's preferences write nothing back** (the plan's). Every checkout's snapshots on the build Mac share the app's preferences, and `save()` writes all of them, those given as arguments too, so a `--keys` step that chose a theme would leave it, and `-appearance dark`, for the next checkout's snapshots. `Preferences.keeps` is false in a snapshot, which reads what's kept and the arguments and writes nothing, the copies of Omarchy's themes included, as it already leaves the window's size and the layouts alone.
- **What the Omarchy section says** (the plan's). `OmarchyState` adds `asking`, from hello to jawd's answer, which jawd gives at once, and covers the app still connecting: neither says anything, so the section never says jawd isn't answering while it is. A jawd the app couldn't start is said as one that isn't answering, and a refusal other than an unknown method in jawd's words. Its line is the section's footer, under the header "Omarchy's, from jawd", as the wireframes have it.
- **Served themes stay when jawd goes** (the plan's; departs from the design, which keeps them "while the connection lasts"). jawd's answer still holds after a drop, and the next hello asks again and replaces it, so the gallery doesn't empty its Omarchy section, nor the slots fall back to their copies, each time jawd restarts.
- **Neutral's colours** (the plan's). Its mention, online and failed colours are the system's orange, green and red, read from AppKit in each appearance with the accent (`SystemAccent`), again when the system's colours change, and held as a family's are; the tests and the harness take macOS 27.0.1's, as AppKit draws them there (`#FF9230`, `#30D158` and `#FF4245` dark; `#FF8D28`, `#34C759` and `#FF383C` light), with its default accent, `#007AFF` in both appearances; its separator is Redlamp's divider as authored, white at 7% or black at 9%, as every family's border is as authored.
- **`scripts/sync-themes` reads three regions** (the plan's): the families' lines, from `// omoi-station classic palettes` to the last of its originals; the Redlamp family's two palettes; and the brand's nine colours they're made of. jaw's copy holds each between `// sync-themes: <region>` and `// sync-themes: end`, line for line as Redlamp has it, so the comparison is of text. `--check` also says when Redlamp's checkout's own `main` has changed those files since the pinned commit, Neutral's tokens in `Tokens/Palette.swift` included, which `RedlampRoles.neutral` is written from by hand. With no checkout, as in CI, it says so and passes.
- **Pictures** (the plan's). JawUITests compare two reference pictures, `theme-gallery-dark` and `theme-gallery-light`, every theme's card for each slot, so a change to any mapping shows. The design has the harness's scenes compared with references too; this plan has the harness draw the panes, the galleries and their states for review (`shot`, `sweep`) and test them as its other scenes are tested (as wide as they say, with notes, the same picture twice), not against references, since the panes are the system's controls in the system's accent, which a reference would pin to one build Mac's macOS and accent. The app's `--snapshot` with `--keys settings:<pane>` pictures the window with its toolbar, which only AppKit draws, to look at.
- **The budgets are measured after this plan** (the plan's). The design's Testing asks for `--bench launch` within its 250 ms p99 with `themes` asked and `--bench idle` with Settings open. `build-mac` has no bench step, and these constraints keep jawd on the build Mac to the live tests and `snapshot`'s `--demo`, so the final report leaves both to the controller, measured as `mac/README.md`'s are, with a Release build on the build Mac. What holds them meanwhile: `themes` is asked in `.hello`, after the first frame, and answered at once; the roles are worked out once until something they rest on changes; and an open Settings draws only when what it observes changes.
- **Settings isn't over the window any more** (the plan's). The harness's With overlays scene loses its Settings band, and `ScreenSpecimen.Overlay` its `settings`, as Task 1 makes Settings a window of its own; Task 7 gives the harness a Settings section with scenes of their own. The harness draws Neutral in macOS's own blue (`SystemAccent.blue`), so its pictures don't depend on the build Mac's accent.
- **The palette's Choose a theme has no key and no menu** (the plan's), as the terminal's palette offers Change the theme: it opens Settings on Appearance.
- **The arrows in the gallery** (the plan's): ← and → go to the theme before or after in the gallery's order, across its sections; ↑ and ↓ to the card above or below in its column, three across, past a section's edge into the next section's first row or the last row of the one before, to that row's last card where it's shorter; past the gallery's ends nothing moves. From Jaw's lone card, ↓ is Omarchy's first. Each card they reach is previewed from the slots as they were at a run's first arrow (`Preferences.arrowsFrom`), as the terminal previews a theme as it moves through its list: those slots as they were where it's the theme they started on, or as a click there would fill them, so a theme with both halves fills the other slot only while the arrows rest on it, → ← loses nothing, and where they stop is what a click there would have chosen; while a run lasts and jawd's themes aren't in, the gallery keeps the start's Omarchy themes from their copies, so the arrows can come back to them. Any choice that isn't an arrow ends the run, a click in the pane, a tile, a card's menu and VoiceOver's actions, as Settings opening does. Which rule the arrows follow, this or filling only the slot shown as ⌥-click does, is the owner's to confirm (Task 6's review, Critical 1).
- **What a click chooses from** (the owner's, 7 November): a click on the theme already chosen for the slot shown changes nothing, ⌥ or not, and while the arrows run a click on a card, ⌥-click, a card's menu and VoiceOver's actions choose from the slots they started from, never from what they show, so a click on the run's start puts everything back as it was and a plain click where they rest keeps what they show, while a tile, which chooses no theme, ends the run keeping what they show, as Settings closing does, and shows the other slot's gallery.
- **The arrows reach the gallery through AppKit** (the plan's). The design has them move through the cards; a SwiftUI view takes keys only while it's focused, which with the default interactions needs the Mac's keyboard navigation on, and which a click on a card doesn't give. `ArrowCatcher`, an invisible AppKit view behind the pane, becomes the window's first responder as the pane opens and again after any click in the pane, on a card, a tile, the radio buttons or the pane's ground (a tap the scroll view's content recognises beside each one's own, which a test shows leaves a card's click choosing it and a radio button's setting the appearance), takes the four arrows without modifiers, and passes every other key on, so the arrows work from the moment the pane opens, with no click and keyboard navigation off. With keyboard navigation on, Tab still reaches the pane's controls, whose arrows are their own until the pane is clicked. Each move scrolls its card into sight with `ScrollViewReader`.
- **The Appearance pane is a scroll view of open sections** (the plan's; departs from the design's grouped form). The appearance's radio buttons sit in a `GroupBox`, at the end of its row after its label as Messages' controls do, and the slots and the gallery's sections under their headers with their footers, open rather than boxed, as the wireframes draw them, set in 20 points from the pane's edges as the grouped forms' sections are and their headers and footers 10 more, as a grouped form's words are from its box, so the three panes line up as the tabs change, the footers the wireframes' too: "Jaw's own Tokyo Night, with Tokyo Night Day as its light half.", "As the terminal draws them; jawd serves them." and "Redlamp's theme families, each a dark and a light half.". A grouped form gives no reliable way to scroll to a card inside a section's row; a `ScrollView` does, so the arrows' card is always in sight. Messages and Panes stay grouped forms.
- **What a slot's tile says when it draws Jaw's in its theme's place** (the plan's). The design words one case, "Gruvbox isn't among jawd's themes", once jawd has served its themes; before it has, with no copy, the tile says "Gruvbox, once jawd serves it"; a theme without the slot's half, "Tokyo Night Day, as Gruvbox has no light half", naming the half of Jaw's it draws; and an ID no source has, "Tokyo Night, as no source has <id>" (Tokyo Night Day in the light slot). A theme's half is named as its own where it has one, so Jaw's light half is Tokyo Night Day in the gallery, the tile and VoiceOver, as the design and the wireframes call it. A card under the pointer is ringed in the secondary label colour.
- **Only the card of the slot chosen for is ticked** (the plan's; departs from the design, which ticks "the cards in the slots"). The gallery shows the half of one slot, so the card it ticks is that slot's theme; a theme in the other slot alone, as ⌥-click leaves one, isn't ticked there, where choosing it would fill the slot shown.
- **The slot the gallery chooses for is `Preferences.choosing`** (the plan's): not kept, nil for the slot in use, and set back to nil each time Settings opens, though not as it's brought to the front, so the pane opens on the slot in use, comes back to the front on the one chosen, and the harness can draw either gallery.
- **VoiceOver chooses as a click does** (the plan's). A card's press chooses it, and its named action, Use for Dark Only (or Light Only), fills only the slot chosen for, as ⌥-click does; a tile's press makes it the slot chosen for. Each card the arrows choose is announced by its label, as the arrows move no VoiceOver cursor, and the outlined tile's value says the gallery shows its themes. The owner's pass by hand then has something to press, and to hear.
- **The parity canvas's words, not the board** (the plan's). The design's list of docs names the parity canvas's Themes and Settings rows and the board's steps 3.5 and 4.3. The board (`ws-mac-parity.canvas.tsx`) and the canvas's states are kept by the parity workstream, which rewrites them as main moves, so Task 8 changes only the words of the canvas's two Mac cells, saying what's on this branch, and leaves their states and the board for that workstream to move when this lands.
- **One hold for every theme, where `Theme` is made and where `Catalog.jaw` is** (the plan's). Every theme the catalog makes, of every source, comes through `Theme`'s initialiser, which holds its halves; `Catalog.jaw`, which the slots fall back on and the harness and about thirty tests take whole, is held where it's made, so the reference pictures draw what the app does. The hold keeps a colour that reads as it is, so `Catalog.jawTheme`, made from `Catalog.jaw`, comes through unmoved, and the mappings' own bands stay as Tasks 2 and 3 pinned them.
- **The hold covers the tints JawUI lays over a ground** (the plan's, on the owner's "all of them at 3:1"): a card's and a quote's well, the accent's tint behind its symbols, the box's chip and a mention of you on its own tint, which moves with it, so its passes are bounded; the tints are made once, in `Roles`, and every draw site takes them from there.
- **Three small marks are drawn on one ground** (the plan's). A link card's thumbnail placeholder is drawn on the card's own well, and a selected library row's file symbol and link icon on the selection, without the tints they sat on, so a tint on a tint on a few marks doesn't move a whole theme's secondary text, accent drawn as text and links; each still reads, as the hold covers the ground it's drawn on.
- **The accent drawn as text is a role of its own** (the plan's). `accentText`, the accent until the hold moves it, is what twenty-eight draw sites take, a file of yours on its way's among them; `accent` stays as the theme wrote it, for its fills and outlines, as the design has it.
- **The dim text is held on the window's ground and the surface; everywhere else it's drawn, it takes the secondary text's colour** (the plan's, by the owner's rule for the dim text on a tint): on your bubbles, a selected row and a selected message's band (`Roles.faint(on:)`, the conversation telling a message's layout when it's selected in lines), and on a card's or a tile's well, a link card's host and a tile's placeholder, as held there the dim text would come level with the secondary text, or near it. Held on the ground and the surface alone it moves in 59 halves and comes within 15% of the secondary text in one, Redlamp's Everforest light (3.26:1 against 3.58:1); Tokyo Night's becomes `66709A`.
- **`SystemAccent` carries the system's orange, green and red** (the plan's), beside the accent it's named for, defaulting to macOS 27.0.1's, so everything that passes it stays as it is and Neutral takes the whole value.

## File structure

New:

- `mac/JawUI/Sources/Settings/SettingsPane.swift`: Settings' panes, each its name, its SF Symbol and its size.
- `mac/JawUI/Sources/Settings/SettingsTabs.swift`: the panes as the toolbar's icon tabs, the window sized and titled as a tab is chosen.
- `mac/JawUI/Sources/Settings/Panes.swift`: the Appearance, Messages and Panes forms.
- `mac/JawUI/Sources/Settings/ThemeCard.swift`: a theme's half as a small chat, a card with its half's name in the gallery, the gallery's rows and sections, an open section as the pane draws one, and a slot's tile (Task 6).
- `mac/JawUI/Sources/Settings/Gallery.swift`: the gallery's sections for a slot, their headers and footers, where the arrows go, and what a slot's tile says (Task 6).
- `mac/JawUI/Sources/Settings/ArrowKeys.swift`: the gallery's arrows, through an AppKit view that holds the window's keys (Task 6).
- `mac/JawUI/Sources/Settings/OmarchyState.swift`: what the Omarchy section says while jawd's themes aren't in (Task 5).
- `mac/JawApp/Sources/SettingsWindowController.swift`: the window: closable, in the app's appearance, on the pane last shown, where it was left.
- `mac/JawThemes/Sources/Contrast.swift`: WCAG's contrast and `theme.rs`'s straight line, on `0xRRGGBB`.
- `mac/JawThemes/Sources/Omarchy.swift`: Omarchy's themes mapped onto the Mac's roles; a `colors.toml` read.
- `mac/JawThemes/Sources/RedlampCatalog.swift`: Redlamp's catalog, copied, under jaw's MIT by its owner.
- `mac/JawThemes/Sources/RedlampRoles.swift`: jaw's roles from Redlamp's ten colours, and Neutral's.
- `mac/JawThemes/Sources/Theme.swift`: a theme to choose, its source and its slot, Omarchy's as jawd serves them, the system's colours, the catalog's list and a slot's roles.
- `mac/JawThemes/Sources/Slots.swift`: the theme in each slot, and how a choice fills them.
- `mac/JawThemes/Sources/Held.swift`: the hold every theme's roles go through: the colours JawUI draws as text or as marks, the grounds and tints it draws each on, and the tints themselves (Task 4b).
- `mac/JawKit/Sources/Protocol/Themes.swift`: `themes`' answer.
- `mac/JawHarness/Sources/Fixtures/Themes.swift`: every theme, Omarchy's read from the harness's bundle (Task 7).
- `mac/JawHarness/Sources/Scenes/SettingsScenes.swift`: the panes, the galleries and their states (Task 7).
- `scripts/sync-themes`: jaw's copy of Redlamp's catalog kept in step.
- Tests: `mac/Tests/JawUITests/SettingsTests.swift`, `mac/Tests/JawUITests/PreferencesTests.swift`, `mac/Tests/JawUITests/GalleryTests.swift`, `mac/Tests/JawThemesTests/OmarchyTests.swift`, `mac/Tests/JawThemesTests/RedlampTests.swift`, `mac/Tests/JawThemesTests/ThemeTests.swift`, `mac/Tests/JawThemesTests/HoldTests.swift`, `mac/Tests/JawUITests/HeldInkTests.swift`, and the reference pictures `mac/Tests/JawUITests/References/theme-gallery-dark.png` and `theme-gallery-light.png`.

Changed:

- `mac/JawThemes/Sources/Roles.swift`: `link` (Task 2); `accentText`, and `Catalog.jaw` held (Task 4b); `Catalog.families` and `Catalog.family(_:)` go (Task 7).
- `mac/JawUI/Sources/Style/Preferences.swift`: the slots, jawd's themes, the remembered copies, the system's colours, and the roles worked out once (Tasks 4, 4b and 5).
- `mac/JawUI/Sources/Layout/MessageLayout.swift`, `mac/JawUI/Sources/Components/Glyphs.swift`, `mac/JawUI/Sources/Library/LibraryDrawing.swift`, `mac/JawUI/Sources/Layout/Cards.swift`: links, read ticks and a link card's bar in `link` (Task 2).
- JawUI's draw sites of the accent as text, the dim text and the tints: `accentText`, `Roles.faint(on:)` and the tints of `Roles`, a message selected in lines told to its layout, and three small marks drawn on one ground (Task 4b): `ChatListController.swift`, `Glyphs.swift`, `Tone.swift`, `MessageLayout.swift`, `Cards.swift`, `ContactCards.swift`, `MessageView.swift`, `ConversationController.swift`, `Palette.swift`, `LibraryDrawing.swift`, `LibraryController.swift`, `Views.swift`, `GlanceView.swift`, `ContactCardsView.swift`, `ContactForm.swift`, and `Panes.swift`'s `RGBA.init(_:)` doc comment.
- `mac/JawKit/Sources/Client/Incoming.swift`, `mac/JawKit/Sources/Client/Jawd.swift`: `themes` (Task 5).
- `mac/JawApp/Sources/AppModel.swift`, `Commands.swift`, `MainWindowController.swift`, `Bench.swift`, `AppDelegate.swift`: the window, Choose a theme, `--keys settings:<pane>` and keys reaching Settings, asking for `themes`, the system's colours, and a snapshot's preferences writing nothing back.
- `mac/JawHarness/Sources/Specimen.swift`, `Scenes/ScreenScenes.swift` (Task 1); `HarnessScene.swift`, `HarnessLaunch.swift`, `ConditionsControls.swift`, `BuiltInScenes.swift`, `Scenes/FoundationScenes.swift` (Tasks 2, 4b and 7); `mac/Project.swift`, the harness's resources (Task 7).
- Tests: `mac/Tests/JawThemesTests/RolesTests.swift`, `mac/Tests/JawKitTests/FixtureTests.swift`, `ClientTests.swift`, `LiveTests.swift`, `mac/Tests/JawUITests/LinkPreviewTests.swift`, `mac/Tests/JawUITests/ComponentTests.swift` (Task 4b), `mac/Tests/JawUITests/SettingsTests.swift` (Task 4), `mac/Tests/JawHarnessTests/ScenesTests.swift`, `LaunchTests.swift`, `CatalogTests.swift`.
- Docs (Task 8): `docs/plans/2026-10-26-jaw-mac-design.md`, `docs/plans/2026-10-26-jaw-mac-plan.md`, `docs/plans/2026-10-27-jaw-mac-parity-plan.md`, `mac/README.md`.

Deleted: `mac/JawUI/Sources/Settings/SettingsView.swift` (Task 1).

Scratch, never committed: `mac/build/settings/`, the pictures each task looks at.

---

### Task 1: Settings in a window of its own, its panes the toolbar's icon tabs

**Files:**
- Create: `mac/JawUI/Sources/Settings/SettingsPane.swift`, `mac/JawUI/Sources/Settings/SettingsTabs.swift`, `mac/JawUI/Sources/Settings/Panes.swift`, `mac/JawApp/Sources/SettingsWindowController.swift`
- Delete: `mac/JawUI/Sources/Settings/SettingsView.swift`
- Modify: `mac/JawApp/Sources/AppModel.swift` (after `let glance = Glance()` at line 60, `restyle()` at 330, after `say(_:warn:)` at 324-328), `mac/JawApp/Sources/Commands.swift` (lines 19-20), `mac/JawApp/Sources/MainWindowController.swift` (line 36 and `showSettings()` at 368-379), `mac/JawApp/Sources/Bench.swift` (lines 112, 142 and 486-487), `mac/JawHarness/Sources/Specimen.swift` (`ScreenSpecimen` at 70-96, `SettingsOver` at 146-182), `mac/JawHarness/Sources/Scenes/ScreenScenes.swift` (`screensOverlays` at 93-101, `OverlaysScene`'s comment at 329-330 and its Settings band at 351-355)
- Test: `mac/Tests/JawUITests/SettingsTests.swift`

**Interfaces:**
- Consumes: JawUI's `Preferences` as it is (`appearance`, `theme`, `look`, `density`, `textSize`, `font`, `paneDim`, `paneOutline`, `paneOutlineColour`, `style(dark:)`, `nsAppearance`), and `Motion.reduced`.
- Produces, in JawUI:
  - `public enum SettingsPane: String, CaseIterable, Sendable`: `.appearance`, `.messages`, `.panes`, each with `title: String`, `symbol: String` and `size: NSSize`.
  - `@MainActor public final class SettingsTabs: NSTabViewController`, with `public init(preferences: Preferences, showing pane: SettingsPane)`, `public var chose: (SettingsPane) -> Void`, `public var pane: SettingsPane`, `public func show(_ pane: SettingsPane)` and `public func fit(_ pane: SettingsPane, animated: Bool)`.
  - `public final class SettingsWindow: NSWindow`, which closes on ⌘W.
  - `public struct AppearancePane: View`, `public struct MessagesPane: View` and `public struct PanesPane: View`, each with `public init(preferences: Preferences)`.
  - `extension RGBA { init(_ colour: NSColor) }`, internal, moved from `SettingsView.swift`.
- Produces, in the app: `SettingsWindowController(preferences:keeps:)` with `show(_ pane: SettingsPane?, over main: NSWindow?)`; `AppModel.settings: SettingsWindowController?` and `AppModel.showSettings(_ pane: SettingsPane? = nil)`; the palette's `theme` command, Choose a theme; and the bench's `--keys` steps `settings` and `settings:<pane>`.

- [ ] **Step 1: Write the failing tests**

Create `mac/Tests/JawUITests/SettingsTests.swift`:

```swift
import AppKit
import JawThemes
@testable import JawUI
import Testing

/// Settings as a window of its own: its panes the toolbar's icon tabs, in order, each its name over
/// its symbol, the window taking each pane's size as its tab is chosen, keeping its top edge, and ⌘W
/// closing it.

/// A `Preferences` on defaults of the test's own, emptied first, so it starts from the app's defaults.
@MainActor
func scratchPreferences(_ name: String) throws -> Preferences {
    let suite = "jaw.tests.\(name)"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defaults.removePersistentDomain(forName: suite)
    return Preferences(defaults: defaults)
}

@MainActor @Test func `Settings' panes are the toolbar's icon tabs, each its name over its symbol, in the toolbar's order`() throws {
    let tabs = SettingsTabs(preferences: try scratchPreferences("settings-tabs"), showing: .messages)
    #expect(tabs.tabStyle == .toolbar)
    #expect(tabs.tabViewItems.map(\.label) == ["Appearance", "Messages", "Panes"])
    #expect(tabs.tabViewItems.map { $0.identifier as? String } == ["appearance", "messages", "panes"])
    #expect(tabs.tabViewItems.map { $0.image?.accessibilityDescription } == ["Appearance", "Messages", "Panes"],
            "each its SF Symbol, named for VoiceOver")
    #expect(SettingsPane.allCases.map(\.symbol) == ["paintpalette", "text.bubble", "rectangle.split.2x1"])
    #expect(SettingsPane.allCases.allSatisfy { $0.size.width == 500 }, "500 points wide, as Redlamp's")
    #expect(tabs.pane == .messages, "it opens on the pane it's asked to")
}

@MainActor @Test func `choosing a pane sizes the window to it, keeping its top edge, titles the window and says which`() throws {
    let tabs = SettingsTabs(preferences: try scratchPreferences("settings-fit"), showing: .appearance)
    let window = NSWindow(contentViewController: tabs)
    window.styleMask = [.titled, .closable]
    window.isReleasedWhenClosed = false
    defer { window.close() }
    tabs.fit(.appearance, animated: false)
    #expect(window.contentView?.frame.size == SettingsPane.appearance.size)
    let top = window.frame.maxY
    var chosen: [SettingsPane] = []
    tabs.chose = { chosen.append($0) }
    for pane in [SettingsPane.panes, .messages, .appearance] {
        tabs.show(pane)
        #expect(window.contentView?.frame.size == pane.size, "\(pane)")
        #expect(window.frame.maxY == top, "\(pane): its top edge stays where it was")
        #expect(window.title == pane.title, "\(pane)")
    }
    #expect(chosen == [.panes, .messages, .appearance] && tabs.pane == .appearance)
}

@MainActor @Test func `⌘W closes Settings' window before the menu bar's Close Pane, the main window's, can take it`() throws {
    let window = SettingsWindow(contentViewController: SettingsTabs(preferences: try scratchPreferences("settings-close"), showing: .panes))
    window.styleMask = [.titled, .closable]
    window.isReleasedWhenClosed = false
    window.setFrameOrigin(NSPoint(x: -30000, y: -30000))
    window.orderFront(nil)
    defer { window.close() }
    func key(_ characters: String, _ modifiers: NSEvent.ModifierFlags) throws -> NSEvent {
        try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: modifiers, timestamp: 0,
                                      windowNumber: window.windowNumber, context: nil, characters: characters,
                                      charactersIgnoringModifiers: characters, isARepeat: false, keyCode: 13))
    }
    #expect(!window.performKeyEquivalent(with: try key("w", [.command, .shift])) && window.isVisible, "⇧⌘W is the menu's Close Window")
    #expect(!window.performKeyEquivalent(with: try key("z", .command)) && window.isVisible,
            "⌘Z stays undo on a layout whose Z is where a US keyboard has W")
    #expect(window.performKeyEquivalent(with: try key("w", .command)) && !window.isVisible)
    window.orderFront(nil)
    #expect(window.performKeyEquivalent(with: try key("ц", .command)) && !window.isVisible,
            "on a layout with no W, the key where a US keyboard has it, as the menu bar matches ⌘W")
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 1, with `error: cannot find 'SettingsTabs' in scope` from `SettingsTests.swift`.

- [ ] **Step 3: Write the panes and their tabs**

Create `mac/JawUI/Sources/Settings/SettingsPane.swift`:

```swift
import AppKit

/// Settings' panes, in the toolbar's order: each its name, the SF Symbol its tab shows over it, and
/// its size, 500 points wide as Redlamp's are and as tall as its form, which the window takes as its
/// tab is chosen.
public enum SettingsPane: String, CaseIterable, Sendable {
    case appearance, messages, panes

    public var title: String {
        switch self {
        case .appearance: "Appearance"
        case .messages: "Messages"
        case .panes: "Panes"
        }
    }

    /// Appearance's is Redlamp's; Panes' is Split Across's, in the Panes menu.
    public var symbol: String {
        switch self {
        case .appearance: "paintpalette"
        case .messages: "text.bubble"
        case .panes: "rectangle.split.2x1"
        }
    }

    public var size: NSSize {
        switch self {
        case .appearance: NSSize(width: 500, height: 180)
        case .messages: NSSize(width: 500, height: 360)
        case .panes: NSSize(width: 500, height: 220)
        }
    }
}
```

Create `mac/JawUI/Sources/Settings/SettingsTabs.swift`:

```swift
import AppKit
import SwiftUI

/// Settings' panes as the Mac's own Settings windows show them: each an icon over its name in the
/// window's toolbar, its SwiftUI form hosted at its pane's size, and the window taking that size as
/// its tab is chosen, keeping its top edge where it is. Every change in a pane is kept as it's made,
/// so there's nothing to confirm.
@MainActor
public final class SettingsTabs: NSTabViewController {
    /// Told each pane chosen, so the next Settings opens on it.
    public var chose: (SettingsPane) -> Void = { _ in }

    public init(preferences: Preferences, showing pane: SettingsPane) {
        super.init(nibName: nil, bundle: nil)
        tabStyle = .toolbar
        transitionOptions = []
        for each in SettingsPane.allCases {
            let host = NSHostingController(rootView: Self.form(each, preferences))
            host.sizingOptions = []
            host.title = each.title
            let item = NSTabViewItem(viewController: host)
            item.identifier = each.rawValue
            item.label = each.title
            item.image = NSImage(systemSymbolName: each.symbol, accessibilityDescription: each.title)
            addTabViewItem(item)
        }
        show(pane)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// The pane on show.
    public var pane: SettingsPane {
        SettingsPane.allCases[max(selectedTabViewItemIndex, 0)]
    }

    /// Shows `pane`, as a click on its tab does.
    public func show(_ pane: SettingsPane) {
        selectedTabViewItemIndex = SettingsPane.allCases.firstIndex(of: pane) ?? 0
    }

    /// Titles and sizes the window for the pane chosen. The first item's choosing comes while `init`
    /// adds it, before the view is loaded, so it doesn't load the view to ask for its window.
    override public func tabView(_ tabView: NSTabView, didSelect tabViewItem: NSTabViewItem?) {
        super.tabView(tabView, didSelect: tabViewItem)
        guard let pane = (tabViewItem?.identifier as? String).flatMap(SettingsPane.init(rawValue:)) else { return }
        let window = isViewLoaded ? view.window : nil
        window?.title = pane.title
        fit(pane, animated: window?.isVisible == true)
        chose(pane)
    }

    /// Sizes the window so its content is `pane`'s size, keeping the window's top edge where it is,
    /// whatever height its title bar and toolbar have. The panes give no preferred size of their own,
    /// so this is the one place the window is sized.
    public func fit(_ pane: SettingsPane, animated: Bool) {
        guard isViewLoaded, let window = view.window, let content = window.contentView else { return }
        var frame = window.frame
        let taller = pane.size.height - content.frame.height
        frame.origin.y -= taller
        frame.size.height += taller
        frame.size.width += pane.size.width - content.frame.width
        window.setFrame(frame, display: true, animate: animated && !Motion.reduced())
    }

    static func form(_ pane: SettingsPane, _ preferences: Preferences) -> AnyView {
        switch pane {
        case .appearance: AnyView(AppearancePane(preferences: preferences))
        case .messages: AnyView(MessagesPane(preferences: preferences))
        case .panes: AnyView(PanesPane(preferences: preferences))
        }
    }
}

/// Settings' window, which ⌘W closes, as it closes a window elsewhere. jaw's ⌘W is the menu bar's
/// Close Pane, the main window's, and AppKit offers a key equivalent to the key window before the
/// menu bar, so this takes it first. On a layout with no W, as a Cyrillic one, the key where a US
/// keyboard has W is taken, as the menu bar takes it there.
public final class SettingsWindow: NSWindow {
    override public func performKeyEquivalent(with event: NSEvent) -> Bool {
        let typed = event.charactersIgnoringModifiers ?? ""
        let w = typed == "w" || (!typed.allSatisfy(\.isASCII) && event.keyCode == 13)
        if event.modifierFlags.intersection(.deviceIndependentFlagsMask) == .command, w {
            performClose(nil)
            return true
        }
        return super.performKeyEquivalent(with: event)
    }
}
```

Create `mac/JawUI/Sources/Settings/Panes.swift`, the three forms as the sheet had them, each a grouped form at its pane's size, its explanation its section's footer:

```swift
import AppKit
import JawThemes
import SwiftUI

/// Settings › Appearance: dark, light or the system's, and the theme.
public struct AppearancePane: View {
    @Bindable var preferences: Preferences

    public init(preferences: Preferences) {
        self.preferences = preferences
    }

    public var body: some View {
        Form {
            Section {
                Picker("Appearance", selection: $preferences.appearance) {
                    Text("The system's").tag(Appearance.system)
                    Text("Dark").tag(Appearance.dark)
                    Text("Light").tag(Appearance.light)
                }
                .pickerStyle(.inline)
                Picker("Theme", selection: $preferences.theme) {
                    ForEach(Catalog.families) { family in
                        Text(family.name == "jaw" ? "Jaw (Tokyo Night)" : family.name).tag(family.id)
                    }
                }
            } footer: {
                Text("Redlamp's themes come in a later version.")
            }
        }
        .formStyle(.grouped)
        .frame(width: SettingsPane.appearance.size.width, height: SettingsPane.appearance.size.height)
    }
}

/// Settings › Messages: bubbles or lines, comfortable or compact, the text size and the font.
public struct MessagesPane: View {
    @Bindable var preferences: Preferences

    public init(preferences: Preferences) {
        self.preferences = preferences
    }

    public var body: some View {
        Form {
            Section {
                Picker("Messages look like", selection: $preferences.look) {
                    ForEach(Look.allCases, id: \.self) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
            } footer: {
                Text(preferences.look == .bubbles
                    ? "Theirs on the left, yours on the right."
                    : "The terminal's layout: names on the left, yours from a third of the way across.")
            }
            Section {
                Picker("Spacing", selection: $preferences.density) {
                    ForEach(Density.allCases, id: \.self) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                LabeledContent("Text size") {
                    HStack {
                        Slider(value: $preferences.textSize, in: Preferences.textSizes, step: 1)
                        Text("\(Int(preferences.textSize)) pt").monospacedDigit().frame(width: 40, alignment: .trailing)
                    }
                }
                Picker("Font", selection: $preferences.font) {
                    ForEach(MessageFont.allCases, id: \.self) { Text($0.title).tag($0) }
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: SettingsPane.messages.size.width, height: SettingsPane.messages.size.height)
    }
}

/// Settings › Panes: how much the panes without the keys are dimmed, and the focused pane's outline.
public struct PanesPane: View {
    @Bindable var preferences: Preferences
    @Environment(\.colorScheme) private var scheme

    public init(preferences: Preferences) {
        self.preferences = preferences
    }

    /// The theme's accent in the appearance Settings is drawn in, the outline's colour unless one's picked.
    var accent: RGBA {
        preferences.style(dark: scheme == .dark).roles.accent
    }

    public var body: some View {
        Form {
            Section {
                LabeledContent("Dim the other panes") {
                    HStack {
                        Slider(value: $preferences.paneDim, in: Preferences.paneDims)
                        Text("\(Int((preferences.paneDim * 100).rounded())) %").monospacedDigit().frame(width: 40, alignment: .trailing)
                    }
                }
                Toggle("Outline the focused pane", isOn: $preferences.paneOutline)
                LabeledContent("Outline colour") {
                    HStack {
                        ColorPicker("", selection: Binding(
                            get: { (preferences.paneOutlineColour ?? accent).color },
                            set: { preferences.paneOutlineColour = RGBA(NSColor($0)) },
                        ), supportsOpacity: false)
                            .labelsHidden()
                        Button("The theme's accent") { preferences.paneOutlineColour = nil }
                            .disabled(preferences.paneOutlineColour == nil)
                    }
                }
                .disabled(!preferences.paneOutline)
            } footer: {
                Text("With more than one pane, the one with the keys is outlined inside its edge, and the others dimmed, as iTerm does.")
            }
        }
        .formStyle(.grouped)
        .frame(width: SettingsPane.panes.size.width, height: SettingsPane.panes.size.height)
    }
}

extension RGBA {
    /// From a colour picked, in sRGB.
    init(_ colour: NSColor) {
        let c = colour.usingColorSpace(.sRGB) ?? colour
        self.init(r: Double(c.redComponent), g: Double(c.greenComponent), b: Double(c.blueComponent))
    }
}
```

Then delete the sheet's view: `git rm mac/JawUI/Sources/Settings/SettingsView.swift`.

- [ ] **Step 4: Run the tests**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 0, `** TEST SUCCEEDED **`, with three more tests than before. The scheme builds JawUI and its tests alone, so the app and the harness, which still name `SettingsView`, don't build yet; the next two steps change them.

- [ ] **Step 5: The app opens the window**

Create `mac/JawApp/Sources/SettingsWindowController.swift`:

```swift
import AppKit
import JawUI

/// Settings, in a window of its own beside the chats: its panes the toolbar's icon tabs
/// (`SettingsTabs`), closable and nothing else, in the app's appearance, opened on the pane last
/// shown, centred over the main window the first time and where it was left after.
@MainActor
final class SettingsWindowController: NSWindowController {
    let tabs: SettingsTabs
    /// Whether the window has its place: kept from an earlier run, or given it as it first opened.
    var placed = false

    /// `keeps` is false for a snapshot, which takes nothing from the last run and leaves nothing for
    /// the next.
    init(preferences: Preferences, keeps: Bool) {
        let kept = keeps ? UserDefaults.standard.string(forKey: "settingsPane").flatMap(SettingsPane.init(rawValue:)) : nil
        let pane = kept ?? .appearance
        tabs = SettingsTabs(preferences: preferences, showing: pane)
        let window = SettingsWindow(contentViewController: tabs)
        window.styleMask = [.titled, .closable]
        window.toolbarStyle = .preference
        window.isReleasedWhenClosed = false
        window.appearance = preferences.nsAppearance
        window.title = pane.title
        super.init(window: window)
        if keeps {
            window.setFrameAutosaveName("jaw.settings")
            placed = window.frame.origin != .zero
            tabs.chose = { UserDefaults.standard.set($0.rawValue, forKey: "settingsPane") }
        }
        tabs.fit(pane, animated: false)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Shows the window, or brings it to the front, on `pane` when one's asked for; the first time,
    /// centred over `main`.
    func show(_ pane: SettingsPane?, over main: NSWindow?) {
        guard let window else { return }
        if let pane { tabs.show(pane) }
        if !placed, let main {
            let size = window.frame.size
            window.setFrameOrigin(NSPoint(x: main.frame.midX - size.width / 2, y: main.frame.midY - size.height / 2))
            placed = true
        }
        showWindow(nil)
    }
}
```

In `mac/JawApp/Sources/AppModel.swift`, after `let glance = Glance()` (line 60), add:

```swift
    /// Settings' window, made the first time it's shown.
    var settings: SettingsWindowController?
```

After `say(_:warn:)` (lines 324-328), add:

```swift
    /// Shows Settings, on `pane` when one's asked for, or brings it to the front.
    func showSettings(_ pane: SettingsPane? = nil) {
        if settings == nil { settings = SettingsWindowController(preferences: preferences, keeps: launch.snapshot == nil) }
        settings?.show(pane, over: window?.window)
    }
```

And make `restyle()`'s first line give Settings the app's appearance, so it changes with Appearance:

```swift
    func restyle() {
        settings?.window?.appearance = preferences.nsAppearance
        window?.restyle()
```

In `mac/JawApp/Sources/Commands.swift`, lines 19-20, the `settings` command, become the command and the palette's Choose a theme beside it:

```swift
        add(Command("settings", "Settings…", phrase: "Settings", detail: "appearance, messages and panes", key: Shortcut(","), place: .app, group: 1)
            { [weak self] in self?.showSettings() })
        add(Command("theme", "Choose a Theme", phrase: "Choose a theme", detail: "Settings › Appearance", place: nil)
            { [weak self] in self?.showSettings(.appearance) })
```

In `mac/JawApp/Sources/MainWindowController.swift`, delete `    var settings: NSWindow?` (line 36), and `showSettings()` with its doc comment (lines 368-379, from `/// Settings, in a sheet over the window: modal to it, centred, Done or Esc to close.` to its closing brace).

In `mac/JawApp/Sources/Bench.swift`:

- Line 112, in `press`, where `let step = …` lists the steps, `key == "settings"` becomes `key == "settings" || key.hasPrefix("settings:")`.
- Line 142, `if key == "settings" { model.window?.showSettings() }`, becomes:

```swift
            if key == "settings" { model.showSettings() }
            if key.hasPrefix("settings:") {
                if let pane = SettingsPane(rawValue: String(key.dropFirst(9))) {
                    model.showSettings(pane)
                } else {
                    FileHandle.standardError.write(Data("\(key): no such pane\n".utf8))
                }
            }
```

- Lines 486-487, in `draw(to:model:)`, the end of the chain that picks the view to draw, `?? model.window?.editorSheet?.contentView ?? model.window?.pollSheet?.contentView ?? model.window?.settings?.contentView`, becomes the line below, so the picture holds Settings with its toolbar once a step has opened it:

```swift
            ?? model.window?.editorSheet?.contentView ?? model.window?.pollSheet?.contentView
            ?? model.settings?.window.flatMap { $0.isVisible ? $0.contentView?.superview : nil }
```

- [ ] **Step 6: The harness no longer draws Settings over the window**

In `mac/JawHarness/Sources/Specimen.swift`:

- `ScreenSpecimen`'s doc comment (lines 70-73) becomes:

```swift
/// A whole window's content at the screen's size, as the window holds it, made from a `Screen`, with
/// the title bar's buttons over it where the window puts them, and over both, when one is open, the
/// palette or the glance, each a view where the window puts its panel, so a picture holds it.
```

- `case palette, glance, settings` becomes `case palette, glance`, and the line `case .settings: SettingsOver(conditions: conditions)` goes.
- `SettingsOver` goes, with its doc comment: lines 146-182, from ``/// Settings as `MainWindowController.showSettings()` shows them`` to the struct's closing brace.

In `mac/JawHarness/Sources/Scenes/ScreenScenes.swift`:

- `screensOverlays`' synopsis becomes `"Lisbon Climbing open with the palette or the glance over it, and Ana Ribeiro open with the glance listing Lisbon Climbing"`, and `, SettingsView.self` goes from its `covers`.
- `OverlaysScene`'s doc comment becomes:

```swift
/// Lisbon Climbing open with the palette and the glance each open over it, and Ana Ribeiro open with
/// the glance over her chat, listing Lisbon Climbing, each where the window puts it.
```

- Its last band, the `ScreenBand` titled `"Settings"` with `overlay: .settings`, goes.

- [ ] **Step 7: Build, test and look at the window**

Run: `mac/scripts/build-mac build`, then `mac/scripts/build-mac test JawUITests` and `mac/scripts/build-mac test JawHarnessTests`; then `make check-parity`.
Expected: each exits 0. A debug launch asserts that no command is listed twice and no key belongs to two commands or to one without a menu (`Menus.problems`), which Choose a theme, with neither key nor menu, keeps.

Make the scratch folder the pictures come back to, which `build-mac`'s `scp` doesn't make: `mkdir -p mac/build/settings`.

Run, one at a time: `mac/scripts/build-mac snapshot mac/build/settings/appearance.png --keys settings:appearance`, then the same with `messages.png` and `settings:messages`, and `panes.png` and `settings:panes`.
Expected: `{"panes":1.0,"snapshot":1.0,…}` among each run's last lines, and each picture, looked at with the Read tool, shows the Settings window alone: titled with its pane's name, the toolbar's three icon tabs, Appearance's palette, Messages' speech bubble and Panes' split rectangle, each over its name and the chosen one filled, its form below at its size with its explanation as the section's footer, and no Done. Appearance shows The system's, Dark and Light, Theme with Jaw (Tokyo Night), and "Redlamp's themes come in a later version." under them.

- [ ] **Step 8: Commit**

```bash
git add mac/JawUI/Sources/Settings mac/JawApp/Sources/SettingsWindowController.swift mac/JawApp/Sources/AppModel.swift \
    mac/JawApp/Sources/Commands.swift mac/JawApp/Sources/MainWindowController.swift mac/JawApp/Sources/Bench.swift \
    mac/JawHarness/Sources/Specimen.swift mac/JawHarness/Sources/Scenes/ScreenScenes.swift mac/Tests/JawUITests/SettingsTests.swift
git commit -F- --trailer "Co-authored-by: Cursor <cursoragent@cursor.com>" <<'EOF'
Settings leaves its sheet for a window of its own, as the owner asked on 6 November and the Settings and themes design has it: SettingsTabs, an NSTabViewController in its toolbar style in JawUI, shows Appearance, Messages and Panes as icon tabs, paintpalette, text.bubble and rectangle.split.2x1, each its SwiftUI form hosted at its size, 500 points wide, the window taking that size as its tab is chosen and keeping its top edge, and SettingsWindowController opens it with ⌘, and Jaw › Settings… on the pane last shown, centred over the main window the first time and where it was left after, closable and nothing else, in the app's appearance, with no Done since every change is kept as it's made, and ⌘W closing it before the menu bar's Close Pane can take it; the forms keep what they had, their explanations now their sections' footers; the palette's Choose a theme opens it on Appearance; --keys settings:<pane> opens it on a pane and --snapshot draws it with its toolbar; and the harness's With overlays scene loses its Settings band, Settings being over the window no more
EOF
```

`git rm` in Step 3 has already staged `SettingsView.swift`'s deletion, so this commit carries it.

---

### Task 2: Omarchy's themes mapped as the terminal maps them, held on the surface, and a role for links

**Files:**
- Create: `mac/JawThemes/Sources/Contrast.swift`, `mac/JawThemes/Sources/Omarchy.swift`
- Modify: `mac/JawThemes/Sources/Roles.swift` (`Roles`' accent at 21-22, `tokyoNight` at 83, `tokyoNightDay` at 109), `mac/JawUI/Sources/Layout/MessageLayout.swift` (line 445), `mac/JawUI/Sources/Components/Glyphs.swift` (`Ticks.of` at 38-47), `mac/JawUI/Sources/Library/LibraryDrawing.swift` (lines 315 and 325), `mac/JawUI/Sources/Layout/Cards.swift` (`linkPreview(_:)`'s bar, line 633), `mac/JawHarness/Sources/Scenes/FoundationScenes.swift` (`RolesScene`'s `named` and `textColours` at 29-43, and its second band's note at 63)
- Test: `mac/Tests/JawThemesTests/OmarchyTests.swift` (new), `mac/Tests/JawThemesTests/RolesTests.swift`, `mac/Tests/JawUITests/LinkPreviewTests.swift`

**Interfaces:**
- Consumes: `RGBA` (`init(_ hex: UInt32)`, `hex`, `opacity(_:)`, `over(_:)`, `contrast(with:)`), and `protocol/themes/*.toml`, read by the tests by path.
- Produces, in JawThemes:
  - `public var link: RGBA` in `Roles`, after `accent`: links and read ticks.
  - `public enum Contrast` with `static func luminance(_ c: UInt32) -> Double`, `static func ratio(_ a: UInt32, _ b: UInt32) -> Double`, `static func mix(_ a: UInt32, _ b: UInt32, _ t: Double) -> UInt32` and `static func fit(_ colour: UInt32, on ground: UInt32, toward end: UInt32, low: Double, high: Double = .infinity, target: Double) -> UInt32`.
  - `public enum Omarchy` with `static let keys: [String]` (the 25), `struct Terminal` (`bg`, `surface`, `selection`, `fg`, `text`, `muted`, `comment`, `gutter`, `accent`, `link`, `mention`, `online`, `notice`, `senders`, all `UInt32`), `static func terminal(_ c: [String: UInt32]) -> Terminal?`, `static func roles(_ colours: [String: String], dark: Bool) -> Roles?`, `static func hex(_ text: String) -> UInt32?` and `static func file(_ text: String) -> (dark: Bool, colours: [String: String])?`.
- Produces, in JawThemesTests, for Tasks 3 and 4: `themesFolder`, `omarchyFiles() throws -> [(id: String, dark: Bool, colours: [String: String])]` and `reads(_ r: Roles, _ name: String)`.

- [ ] **Step 1: Write the failing tests**

Create `mac/Tests/JawThemesTests/OmarchyTests.swift`:

```swift
import Foundation
@testable import JawThemes
import Testing

/// Omarchy's themes, read from `protocol/themes/` as jaw-proto reads them and mapped as `theme.rs`
/// maps them, then held on the Mac's surface: the port lands on the terminal's own colours, each theme
/// moves what `theme.rs` and the surface move and nothing else, and every theme reads.

let themesFolder = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent().appending(path: "protocol/themes")

/// Omarchy's themes as `protocol/themes/` holds them, by ID, each read as jaw-proto reads it.
func omarchyFiles() throws -> [(id: String, dark: Bool, colours: [String: String])] {
    let names = try FileManager.default.contentsOfDirectory(atPath: themesFolder.path).filter { $0.hasSuffix(".toml") }.sorted()
    return try names.map { name in
        let text = try String(contentsOf: themesFolder.appending(path: name), encoding: .utf8)
        let read = try #require(Omarchy.file(text), "\(name) reads")
        return (String(name.dropLast(5)), read.dark, read.colours)
    }
}

/// What every theme's roles read at, of every source and in each half: text at 4.5:1 wherever it's
/// drawn, every colour at 3:1 on the ground and on the surface, the accent on the ground, the dim text
/// in its band, and a selected row's online at 4.5:1 on its selection.
func reads(_ r: Roles, _ name: String) {
    for (ground, colour) in [("ground", r.ground), ("surface", r.surface), ("selection", r.selection), ("yours", r.yours)] {
        #expect(r.text.contrast(with: colour) >= 4.5, "\(name): text on the \(ground), \(r.text.contrast(with: colour))")
    }
    let colours = [("secondary", r.secondary), ("link", r.link), ("mention", r.mention), ("online", r.online), ("failed", r.failed)]
        + r.senders.map { ("a sender", $0) }
    for (role, colour) in colours {
        #expect(colour.contrast(with: r.ground) >= 3 && colour.contrast(with: r.surface) >= 3,
                "\(name): \(role) on the ground and on the surface")
    }
    #expect(r.accent.contrast(with: r.ground) >= 3, "\(name): the accent")
    #expect((2.5 ... 4.51).contains(r.tertiary.contrast(with: r.ground)), "\(name): dim text \(r.tertiary.contrast(with: r.ground))")
    #expect(r.selectedOnline.contrast(with: r.selection) >= 4.5, "\(name): a selected row's online")
}

@Test func `every theme file reads as jaw-proto reads it, with its mode, its 25 colours and the fallbacks it fills`() throws {
    let themes = try omarchyFiles()
    #expect(themes.count == 23)
    #expect(themes.filter { !$0.dark }.map { $0.id }.sorted() == ["catppuccin-latte", "flexoki-light", "lupine", "rose-pine", "white"])
    #expect(themes.allSatisfy { $0.colours.count == 25 })
    for (id, brown) in [("last-horizon", "#362f3a"), ("solitude", "#6d6e6e"), ("white", "#252525")] {
        let c = try #require(themes.first { $0.id == id }?.colours)
        #expect(c["orange"] == c["yellow"] && c["brown"] == brown, "\(id): orange is its yellow, and brown that mixed half with black")
    }
    let gruvbox = try #require(themes.first { $0.id == "gruvbox" }?.colours)
    #expect(gruvbox["orange"] != gruvbox["yellow"] && gruvbox["brown"] == "#70432e", "a theme's own orange and brown are kept")
    #expect(Omarchy.file("mode = \"dim\"\n") == nil, "a mode that isn't dark or light")
    #expect(Omarchy.file("mode = \"dark\"\naccent \"#ffffff\"\n") == nil, "a line that isn't key = value")
}

@Test func `the port lands on theme.rs's own Tokyo Night, grey by grey and colour by colour`() throws {
    let tokyo = try #require(omarchyFiles().first { $0.id == "tokyo-night" })
    let t = try #require(Omarchy.terminal(tokyo.colours.compactMapValues(Omarchy.hex)))
    #expect([t.bg, t.surface, t.selection, t.fg, t.text, t.muted, t.comment, t.gutter]
        == [0x1A1B26, 0x24283B, 0x2D3247, 0xC0CAF5, 0xA9B1D6, 0x8088B0, 0x565F89, 0x414868], "as theme.rs's own test pins them")
    #expect([t.accent, t.link, t.mention, t.senders[3], t.senders[7]] == [0x7AA2F7, 0x449DAB, 0xEB927B, 0xAD8EE6, 0x0DB9D7])
}

/// What `theme.rs` moves in each theme at Omarchy's 4c1b4cb, as its own test lists it.
let terminalMoves: [(moved: [String], themes: [String])] = [
    ([], ["catppuccin", "flexoki-light", "gruvbox", "hackerman", "lumon", "matte-black", "rose-pine"]),
    (["dim text raised"], ["catppuccin-latte", "everforest", "last-horizon", "miasma", "nord", "solitude", "white"]),
    (["borders lowered", "dim text lowered"], ["ethereal", "osaka-jade", "retro-82", "starship"]),
    (["borders lowered"], ["miasma", "ristretto", "vantablack", "white"]),
    (["surface raised"], ["last-horizon", "lupine", "solitude"]),
    (["selection raised"], ["kanagawa", "tokyo-night", "vantablack", "white"]),
]

@Test func `each theme moves the greys theme.rs moves, and no others`() throws {
    for theme in try omarchyFiles() {
        let c = theme.colours.compactMapValues(Omarchy.hex)
        let t = try #require(Omarchy.terminal(c))
        let greys: [(String, UInt32, UInt32, UInt32)] = [
            ("surface", c["lighter_background"]!, t.surface, t.bg), ("selection", c["selection"]!, t.selection, t.surface),
            ("dim text", c["dark_foreground"]!, t.comment, t.bg), ("borders", c["muted"]!, t.gutter, t.bg),
        ]
        var moved: [String] = []
        for (what, was, now, ground) in greys where now != was {
            moved.append("\(what) \(Contrast.ratio(now, ground) > Contrast.ratio(was, ground) ? "raised" : "lowered")")
        }
        let listed = terminalMoves.filter { $0.themes.contains(theme.id) }.flatMap { $0.moved }
        #expect(moved.sorted() == listed.sorted(), "\(theme.id)")
    }
}

/// What holding Omarchy's colours at 3:1 on the Mac's surface too moves, beyond what `theme.rs` moves,
/// at Omarchy's 4c1b4cb, as a port of the rule found on 6 November.
let surfaceMoves: [String: [String]] = [
    "catppuccin-latte": ["link", "online", "senders 5"],
    "flexoki-light": ["link", "mention", "online", "senders 6"],
    "kanagawa": ["failed", "senders 1"],
    "last-horizon": ["mention", "senders 2"],
    "matte-black": ["senders 1"],
    "miasma": ["failed", "mention", "senders 2"],
    "nord": ["failed", "senders 1"],
    "rose-pine": ["link", "mention", "senders 5"],
    "solitude": ["failed", "senders 1"],
]

@Test func `the Mac holds the terminal's colours at 3:1 on its surface too, which moves nine themes' and no others'`() throws {
    for theme in try omarchyFiles() {
        let c = theme.colours.compactMapValues(Omarchy.hex)
        let t = try #require(Omarchy.terminal(c))
        let r = try #require(Omarchy.roles(theme.colours, dark: theme.dark))
        let red = Contrast.fit(c["red"]!, on: t.bg, toward: t.fg, low: 3, target: 3)
        var moved: [String] = []
        for (what, was, now) in [("link", t.link, r.link), ("mention", t.mention, r.mention), ("online", t.online, r.online),
                                 ("failed", red, r.failed)] where now.hex != was
        {
            moved.append(what)
        }
        let senders = zip(t.senders, r.senders).count { $0.0 != $0.1.hex }
        if senders > 0 { moved.append("senders \(senders)") }
        #expect(moved.sorted() == (surfaceMoves[theme.id] ?? []).sorted(), "\(theme.id)")
        #expect(r.accent.hex == t.accent && r.ground.hex == t.bg && r.tertiary.hex == t.comment, "\(theme.id): the rest as the terminal has it")
    }
}

@Test func `every one of Omarchy's themes reads on the Mac, in the half its mode is`() throws {
    for theme in try omarchyFiles() {
        let roles = try #require(Omarchy.roles(theme.colours, dark: theme.dark), "\(theme.id) maps")
        #expect(roles.isDark == theme.dark)
        reads(roles, theme.id)
    }
    var missing = try #require(omarchyFiles().first).colours
    missing["accent"] = nil
    #expect(Omarchy.roles(missing, dark: true) == nil, "a theme without one of its colours isn't mapped")
}
```

In `mac/Tests/JawThemesTests/RolesTests.swift`, add to the end of the first test, `` `jaw's theme is the terminal's Tokyo Night by role` ``:

```swift
    #expect(dark.link == dark.accent && Catalog.jaw.light.link == Catalog.jaw.light.accent, "Tokyo Night's links are its blue, in both halves")
```

and at the end of the file:

```swift
/// Tokyo Night Day passes by a hair, as built: its red reads at 3.005:1 on its ground against a floor
/// of 3, and its dim text at 2.54:1 against 2.5, so any change to it, a rounding included, fails here.
@Test func `Jaw's Tokyo Night reads as every theme must, in both halves`() {
    reads(Catalog.jaw.dark, "Tokyo Night")
    reads(Catalog.jaw.light, "Tokyo Night Day")
}
```

At the end of `mac/Tests/JawUITests/LinkPreviewTests.swift`:

```swift
@Test func `links and read ticks are drawn in the link colour, not the accent`() throws {
    var roles = Catalog.jaw.dark
    roles.link = RGBA(0x449DAB)
    #expect(Ticks.of(.read, roles)?.1 == roles.link && Ticks.of(.played, roles)?.1 == roles.link)
    #expect(Ticks.of(.delivered, roles)?.1 == roles.tertiary)
    let s = LayoutSpec(style: Style(roles: roles), me: ["person:1"])
    let message = try #require(Samples.messages.first { $0.spans.contains { $0.t == .link } })
    let frame = try #require(laid(message, s).text?.frame)
    var colours: [CGColor] = []
    for line in CTFrameGetLines(frame) as! [CTLine] {
        for run in CTLineGetGlyphRuns(line) as! [CTRun] {
            let attributes = CTRunGetAttributes(run) as NSDictionary
            if attributes[NSAttributedString.Key.jawLink] != nil, let colour = attributes[NSAttributedString.Key.ctColour] {
                colours.append(colour as! CGColor)
            }
        }
    }
    #expect(!colours.isEmpty && colours.allSatisfy { $0 == roles.link.cgColor }, "a link's run is in the link colour")
    let history = laid(sampleLink("L2"), s)
    #expect(history.parts.contains { part in
        if case let .fill(rect, colour, _) = part { rect.width == 3 && colour == roles.link.cgColor } else { false }
    }, "a link's card with no thumbnail marks its edge in the link colour")
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawThemes`
Expected: exit 1, with `error: cannot find 'Omarchy' in scope` from `OmarchyTests.swift`.

- [ ] **Step 3: Write the port, the mapping and the role**

Create `mac/JawThemes/Sources/Contrast.swift`:

```swift
import Foundation

/// WCAG's contrast on colours as `0xRRGGBB`, and the straight line the terminal's `theme.rs` moves a
/// colour along until it reads, ported as it is: `mix` rounds each channel half away from zero, as
/// Rust's `round` does, and `fit` halves the line 30 times, so the Mac lands on the terminal's colours.
public enum Contrast {
    /// WCAG's relative luminance of a colour.
    public static func luminance(_ c: UInt32) -> Double {
        func channel(_ shift: UInt32) -> Double {
            let v = Double((c >> shift) & 0xFF) / 255
            return v <= 0.039_28 ? v / 12.92 : pow((v + 0.055) / 1.055, 2.4)
        }
        return 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    }

    /// WCAG's contrast between two colours, from 1 to 21.
    public static func ratio(_ a: UInt32, _ b: UInt32) -> Double {
        let (la, lb) = (luminance(a), luminance(b))
        return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
    }

    /// `a` moved `t` of the way to `b`, channel by channel.
    public static func mix(_ a: UInt32, _ b: UInt32, _ t: Double) -> UInt32 {
        func channel(_ shift: UInt32) -> UInt32 {
            let (x, y) = (Double((a >> shift) & 0xFF), Double((b >> shift) & 0xFF))
            return min(UInt32((x + (y - x) * t).rounded()), 255) << shift
        }
        return channel(16) | channel(8) | channel(0)
    }

    /// `colour` as it is when its contrast with `ground` is from `low` to `high`, and otherwise moved
    /// toward `end`, if it reads too little, or toward `ground`, if too much, until its contrast is
    /// `target`, so it keeps its hue.
    public static func fit(_ colour: UInt32, on ground: UInt32, toward end: UInt32, low: Double, high: Double = .infinity,
                           target: Double) -> UInt32
    {
        let now = ratio(colour, ground)
        if now >= low, now <= high { return colour }
        let goal = now < low ? end : ground
        var (near, far) = (0.0, 1.0)
        for _ in 0 ..< 30 {
            let t = (near + far) / 2
            let reached = ratio(mix(colour, goal, t), ground)
            if (now < low && reached < target) || (now > high && reached > target) {
                near = t
            } else {
                far = t
            }
        }
        return mix(colour, goal, far)
    }
}
```

Create `mac/JawThemes/Sources/Omarchy.swift`:

```swift
import Foundation

/// Omarchy's themes as jawd serves them (`themes`), mapped onto the Mac's roles as the terminal's
/// `theme.rs` maps them onto its own, then held at 3:1 against the surface too, where the Mac draws
/// their bubbles and its chat list, as the terminal holds its colours against the ground.
public enum Omarchy {
    /// The 25 colours jawd serves for each theme, by Omarchy's names for them.
    public static let keys = [
        "accent", "selection", "muted", "background", "dark_background", "darker_background", "lighter_background",
        "foreground", "dark_foreground", "light_foreground", "bright_foreground", "red", "yellow", "orange", "green", "cyan",
        "blue", "magenta", "brown", "bright_red", "bright_yellow", "bright_green", "bright_cyan", "bright_blue", "bright_magenta",
    ]

    /// The terminal's roles for a theme, as `theme.rs`'s `Roles` has them, by its names for them.
    public struct Terminal: Equatable, Sendable {
        public var bg, surface, selection, fg, text, muted, comment, gutter: UInt32
        public var accent, link, mention, online, notice: UInt32
        public var senders: [UInt32]
    }

    /// `theme.rs`'s `Roles::of`, ported as it is: a grey outside its band of contrast with the ground
    /// moved toward the ground or the text until it's in, keeping its hue; the selection then held
    /// apart from the surface too, which overlays draw it on; and every colour at least 3:1. Nil when
    /// one of the colours it reads is missing.
    public static func terminal(_ c: [String: UInt32]) -> Terminal? {
        guard let bg = c["background"], let bright = c["bright_foreground"], let plain = c["foreground"],
              let dim = c["dark_foreground"], let lighter = c["lighter_background"], let picked = c["selection"],
              let muted = c["muted"], let accent = c["accent"], let cyan = c["cyan"], let orange = c["orange"],
              let green = c["green"], let yellow = c["yellow"]
        else { return nil }
        let named = ["red", "yellow", "blue", "magenta", "cyan", "green", "orange", "bright_cyan"].compactMap { c[$0] }
        guard named.count == 8 else { return nil }
        let (fg, text) = Contrast.ratio(bright, bg) >= Contrast.ratio(plain, bg) ? (bright, plain) : (plain, bright)
        func held(_ colour: UInt32, _ low: Double, _ high: Double, _ target: Double) -> UInt32 {
            Contrast.fit(colour, on: bg, toward: fg, low: low, high: high, target: target)
        }
        func hue(_ colour: UInt32) -> UInt32 {
            held(colour, 3, .infinity, 3)
        }
        let comment = held(dim, 2.5, 4.5, 3)
        let surface = held(lighter, 1.05, .infinity, 1.1)
        let selection = held(picked, 1.15, .infinity, 1.25)
        return Terminal(
            bg: bg, surface: surface, selection: Contrast.fit(selection, on: surface, toward: fg, low: 1.15, target: 1.15),
            fg: fg, text: text, muted: Contrast.mix(comment, text, 0.5), comment: comment, gutter: held(muted, 1.4, 2.6, 1.8),
            accent: accent, link: hue(cyan), mention: hue(orange), online: hue(green), notice: hue(yellow), senders: named.map(hue),
        )
    }

    /// The Mac's roles for one of Omarchy's themes, from its colours by Omarchy's keys as `#rrggbb`:
    /// the terminal's, the Mac's own beside them (your bubbles, a selected row's online, failed), and
    /// each colour the terminal holds at 3:1 against the ground held so against the surface too. Nil
    /// when a colour is missing or isn't `#rrggbb`.
    public static func roles(_ colours: [String: String], dark: Bool) -> Roles? {
        var c: [String: UInt32] = [:]
        for key in keys {
            guard let value = colours[key].flatMap(hex) else { return nil }
            c[key] = value
        }
        guard let t = terminal(c), let red = c["red"] else { return nil }
        func surfaced(_ colour: UInt32) -> UInt32 {
            Contrast.fit(colour, on: t.surface, toward: t.fg, low: 3, target: 3)
        }
        let online = surfaced(t.online)
        return Roles(
            isDark: dark, ground: RGBA(t.bg), surface: RGBA(t.surface), selection: RGBA(t.selection), text: RGBA(t.fg),
            secondary: RGBA(t.muted), tertiary: RGBA(t.comment), separator: RGBA(t.gutter), accent: RGBA(t.accent),
            link: RGBA(surfaced(t.link)), yours: RGBA(t.accent).opacity(dark ? 0.18 : 0.12).over(RGBA(t.bg)),
            mention: RGBA(surfaced(t.mention)), online: RGBA(online),
            selectedOnline: RGBA(Contrast.fit(online, on: t.selection, toward: t.fg, low: 4.5, target: 4.5)),
            failed: RGBA(surfaced(Contrast.fit(red, on: t.bg, toward: t.fg, low: 3, target: 3))),
            senders: t.senders.map { RGBA(surfaced($0)) },
        )
    }

    /// `#rrggbb`, in either case, as `0xRRGGBB`.
    public static func hex(_ text: String) -> UInt32? {
        guard text.count == 7, text.hasPrefix("#"), text.dropFirst().allSatisfy(\.isHexDigit) else { return nil }
        return UInt32(text.dropFirst(), radix: 16)
    }

    /// A theme's `colors.toml`, as `protocol/themes/` holds Omarchy's, read as jaw-proto reads it: its
    /// mode, and its colours by Omarchy's keys as lowercase `#rrggbb`, with `orange` its `yellow` and
    /// `brown` its `orange` mixed half with black, each channel rounded half up, where it leaves them
    /// out. Nil when a line isn't `key = "value"`, the mode isn't `dark` or `light`, or a colour is
    /// missing. For the tests and the harness, which ask jawd for nothing; the app reads jawd's `themes`.
    public static func file(_ text: String) -> (dark: Bool, colours: [String: String])? {
        var values: [String: String] = [:]
        for raw in text.split(whereSeparator: \.isNewline) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.isEmpty || line.hasPrefix("#") { continue }
            guard let equals = line.firstIndex(of: "=") else { return nil }
            let key = line[..<equals].trimmingCharacters(in: .whitespaces)
            values[key] = line[line.index(after: equals)...].trimmingCharacters(in: .whitespaces)
                .trimmingCharacters(in: CharacterSet(charactersIn: "\""))
        }
        guard let mode = values["mode"], mode == "dark" || mode == "light" else { return nil }
        if values["orange"] == nil { values["orange"] = values["yellow"] }
        if values["brown"] == nil, let orange = values["orange"].flatMap(hex) {
            func half(_ shift: UInt32) -> UInt32 {
                ((((orange >> shift) & 0xFF) + 1) / 2) << shift
            }
            values["brown"] = String(format: "#%06x", half(16) | half(8) | half(0))
        }
        var colours: [String: String] = [:]
        for key in keys {
            guard let value = values[key].flatMap(hex) else { return nil }
            colours[key] = String(format: "#%06x", value)
        }
        return (mode == "dark", colours)
    }
}
```

In `mac/JawThemes/Sources/Roles.swift`, `Roles` gains `link` after `accent` (lines 21-22):

```swift
    /// Focus, selection, unread counts.
    public var accent: RGBA
    /// Links and read ticks: the accent in Jaw's and Redlamp's themes, the terminal's cyan in Omarchy's.
    public var link: RGBA
```

and both halves of Tokyo Night give it their blue: in `tokyoNight` (line 83) and in `tokyoNightDay` (line 109), `accent: blue,` becomes `accent: blue, link: blue,`. Its doc comment above `accent` said "Focus, selection, links, read ticks, unread counts."; it now says what's left, as above.

In `mac/JawUI/Sources/Layout/MessageLayout.swift`, a link span's colour, line 445, `a[.ctColour] = roles.accent.cgColor` under `case .link:`, becomes `a[.ctColour] = roles.link.cgColor`.

In `mac/JawUI/Sources/Components/Glyphs.swift`, `Ticks.of`:

```swift
    /// How many ticks a status has, and in which colour: one sent, two delivered, two in the link colour
    /// read; nil for the states said in words or symbols.
    public static func of(_ status: MessageStatus?, _ roles: Roles) -> (Int, RGBA)? {
        switch status {
        case .sent: (1, roles.tertiary)
        case .delivered: (2, roles.tertiary)
        case .read, .played: (2, roles.link)
        case .pending, .failed, nil: nil
        }
    }
```

In `mac/JawUI/Sources/Library/LibraryDrawing.swift`'s `link(_:icon:x:right:size:_:into:)`, the link's symbol (line 315) and its host (line 325) take `r.link` where they take `r.accent`: `symbol("link", 13, r.link, centredIn: icon, into: &d.under)` and `[.ctFont: small, .ctColour: r.link.cgColor]`.

In `mac/JawUI/Sources/Layout/Cards.swift`'s `linkPreview(_:)`, the bar a link's card draws at its edge when it has no thumbnail (line 633) is the link's mark, so it takes the link colour: `roles.accent.cgColor` there becomes `roles.link.cgColor`. The card's other colours stay: its title in the text colour and its host in the dim text.

In `mac/JawHarness/Sources/Scenes/FoundationScenes.swift`'s `RolesScene`, `("link", roles.link)` goes after `("accent", roles.accent)` in both `named` and `textColours`, and the second band's note begins "Secondary and tertiary text, the accent's counts, the links and read ticks, mentions, online and failed," where it began "Secondary and tertiary text, the accent's links and counts, mentions, online and failed,".

- [ ] **Step 4: Run the tests**

Run: `mac/scripts/build-mac test JawThemes`, then `mac/scripts/build-mac test JawUITests`, `mac/scripts/build-mac test JawHarnessTests` and `mac/scripts/build-mac build`; then `make check-parity`.
Expected: each exits 0. JawThemes runs six more tests than before, JawUITests one more, and every reference picture still matches: Jaw's Tokyo Night draws its links and read ticks in the blue it drew them in. If `surfaceMoves` differs from what the port finds, check `Omarchy.roles` against the design's table before changing the list, and say so in the report: it's what a port of the rule found on 6 November, and `theme.rs`'s own numbers, which the port must land on first, are the second test's.

- [ ] **Step 5: Commit**

```bash
git add mac/JawThemes/Sources/Contrast.swift mac/JawThemes/Sources/Omarchy.swift mac/JawThemes/Sources/Roles.swift \
    mac/JawUI/Sources/Layout/MessageLayout.swift mac/JawUI/Sources/Components/Glyphs.swift mac/JawUI/Sources/Library/LibraryDrawing.swift \
    mac/JawUI/Sources/Layout/Cards.swift mac/JawHarness/Sources/Scenes/FoundationScenes.swift mac/Tests/JawThemesTests \
    mac/Tests/JawUITests/LinkPreviewTests.swift
git commit -F- --trailer "Co-authored-by: Cursor <cursoragent@cursor.com>" <<'EOF'
Omarchy's themes map onto the Mac's roles as the terminal's theme.rs maps them onto its own, as the owner chose on 6 November: Contrast ports its contrast, its mix and its straight line toward the ground or the text on 0xRRGGBB colours as they are, so Tokyo Night lands on theme.rs's own pinned greys and colours and each theme moves the greys its test lists, and Omarchy maps a theme's 25 colours by their keys onto the Mac's roles, with your bubbles, a selected row's online and failed beside them, then holds each colour at 3:1 on the surface as well as the ground, readable over exact, which moves colours in nine of the 23 at Omarchy's 4c1b4cb, mostly senders', and reads a colors.toml as jaw-proto does for the tests and the harness; the Mac's roles gain link, the terminal's cyan in Omarchy's themes and the accent in Jaw's, so Kanagawa's links stand apart from its text, and the message layout, read ticks, a link card's bar and the library's links draw in it; every one of Omarchy's 23 reads in the half its mode is, as Jaw's Tokyo Night does
EOF
```

---

### Task 3: Redlamp's catalog, copied, mapped and kept in step

**Files:**
- Create: `mac/JawThemes/Sources/RedlampCatalog.swift`, `mac/JawThemes/Sources/RedlampRoles.swift`, `scripts/sync-themes`
- Test: `mac/Tests/JawThemesTests/RedlampTests.swift`

**Interfaces:**
- Consumes: `Contrast` (Task 2), `RGBA`, `Roles` with `link`, and `reads(_:_:)` from Task 2's tests.
- Produces, in JawThemes:
  - `public struct ThemePalette` (`background`, `card`, `foreground`, `muted`, `primary`, `accent`, `border`, `success`, `destructive`, `neutral`, all `RGBA`, and `primaryRoles: PrimaryRoles`) and `public struct PrimaryRoles: OptionSet` (`.accent`, `.selection`, `.editedDot`, `.all`), as Redlamp has them.
  - `public enum RedlampCatalog` with `public struct Family` (`id`, `name`, `dark`, `light`) and `public static let families: [Family]`, Redlamp's 21 families of ten colours in its order; `static func hsl(_ string: String) -> RGBA` and `enum Brand`, internal.
  - `RGBA`'s internal `init(red:green:blue:alpha:)`, `init(white:alpha:)`, `init(hue:saturation:lightness:alpha:)` and `mixed(with:amount:)`, Redlamp's spellings.
  - `public enum RedlampRoles` with `public static func roles(_ p: ThemePalette, dark: Bool) -> Roles` and `public static func neutral(dark: Bool, accent: RGBA) -> Roles`.
- Produces: `scripts/sync-themes [--check]`, which prints `sync-themes: jaw's copy is Redlamp's catalog at 9fa9d9155` and exits 0 when the copy is the pinned commit's, says what differs and exits 1 when it isn't, and with no checkout says so and exits 0; a checkout without the pinned commit, or without a `main`, stops it with exit 1, saying which revision to fetch there.

- [ ] **Step 1: Write the failing tests**

Create `mac/Tests/JawThemesTests/RedlampTests.swift`:

```swift
import Foundation
@testable import JawThemes
import Testing

/// Redlamp's catalog as jaw copies it, and jaw's roles from it: the families in Redlamp's order, the
/// Mac design's table, the same bands as Omarchy's, which move what a port of them found on
/// 6 November, the Redlamp family's red kept to its accent, and every half reading, Neutral's in the
/// system's blue.

@Test func `the copy holds Redlamp's 21 families of ten colours in its order, Neutral apart`() {
    #expect(RedlampCatalog.families.map { $0.id } == [
        "redlamp", "kanagawa", "tokyo-night", "moonlight", "catppuccin", "rose-pine", "nord", "everforest", "gruvbox", "dracula",
        "solarized", "one-dark", "monokai", "ocean", "sakura", "copper", "mc-vinyl-noir", "mc-neon-dusk", "mc-burnt-tape",
        "mc-phantom-frequency", "mc-velvet-static",
    ])
    #expect(Array(RedlampCatalog.families.map { $0.name }.suffix(5)) == ["Vinyl Noir", "Neon Dusk", "Burnt Tape", "Phantom Frequency", "Velvet Static"])
    #expect(RedlampCatalog.hsl("240 16% 14%").hex == 0x1E1E29, "Kanagawa's dark ground, as Redlamp parses it")
    #expect(RedlampCatalog.hsl("nonsense").hex == 0x000000, "a typo yields black, as in Redlamp")
    let redlamp = RedlampCatalog.families[0]
    #expect(redlamp.dark.background.hex == 0x0A0707 && redlamp.dark.primary.hex == 0xE0402E && redlamp.dark.primaryRoles == .accent)
    #expect(redlamp.light.background.hex == 0xF3EEE8 && redlamp.light.neutral.hex == 0x57504E, "steel, the light half's grey")
}

/// What holding Redlamp's halves in the bands moves, role by role, as a port of the rule found on
/// 6 November; every other half comes through as the Mac design's table maps it.
let redlampMoves: [String: [String]] = [
    "dim text": [
        "redlamp light", "kanagawa light", "tokyo-night dark", "tokyo-night light", "moonlight dark", "moonlight light",
        "catppuccin light", "rose-pine dark", "rose-pine light", "nord dark", "everforest dark", "everforest light", "gruvbox light",
        "dracula dark", "dracula light", "solarized dark", "solarized light", "one-dark dark", "one-dark light", "monokai dark",
        "monokai light", "ocean light", "sakura dark", "sakura light", "copper dark", "copper light", "mc-vinyl-noir dark",
        "mc-vinyl-noir light", "mc-neon-dusk dark", "mc-neon-dusk light", "mc-burnt-tape dark", "mc-burnt-tape light",
        "mc-phantom-frequency dark", "mc-phantom-frequency light", "mc-velvet-static dark", "mc-velvet-static light",
    ],
    "mention": [
        "kanagawa light", "tokyo-night light", "moonlight light", "catppuccin light", "rose-pine light", "nord light",
        "everforest light", "gruvbox light", "solarized light", "monokai light", "ocean light", "sakura light", "copper light",
        "mc-vinyl-noir light", "mc-burnt-tape light", "mc-phantom-frequency light", "mc-velvet-static light",
    ],
    "online": [
        "kanagawa light", "moonlight light", "catppuccin light", "nord light", "everforest light", "gruvbox light", "dracula light",
        "solarized light", "monokai light",
    ],
    "secondary": [
        "tokyo-night dark", "rose-pine light", "nord dark", "everforest dark", "everforest light", "dracula dark", "solarized dark",
        "one-dark dark", "monokai dark",
    ],
    "accent": ["tokyo-night light", "everforest light"],
    "failed": ["tokyo-night light", "moonlight light", "nord dark", "everforest light", "solarized dark"],
    "selection": ["everforest dark", "everforest light"],
    "yours": ["everforest light"],
]

@Test func `Redlamp's halves map by the Mac design's table, held by the bands, which leave five dark halves as they are`() {
    var moved: [String: [String]] = [:]
    var untouched: [String] = []
    for family in RedlampCatalog.families {
        for (half, p, dark) in [("dark", family.dark, true), ("light", family.light, false)] {
            let r = RedlampRoles.roles(p, dark: dark)
            let tint = p.primaryRoles.contains(.selection) ? p.primary : p.neutral
            let accent = p.primaryRoles.contains(.accent) ? p.primary : p.neutral
            let table: [(String, RGBA, RGBA)] = [
                ("dim text", p.muted.opacity(0.28 / 0.45).over(p.background), r.tertiary), ("mention", p.accent, r.mention),
                ("online", p.success, r.online), ("secondary", p.muted, r.secondary), ("accent", accent, r.accent),
                ("failed", p.destructive, r.failed), ("selection", tint.opacity(dark ? 0.2 : 0.16).over(p.card), r.selection),
                ("yours", tint.opacity(dark ? 0.18 : 0.12).over(p.background), r.yours),
            ]
            let name = "\(family.id) \(half)"
            let differ = table.filter { $0.1.hex != $0.2.hex }.map { $0.0 }
            for role in differ {
                moved[role, default: []].append(name)
            }
            if differ.isEmpty { untouched.append(name) }
            #expect(r.ground.hex == p.background.hex && r.surface.hex == p.card.hex && r.text.hex == p.foreground.hex, "\(name)")
            #expect(r.link == r.accent && r.separator.hex == p.border.hex, "\(name): links as the accent, borders as authored")
        }
    }
    #expect(untouched == ["redlamp dark", "kanagawa dark", "catppuccin dark", "gruvbox dark", "ocean dark"])
    for (role, halves) in redlampMoves {
        #expect(moved[role, default: []].sorted() == halves.sorted(), "\(role)")
    }
    #expect(Set(moved.keys) == Set(redlampMoves.keys))
}

@Test func `the Redlamp family keeps its red to the accent and links, its selection and your bubbles from its grey`() {
    let redlamp = RedlampCatalog.families[0]
    let dark = RedlampRoles.roles(redlamp.dark, dark: true)
    #expect(dark.accent.hex == 0xE0402E && dark.link.hex == 0xE0402E, "the safelight")
    #expect(dark.selection.hex == RedlampCatalog.Brand.ring.opacity(0.2).over(redlamp.dark.card).hex, "ring, not red")
    #expect(dark.yours.hex == RedlampCatalog.Brand.ring.opacity(0.18).over(redlamp.dark.background).hex)
}

@Test func `every half of Redlamp's reads, its senders at 4.5:1 on their bubble, and Neutral's in the system's blue`() {
    for family in RedlampCatalog.families {
        for (half, p, dark) in [("dark", family.dark, true), ("light", family.light, false)] {
            let r = RedlampRoles.roles(p, dark: dark)
            reads(r, "\(family.id) \(half)")
            #expect(r.senders.allSatisfy { $0.contrast(with: r.surface) >= 4.5 }, "\(family.id) \(half): senders on their bubble")
            #expect(r.isDark == dark)
        }
    }
    for (dark, accent) in [(true, RGBA(0x0A84FF)), (false, RGBA(0x007AFF))] {
        let r = RedlampRoles.neutral(dark: dark, accent: accent)
        reads(r, "neutral \(dark ? "dark" : "light")")
        #expect(r.ground.hex == (dark ? 0x1D1D1D : 0xECECEC), "Redlamp's panel grey")
        #expect(r.accent.hex == accent.hex, "the system's accent, which reads as it is")
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawThemes`
Expected: exit 1, with `error: cannot find 'RedlampCatalog' in scope` from `RedlampTests.swift`.

- [ ] **Step 3: Copy the catalog**

Create `mac/JawThemes/Sources/RedlampCatalog.swift`. What's between each pair of `// sync-themes:` markers is Redlamp's at 9fa9d91, line for line, which `scripts/sync-themes` checks; the families' lines are those of `ThemeCatalog.swift` from `// omoi-station classic palettes` to the last of its originals:

```swift
// Redlamp's theme catalog: the data of packages/RedlampDesign/Sources/Themes/ThemeCatalog.swift and the brand's colours
// in Tokens/Brand.swift, at Redlamp's 9fa9d915540782ce600ee6e4854dc3f2ccd15250, relicensed by its owner for jaw under
// jaw's MIT licence. scripts/sync-themes keeps each region between its `// sync-themes:` markers line for line as Redlamp has it.

import Foundation

/// A theme's ten colors, in the layout omoi-station's themes are authored in. jaw's roles come from
/// these by `RedlampRoles`.
public struct ThemePalette: Sendable, Hashable {
    public var background, card, foreground, muted, primary, accent, border, success, destructive, neutral: RGBA
    /// The tokens that take `primary`. The rest take `neutral`, so a theme whose primary
    /// must stay rare (the brand red) can keep it to one place.
    public var primaryRoles: PrimaryRoles

    public init(
        background: RGBA, card: RGBA, foreground: RGBA, muted: RGBA, primary: RGBA, accent: RGBA,
        border: RGBA, success: RGBA, destructive: RGBA, neutral: RGBA, primaryRoles: PrimaryRoles = .all,
    ) {
        self.background = background
        self.card = card
        self.foreground = foreground
        self.muted = muted
        self.primary = primary
        self.accent = accent
        self.border = border
        self.success = success
        self.destructive = destructive
        self.neutral = neutral
        self.primaryRoles = primaryRoles
    }
}

public struct PrimaryRoles: OptionSet, Sendable, Hashable {
    public let rawValue: Int

    public init(rawValue: Int) {
        self.rawValue = rawValue
    }

    public static let accent = PrimaryRoles(rawValue: 1 << 0)
    public static let selection = PrimaryRoles(rawValue: 1 << 1)
    public static let editedDot = PrimaryRoles(rawValue: 1 << 2)
    public static let all: PrimaryRoles = [.accent, .selection, .editedDot]
}

/// Redlamp's families of ten colours, each a dark and a light half, in Redlamp's slot order. Neutral,
/// which Redlamp authors as its own greys rather than ten colours, isn't among them:
/// `RedlampRoles.neutral` has it.
public enum RedlampCatalog {
    /// A family: its ID and its name, as Redlamp has them, and its two halves.
    public struct Family: Sendable, Hashable {
        public var id: String
        public var name: String
        public var dark: ThemePalette
        public var light: ThemePalette
    }

    public static let families: [Family] = [
        Family(id: "redlamp", name: "Redlamp", dark: redlampDark, light: redlampLight),
        // swiftformat:disable all
        // sync-themes: families
        // omoi-station classic palettes
        family("kanagawa", "Kanagawa",
               dark: pal("240 16% 14%", "240 16% 11%", "46 37% 77%", "258 13% 60%", "222 50% 67%", "34 100% 62%", "240 10% 26%", "88 32% 58%", "354 65% 65%", "258 13% 60%"),
               light: pal("40 40% 95%", "40 30% 92%", "240 16% 20%", "42 26% 46%", "222 50% 52%", "34 100% 48%", "40 15% 82%", "88 32% 45%", "0 83% 48%", "42 26% 46%")),
        family("tokyo-night", "Tokyo Night",
               dark: pal("230 19% 18%", "230 23% 16%", "226 84% 88%", "228 20% 44%", "217 92% 72%", "267 82% 75%", "230 16% 30%", "88 57% 61%", "355 75% 72%", "228 20% 44%"),
               light: pal("234 14% 89%", "233 11% 84%", "230 13% 20%", "230 18% 50%", "217 76% 55%", "266 87% 64%", "233 11% 78%", "90 34% 34%", "348 93% 56%", "230 18% 50%")),
        family("moonlight", "Moonlight",
               dark: pal("233 27% 17%", "233 25% 13%", "224 82% 88%", "228 29% 52%", "220 100% 75%", "19 100% 71%", "231 20% 30%", "85 65% 73%", "355 100% 73%", "228 29% 52%"),
               light: pal("235 25% 92%", "235 20% 88%", "230 23% 14%", "231 15% 45%", "224 70% 50%", "19 100% 60%", "235 15% 80%", "155 50% 38%", "355 90% 55%", "231 15% 50%")),
        family("catppuccin", "Catppuccin",
               dark: pal("240 21% 15%", "240 21% 12%", "226 64% 88%", "228 17% 63%", "267 84% 81%", "23 92% 75%", "237 12% 31%", "115 54% 76%", "351 74% 73%", "228 17% 63%"),
               light: pal("220 23% 95%", "220 22% 92%", "234 16% 35%", "233 10% 47%", "266 85% 58%", "22 99% 52%", "225 14% 77%", "109 58% 40%", "347 87% 44%", "228 11% 53%")),
        family("rose-pine", "Rosé Pine",
               dark: pal("249 22% 12%", "249 14% 15%", "245 50% 91%", "253 10% 47%", "343 76% 68%", "35 91% 72%", "250 12% 25%", "197 49% 38%", "343 76% 68%", "253 10% 47%"),
               light: pal("33 54% 95%", "33 100% 97%", "248 19% 40%", "257 7% 62%", "343 35% 55%", "35 84% 56%", "25 25% 88%", "197 50% 33%", "343 35% 55%", "257 7% 62%")),
        family("nord", "Nord",
               dark: pal("220 16% 22%", "220 16% 26%", "219 28% 88%", "220 16% 55%", "213 32% 63%", "193 43% 67%", "220 16% 36%", "92 28% 65%", "354 42% 56%", "220 16% 55%"),
               light: pal("220 16% 94%", "220 16% 90%", "220 16% 22%", "220 16% 36%", "213 32% 52%", "92 28% 65%", "220 16% 82%", "92 28% 50%", "354 42% 56%", "220 16% 50%")),
        family("everforest", "Everforest",
               dark: pal("192 15% 19%", "192 12% 24%", "38 23% 74%", "120 5% 50%", "95 35% 63%", "24 74% 68%", "195 10% 33%", "95 35% 63%", "359 73% 70%", "120 5% 50%"),
               light: pal("47 55% 94%", "48 38% 90%", "195 12% 40%", "110 8% 55%", "92 99% 32%", "24 93% 55%", "45 18% 84%", "92 99% 32%", "1 92% 65%", "110 8% 55%")),
        family("gruvbox", "Gruvbox",
               dark: pal("0 0% 16%", "20 5% 22%", "40 57% 81%", "35 16% 59%", "27 99% 55%", "42 95% 58%", "27 10% 36%", "64 65% 44%", "6 96% 59%", "0 7% 52%"),
               light: pal("47 76% 93%", "44 68% 90%", "0 0% 16%", "30 12% 42%", "24 88% 45%", "42 95% 58%", "33 30% 76%", "60 73% 35%", "2 75% 46%", "0 7% 52%")),
        family("dracula", "Dracula",
               dark: pal("231 15% 18%", "232 14% 22%", "60 30% 96%", "225 27% 51%", "265 89% 78%", "326 100% 74%", "232 14% 31%", "135 94% 65%", "0 100% 67%", "225 27% 51%"),
               light: pal("231 40% 96%", "231 30% 93%", "231 15% 18%", "225 20% 45%", "265 89% 55%", "326 80% 52%", "231 20% 83%", "135 60% 38%", "0 100% 50%", "225 20% 50%")),
        family("solarized", "Solarized",
               dark: pal("193 100% 11%", "192 81% 14%", "180 7% 73%", "194 14% 40%", "205 82% 48%", "45 100% 35%", "192 40% 20%", "68 100% 30%", "1 71% 52%", "194 14% 40%"),
               light: pal("44 87% 94%", "44 44% 90%", "192 81% 14%", "194 14% 40%", "205 82% 48%", "45 100% 35%", "44 25% 83%", "68 100% 30%", "1 71% 52%", "194 14% 40%")),
        family("one-dark", "One Dark",
               dark: pal("220 13% 18%", "220 14% 15%", "219 14% 71%", "220 14% 45%", "207 82% 66%", "286 60% 67%", "220 13% 28%", "95 38% 62%", "355 65% 65%", "220 14% 45%"),
               light: pal("230 8% 97%", "230 6% 93%", "230 8% 24%", "230 6% 50%", "222 87% 55%", "301 62% 40%", "230 5% 83%", "120 34% 42%", "4 72% 55%", "230 6% 50%")),
        family("monokai", "Monokai",
               dark: pal("70 8% 15%", "70 8% 11%", "60 30% 96%", "50 11% 41%", "338 95% 56%", "80 76% 53%", "70 6% 25%", "80 76% 53%", "338 95% 56%", "50 11% 41%"),
               light: pal("60 10% 96%", "60 8% 93%", "70 8% 15%", "50 11% 41%", "338 95% 45%", "80 76% 42%", "50 8% 82%", "80 76% 38%", "338 95% 45%", "50 11% 45%")),
        family("ocean", "Ocean",
               dark: pal("222 47% 11%", "217 33% 17%", "214 32% 91%", "215 20% 65%", "170 70% 59%", "38 92% 50%", "217 15% 28%", "160 60% 55%", "0 91% 71%", "215 20% 55%"),
               light: pal("170 50% 98%", "170 35% 95%", "215 28% 17%", "215 16% 47%", "174 61% 31%", "38 92% 50%", "170 15% 85%", "160 60% 35%", "0 84% 60%", "215 16% 47%")),
        family("sakura", "Sakura",
               dark: pal("260 20% 12%", "260 15% 16%", "20 30% 88%", "260 8% 50%", "340 68% 69%", "120 25% 65%", "260 10% 27%", "120 30% 60%", "0 80% 65%", "260 10% 55%"),
               light: pal("20 60% 98%", "20 35% 95%", "260 15% 15%", "260 8% 50%", "340 55% 55%", "120 25% 56%", "20 15% 85%", "120 30% 45%", "0 70% 55%", "260 8% 55%")),
        family("copper", "Copper",
               dark: pal("0 0% 11%", "0 0% 15%", "30 18% 82%", "20 6% 50%", "28 59% 60%", "0 30% 63%", "0 0% 25%", "140 35% 55%", "0 72% 62%", "20 6% 55%"),
               light: pal("30 24% 95%", "30 18% 91%", "0 0% 12%", "20 6% 45%", "28 60% 47%", "0 30% 63%", "30 12% 80%", "140 35% 40%", "0 65% 50%", "20 6% 50%")),
        // omoi-station originals
        family("mc-vinyl-noir", "Vinyl Noir",
               dark: pal("30 8% 10%", "30 6% 14%", "35 18% 78%", "30 10% 48%", "35 55% 58%", "42 40% 62%", "30 6% 22%", "145 25% 55%", "10 55% 58%", "30 10% 48%"),
               light: pal("35 20% 95%", "35 15% 91%", "30 5% 15%", "30 8% 45%", "32 45% 42%", "42 35% 50%", "35 10% 80%", "145 25% 40%", "10 50% 48%", "30 8% 48%")),
        family("mc-neon-dusk", "Neon Dusk",
               dark: pal("250 15% 12%", "250 12% 16%", "260 15% 80%", "260 8% 48%", "320 40% 65%", "175 35% 58%", "250 8% 26%", "160 30% 55%", "350 45% 62%", "260 8% 50%"),
               light: pal("260 12% 96%", "260 10% 93%", "250 10% 18%", "260 6% 48%", "320 35% 48%", "175 30% 42%", "260 6% 82%", "160 25% 42%", "350 40% 48%", "260 6% 50%")),
        family("mc-burnt-tape", "Burnt Tape",
               dark: pal("20 15% 10%", "20 12% 14%", "30 15% 76%", "20 8% 46%", "18 50% 56%", "38 45% 58%", "20 8% 22%", "150 22% 52%", "5 50% 55%", "20 8% 48%"),
               light: pal("30 30% 94%", "30 22% 90%", "20 12% 18%", "20 8% 48%", "16 45% 42%", "38 40% 48%", "28 12% 80%", "150 22% 42%", "5 45% 45%", "20 8% 50%")),
        family("mc-phantom-frequency", "Phantom Frequency",
               dark: pal("220 18% 11%", "220 15% 15%", "210 18% 78%", "215 10% 46%", "240 28% 65%", "168 30% 55%", "220 10% 24%", "155 25% 55%", "345 40% 60%", "215 10% 48%"),
               light: pal("210 20% 96%", "210 15% 93%", "220 15% 18%", "215 10% 48%", "240 22% 48%", "168 25% 48%", "210 10% 82%", "155 22% 42%", "345 35% 48%", "215 10% 50%")),
        family("mc-velvet-static", "Velvet Static",
               dark: pal("280 15% 10%", "280 12% 14%", "300 12% 80%", "290 8% 46%", "335 35% 62%", "45 35% 62%", "280 8% 24%", "155 22% 55%", "350 45% 60%", "290 8% 48%"),
               light: pal("300 10% 96%", "300 8% 93%", "280 10% 16%", "290 6% 48%", "330 28% 42%", "45 30% 52%", "300 5% 82%", "155 20% 42%", "350 40% 45%", "290 6% 48%")),
        // sync-themes: end
        // swiftformat:enable all
    ]

    // MARK: - Redlamp

    // The brand's room: warm near-black, paper text, steel edges. The red is kept to the accent, since
    // the brand allows it once per view (Redlamp's docs/brand/README.md).
    // sync-themes: redlamp
    static let redlampDark = ThemePalette(
        background: Brand.wall, card: Brand.bakelite, foreground: Brand.paper,
        muted: Brand.ring.mixed(with: Brand.steel, amount: 0.5),
        primary: Brand.safelight, accent: Brand.filament, border: Brand.steel,
        success: hsl("95 22% 52%"), destructive: hsl("6 40% 50%"), neutral: Brand.ring,
        primaryRoles: .accent,
    )

    static let redlampLight = ThemePalette(
        background: Brand.paper, card: Brand.paper.mixed(with: Brand.ink, amount: 0.04), foreground: Brand.ink,
        muted: Brand.steel.mixed(with: Brand.paper, amount: 0.3), primary: Brand.safelightOnLight,
        accent: Brand.safelight, border: Brand.ring,
        success: hsl("95 25% 38%"), destructive: hsl("6 45% 40%"), neutral: Brand.steel,
        primaryRoles: .accent,
    )
    // sync-themes: end

    /// Redlamp's brand colours, which the Redlamp family is made of.
    enum Brand {
        // sync-themes: brand
        static let safelight = RGBA(red: 224 / 255, green: 64 / 255, blue: 46 / 255)
        static let safelightOnLight = RGBA(red: 216 / 255, green: 53 / 255, blue: 42 / 255)
        static let filament = RGBA(red: 255 / 255, green: 176 / 255, blue: 138 / 255)
        static let wall = RGBA(red: 10 / 255, green: 7 / 255, blue: 7 / 255)
        static let bakelite = RGBA(red: 34 / 255, green: 28 / 255, blue: 26 / 255)
        static let steel = RGBA(red: 87 / 255, green: 80 / 255, blue: 78 / 255)
        static let ring = RGBA(red: 217 / 255, green: 208 / 255, blue: 203 / 255)
        static let paper = RGBA(red: 243 / 255, green: 238 / 255, blue: 232 / 255)
        static let ink = RGBA(red: 26 / 255, green: 20 / 255, blue: 20 / 255)
        // sync-themes: end
    }

    // MARK: - Builder

    private static func family(_ id: String, _ name: String, dark: ThemePalette, light: ThemePalette) -> Family {
        Family(id: id, name: name, dark: dark, light: light)
    }

    private static func pal(
        _ background: String, _ card: String, _ foreground: String, _ muted: String, _ primary: String,
        _ accent: String, _ border: String, _ success: String, _ destructive: String, _ neutral: String,
    ) -> ThemePalette {
        ThemePalette(
            background: hsl(background), card: hsl(card), foreground: hsl(foreground), muted: hsl(muted),
            primary: hsl(primary), accent: hsl(accent), border: hsl(border), success: hsl(success),
            destructive: hsl(destructive), neutral: hsl(neutral),
        )
    }

    /// Parses `"H S% L%"`. The palettes are authored data, so malformed input only guards
    /// typos and yields black.
    static func hsl(_ string: String) -> RGBA {
        let parts = string.split(whereSeparator: { $0 == " " || $0 == "%" }).compactMap { Double($0) }
        guard parts.count >= 3 else { return RGBA(white: 0) }
        return RGBA(hue: parts[0], saturation: parts[1] / 100, lightness: parts[2] / 100)
    }
}

extension RGBA {
    /// From its channels, as Redlamp writes its brand's colours.
    init(red: Double, green: Double, blue: Double, alpha: Double = 1) {
        self.init(r: red, g: green, b: blue, a: alpha)
    }

    /// A grey.
    init(white: Double, alpha: Double = 1) {
        self.init(r: white, g: white, b: white, a: alpha)
    }

    /// Hue in degrees (wrapped), saturation and lightness 0...1: the HSL model themes are
    /// authored in.
    init(hue degrees: Double, saturation: Double, lightness: Double, alpha: Double = 1) {
        let hue = ((degrees.truncatingRemainder(dividingBy: 360)) + 360).truncatingRemainder(dividingBy: 360)
        let a = saturation * min(lightness, 1 - lightness)
        func channel(_ n: Double) -> Double {
            let k = (n + hue / 30).truncatingRemainder(dividingBy: 12)
            return lightness - a * max(-1, min(k - 3, 9 - k, 1))
        }
        self.init(r: channel(0), g: channel(8), b: channel(4), a: alpha)
    }

    /// `amount` of the way to `other`, as Redlamp writes it.
    func mixed(with other: RGBA, amount: Double) -> RGBA {
        mixed(with: other, amount)
    }
}
```

- [ ] **Step 4: Map it**

Create `mac/JawThemes/Sources/RedlampRoles.swift`:

```swift
import Foundation

/// jaw's roles from Redlamp's families, by the Mac design's table, then held as Omarchy's are: every
/// colour at 3:1 against the ground and the surface, secondary text too, the dim text at 2.5 to 4.5:1,
/// and the selection and your bubbles moved toward their ground until text reads at 4.5:1 on them, so
/// a family made for a photo editor's panels reads in a chat, readable over exact (Owner, 6 Nov).
public enum RedlampRoles {
    /// A family's half: `primary`, or `neutral` where `primaryRoles` keeps it out, takes the accent and
    /// links, the selection at 20% over `card` (16% in light) and your bubbles at 18% over `background`
    /// (12%); `accent` takes mentions, `success` online and `destructive` failed; the senders are eight
    /// hues 45° apart from `primary`'s.
    public static func roles(_ p: ThemePalette, dark: Bool) -> Roles {
        let tint = p.primaryRoles.contains(.selection) ? p.primary : p.neutral
        return held(
            dark: dark, ground: p.background, surface: p.card, text: p.foreground,
            selection: tint.opacity(dark ? 0.2 : 0.16).over(p.card), yours: tint.opacity(dark ? 0.18 : 0.12).over(p.background),
            secondary: p.muted, tertiary: p.muted.opacity(0.28 / 0.45).over(p.background), separator: p.border,
            accent: p.primaryRoles.contains(.accent) ? p.primary : p.neutral,
            mention: p.accent, online: p.success, failed: p.destructive, hue: p.primary,
        )
    }

    /// Neutral, Redlamp's own grey: its standard tokens' greys at their alphas over its panel, white
    /// over 11.5% grey in dark and black over 92.5% in light; its accent the system's, which Redlamp's
    /// Neutral leaves its accent to; and the system's orange, green and red for mentions, online and
    /// failed, as macOS draws them in each appearance, held as a family's are, its senders' hues from
    /// the accent's.
    public static func neutral(dark: Bool, accent: RGBA) -> Roles {
        let ink = dark ? RGBA(0xFFFFFF) : RGBA(0x000000)
        let ground = dark ? RGBA(r: 0.115, g: 0.115, b: 0.115) : RGBA(r: 0.925, g: 0.925, b: 0.925)
        let surface = RGBA(0xFFFFFF).opacity(dark ? 0.05 : 0.55).over(ground)
        return held(
            dark: dark, ground: ground, surface: surface, text: ink.opacity(dark ? 0.9 : 0.88).over(ground),
            selection: ink.opacity(dark ? 0.1 : 0.08).over(surface), yours: accent.opacity(dark ? 0.18 : 0.12).over(ground),
            secondary: ink.opacity(dark ? 0.45 : 0.5).over(ground), tertiary: ink.opacity(dark ? 0.28 : 0.3).over(ground),
            separator: ink.opacity(dark ? 0.07 : 0.09).over(ground), accent: accent,
            mention: dark ? RGBA(0xFF9F0A) : RGBA(0xFF9500), online: dark ? RGBA(0x30D158) : RGBA(0x34C759),
            failed: dark ? RGBA(0xFF453A) : RGBA(0xFF3B30), hue: accent,
        )
    }

    /// The roles from what a half gives each, held: colours at 3:1 on the ground and then the surface,
    /// the dim text in its band, the selection and your bubbles under text, a selected row's online at
    /// 4.5:1 on its selection, links as the accent and the borders as authored.
    static func held(dark: Bool, ground: RGBA, surface: RGBA, text: RGBA, selection: RGBA, yours: RGBA, secondary: RGBA,
                     tertiary: RGBA, separator: RGBA, accent: RGBA, mention: RGBA, online: RGBA, failed: RGBA, hue: RGBA) -> Roles
    {
        let (g, s, t) = (ground.hex, surface.hex, text.hex)
        func both(_ c: RGBA) -> UInt32 {
            Contrast.fit(Contrast.fit(c.hex, on: g, toward: t, low: 3, target: 3), on: s, toward: t, low: 3, target: 3)
        }
        let picked = under(t, selection.hex, toward: s)
        let on = both(online)
        let a = RGBA(both(accent))
        return Roles(
            isDark: dark, ground: RGBA(g), surface: RGBA(s), selection: RGBA(picked), text: RGBA(t), secondary: RGBA(both(secondary)),
            tertiary: RGBA(Contrast.fit(tertiary.hex, on: g, toward: t, low: 2.5, high: 4.5, target: 3)), separator: RGBA(separator.hex),
            accent: a, link: a, yours: RGBA(under(t, yours.hex, toward: g)), mention: RGBA(both(mention)), online: RGBA(on),
            selectedOnline: RGBA(Contrast.fit(on, on: picked, toward: t, low: 4.5, target: 4.5)), failed: RGBA(both(failed)),
            senders: senders(from: hue, on: s, dark: dark),
        )
    }

    /// `fill` moved toward `ground` until `text` reads at 4.5:1 on it.
    static func under(_ text: UInt32, _ fill: UInt32, toward ground: UInt32) -> UInt32 {
        guard Contrast.ratio(text, fill) < 4.5 else { return fill }
        var (near, far) = (0.0, 1.0)
        for _ in 0 ..< 30 {
            let t = (near + far) / 2
            if Contrast.ratio(text, Contrast.mix(fill, ground, t)) < 4.5 {
                near = t
            } else {
                far = t
            }
        }
        return Contrast.mix(fill, ground, far)
    }

    /// Eight hues 45° apart from `primary`'s, its saturation and lightness kept, each lightened in dark
    /// or darkened in light half a percent at a time until it reads at 4.5:1 on `surface`.
    static func senders(from primary: RGBA, on surface: UInt32, dark: Bool) -> [RGBA] {
        let (hue, saturation, lightness) = hsl(primary)
        return (0 ..< 8).map { k in
            var l = lightness
            var colour = RGBA(hue: hue + Double(k) * 45, saturation: saturation, lightness: l).hex
            var steps = 0
            while Contrast.ratio(colour, surface) < 4.5, steps < 200 {
                l = min(max(l + (dark ? 0.005 : -0.005), 0), 1)
                colour = RGBA(hue: hue + Double(k) * 45, saturation: saturation, lightness: l).hex
                steps += 1
            }
            return RGBA(colour)
        }
    }

    /// A colour's hue in degrees, its saturation and its lightness, as HSL has them.
    static func hsl(_ c: RGBA) -> (hue: Double, saturation: Double, lightness: Double) {
        let (high, low) = (max(c.r, c.g, c.b), min(c.r, c.g, c.b))
        let lightness = (high + low) / 2
        guard high > low else { return (0, 0, lightness) }
        let d = high - low
        let saturation = lightness > 0.5 ? d / (2 - high - low) : d / (high + low)
        let hue: Double = switch high {
        case c.r: (c.g - c.b) / d + (c.g < c.b ? 6 : 0)
        case c.g: (c.b - c.r) / d + 2
        default: (c.r - c.g) / d + 4
        }
        return (hue * 60, saturation, lightness)
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `mac/scripts/build-mac test JawThemes`
Expected: exit 0, `** TEST SUCCEEDED **`, with four more tests than Task 2 left. If `redlampMoves` or the five untouched halves differ from what the port finds, check `RedlampRoles` against the design's table and bands before changing the lists, and say so in the report.

- [ ] **Step 6: Write `scripts/sync-themes`**

Create `scripts/sync-themes`, and make it executable with `chmod +x scripts/sync-themes`:

```python
#!/usr/bin/env python3
"""Keeps jaw's copy of Redlamp's theme catalog in step with Redlamp's own.

    scripts/sync-themes            # copy the catalog's data from Redlamp at the commit pinned below
    scripts/sync-themes --check    # say what differs, failing if jaw's copy isn't the pinned commit's

mac/JawThemes/Sources/RedlampCatalog.swift holds the data of Redlamp's catalog, its owner having
relicensed it for jaw under jaw's MIT licence on 6 November 2026: between its `// sync-themes: <region>`
markers, line for line as Redlamp has it, its families' ten HSL colours (`families`), the Redlamp
family's two palettes (`redlamp`) and the brand's nine colours they're made of (`brand`). Redlamp's
repository is private, so this reads a checkout of it, ~/src/darkroom or the folder REDLAMP names, with
`git show` at REDLAMP_COMMIT, never checking anything out, fetching or writing there; with no checkout,
as in CI, it says so and checks nothing. --check also says what the checkout's own main has changed in
those files since the pinned commit, Neutral's tokens in Tokens/Palette.swift among them, which jaw's
RedlampRoles.neutral is written from by hand. To move to a newer Redlamp, change REDLAMP_COMMIT, run
this and look at what changed: a family added or gone changes the gallery and the tests that list
Redlamp's families.
"""

import argparse
import difflib
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Optional

REDLAMP_COMMIT = "9fa9d915540782ce600ee6e4854dc3f2ccd15250"
ROOT = Path(__file__).resolve().parent.parent
COPY = ROOT / "mac" / "JawThemes" / "Sources" / "RedlampCatalog.swift"
CATALOG = "packages/RedlampDesign/Sources/Themes/ThemeCatalog.swift"
BRAND = "packages/RedlampDesign/Sources/Tokens/Brand.swift"
PALETTE = "packages/RedlampDesign/Sources/Tokens/Palette.swift"
REGIONS = ("families", "redlamp", "brand")
MARKED = r"(^[ ]*// sync-themes: {}\n)(.*?)(^[ ]*// sync-themes: end$)"


def checkout() -> Path:
    return Path(os.environ.get("REDLAMP") or Path.home() / "src" / "darkroom")


def show(where: Path, rev: str, path: str) -> str:
    """A file of Redlamp's at `rev`, read without touching the checkout; a checkout without `rev`
    stops this, saying what to fetch there, which this never does itself."""
    try:
        return subprocess.run(
            ["git", "-C", str(where), "show", f"{rev}:{path}"],
            check=True,
            capture_output=True,
            text=True,
            env={**os.environ, "GIT_OPTIONAL_LOCKS": "0"},
        ).stdout
    except subprocess.CalledProcessError as e:
        sys.exit(f"sync-themes: {where} has no {path} at {rev}, so fetch {rev} there first ({e.stderr.strip()})")


def families(catalog: str) -> list[str]:
    """The families' lines, from omoi-station's classic palettes to the last of its originals."""
    lines = catalog.splitlines()
    start = next(i for i, line in enumerate(lines) if line.strip() == "// omoi-station classic palettes")
    end = next(i for i, line in enumerate(lines) if i > start and line.strip() == "// swiftlint:enable line_length")
    return lines[start:end]


def palettes(catalog: str) -> list[str]:
    """The Redlamp family's two palettes, as written, with a blank line between them."""
    blocks = re.findall(r"^    static let redlamp(?:Dark|Light) = ThemePalette\(\n.*?^    \)$", catalog, re.S | re.M)
    if len(blocks) != 2:
        sys.exit("sync-themes: Redlamp's catalog doesn't have the Redlamp family's two palettes where it did")
    return "\n\n".join(blocks).splitlines()


def brand(text: str) -> list[str]:
    """The brand's colours, one line each, as jaw's copy nests them."""
    return [f"        static let {name} = {value}" for name, value in re.findall(r"static let (\w+) = (RGBA\(red: [^)]*\))", text)]


def neutral(palette: str) -> Optional[str]:
    """Redlamp's standard tokens, dark and light, which Neutral is; None where they aren't written as
    they were, so a reshaped Palette.swift isn't read as Neutral unchanged."""
    found = re.search(
        r"public static let standard = PaletteTokens\(.*?\n    \)\n\n.*?public static let standardLight = PaletteTokens\(.*?\n    \)",
        palette,
        re.S,
    )
    return found.group(0) if found else None


def wanted(where: Path, rev: str) -> dict[str, list[str]]:
    catalog = show(where, rev, CATALOG)
    return {"families": families(catalog), "redlamp": palettes(catalog), "brand": brand(show(where, rev, BRAND))}


def present(copy: str) -> dict[str, list[str]]:
    out = {}
    for name in REGIONS:
        found = re.search(MARKED.format(name), copy, re.S | re.M)
        if not found:
            sys.exit(f"sync-themes: {COPY.relative_to(ROOT)} has no `// sync-themes: {name}` region")
        out[name] = found.group(2).rstrip("\n").splitlines()
    return out


def written(copy: str, regions: dict[str, list[str]]) -> str:
    for name, lines in regions.items():
        copy = re.sub(
            MARKED.format(name),
            lambda m, lines=lines: m.group(1) + "\n".join(lines) + "\n" + m.group(3),
            copy,
            count=1,
            flags=re.S | re.M,
        )
    return copy


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="say what differs; fail if jaw's copy isn't the pinned commit's")
    args = parser.parse_args()
    where = checkout()
    if not (where / ".git").exists():
        print(f"sync-themes: no checkout of Redlamp at {where}, so nothing is checked (REDLAMP names another)")
        return
    pinned = wanted(where, REDLAMP_COMMIT)
    copy = COPY.read_text()
    if not args.check:
        COPY.write_text(written(copy, pinned))
        print(f"sync-themes: {COPY.relative_to(ROOT)} has Redlamp's catalog at {REDLAMP_COMMIT[:9]}")
        return
    have = present(copy)
    differs = [name for name in REGIONS if have[name] != pinned[name]]
    for name in differs:
        print(f"sync-themes: {name} differs from Redlamp's at {REDLAMP_COMMIT[:9]}:")
        sys.stdout.writelines(difflib.unified_diff([l + "\n" for l in pinned[name]], [l + "\n" for l in have[name]], "redlamp", "jaw"))
    moved = wanted(where, "main")
    for name in REGIONS:
        if moved[name] != pinned[name]:
            print(f"sync-themes: Redlamp's main has changed {name} since {REDLAMP_COMMIT[:9]}: move REDLAMP_COMMIT to take it")
    now, then = neutral(show(where, "main", PALETTE)), neutral(show(where, REDLAMP_COMMIT, PALETTE))
    if now is None or then is None:
        print("sync-themes: Neutral's tokens aren't written in Redlamp's Tokens/Palette.swift as they were: look at RedlampRoles.neutral")
    elif now != then:
        print(f"sync-themes: Redlamp's main has changed Neutral's tokens since {REDLAMP_COMMIT[:9]}: look at RedlampRoles.neutral")
    if not differs:
        print(f"sync-themes: jaw's copy is Redlamp's catalog at {REDLAMP_COMMIT[:9]}")
    sys.exit(1 if differs else 0)


if __name__ == "__main__":
    main()
```

- [ ] **Step 7: Try it**

It reads Redlamp's checkout with `git show`, which changes nothing there, and builds nothing, so it runs here.

Run: `scripts/sync-themes --check`
Expected: `sync-themes: jaw's copy is Redlamp's catalog at 9fa9d9155`, exit 0, and nothing about Redlamp's main, whose `main` was at 9fa9d91 on 6 November; if it has moved since, it says which regions, which is a note, not a failure.

Keep the copy as Step 3 wrote it, to compare with after the round trip: `mkdir -p mac/build/settings && cp mac/JawThemes/Sources/RedlampCatalog.swift mac/build/settings/RedlampCatalog.swift`. The file isn't committed yet, so `git diff` would show nothing whatever `sync-themes` wrote.

Run: `sed -i '' 's/"240 16% 14%", "240 16% 11%"/"240 16% 15%", "240 16% 11%"/' mac/JawThemes/Sources/RedlampCatalog.swift && scripts/sync-themes --check; echo "exit $?"`
Expected: `sync-themes: families differs from Redlamp's at 9fa9d9155:`, a diff with `-` Redlamp's line and `+` jaw's changed one, and `exit 1`.

Run: `scripts/sync-themes && cmp mac/build/settings/RedlampCatalog.swift mac/JawThemes/Sources/RedlampCatalog.swift && scripts/sync-themes --check`
Expected: `sync-themes: mac/JawThemes/Sources/RedlampCatalog.swift has Redlamp's catalog at 9fa9d9155`, `cmp` silent (the copy is back byte for byte as Step 3 wrote it), then the `--check` line of the first run, exit 0.

Run: `REDLAMP=/tmp/no-redlamp-here scripts/sync-themes --check; echo "exit $?"`
Expected: `sync-themes: no checkout of Redlamp at /tmp/no-redlamp-here, so nothing is checked (REDLAMP names another)` and `exit 0`.

Run: `REDLAMP="$PWD" scripts/sync-themes --check; echo "exit $?"`, this worktree standing in for a checkout without the pinned commit, which `git show` only reads.
Expected: one line, `sync-themes: /Users/pedrogomes/src/jaw-mac-settings has no packages/RedlampDesign/Sources/Themes/ThemeCatalog.swift at 9fa9d915540782ce600ee6e4854dc3f2ccd15250, so fetch 9fa9d915540782ce600ee6e4854dc3f2ccd15250 there first (fatal: …)`, and `exit 1`, with no traceback.

Then `mac/scripts/build-mac build` and `make check-parity`; each exits 0.

- [ ] **Step 8: Commit**

```bash
git add mac/JawThemes/Sources/RedlampCatalog.swift mac/JawThemes/Sources/RedlampRoles.swift scripts/sync-themes mac/Tests/JawThemesTests/RedlampTests.swift
git commit -F- --trailer "Co-authored-by: Cursor <cursoragent@cursor.com>" <<'EOF'
Redlamp's theme catalog comes into jaw, as the owner chose on 6 November, relicensed by its owner under jaw's MIT ("I own redlamp, MIT is fine"): RedlampCatalog.swift holds its 21 families of ten HSL colours in Redlamp's order, the Redlamp family's two palettes and the brand's nine colours, line for line between sync-themes markers, with ThemePalette, PrimaryRoles and its HSL parser, and RedlampRoles maps each half onto the Mac's roles by the Mac design's table, then holds it by the bands Omarchy's go through, readable over exact, so the dim text, mentions, online and secondary text are raised where a photo editor's panels had them faint, five dark halves coming through untouched, the Redlamp family keeps its red to the accent and links, the senders are eight hues 45° apart from primary's at 4.5:1 on their bubble, and Neutral is Redlamp's standard greys with the system's accent and its orange, green and red; scripts/sync-themes pins Redlamp's 9fa9d91, copies the regions from a checkout with git show and never touches it, and with --check says what differs and what Redlamp's main has changed since, Neutral's tokens included, saying so and checking nothing where there's no checkout, as in CI
EOF
```

---

### Task 4: Every theme in a catalog, and a slot for dark and one for light

**Files:**
- Create: `mac/JawThemes/Sources/Theme.swift`, `mac/JawThemes/Sources/Slots.swift`
- Modify: `mac/JawUI/Sources/Style/Preferences.swift` (`Preferences` at lines 60-136), `mac/JawUI/Sources/Settings/Panes.swift` (`AppearancePane`, Task 1's), `mac/JawUI/Sources/Settings/SettingsPane.swift` (`size`), `mac/JawApp/Sources/AppDelegate.swift` (after the observer of `AppleInterfaceThemeChangedNotification`, lines 38-42), `mac/JawApp/Sources/AppModel.swift` (`init`, line 105), `mac/Tests/JawUITests/SettingsTests.swift` (`scratchPreferences`, Task 1's)
- Test: `mac/Tests/JawThemesTests/ThemeTests.swift`, `mac/Tests/JawUITests/PreferencesTests.swift`

**Interfaces:**
- Consumes: `Omarchy.roles(_:dark:)` and `Omarchy.file(_:)` (Task 2), `RedlampCatalog.families`, `RedlampRoles.roles(_:dark:)` and `RedlampRoles.neutral(dark:accent:)` (Task 3), `Catalog.jaw`, and `omarchyFiles()` from Task 2's tests.
- Produces, in JawThemes:
  - `public enum ThemeSource: String, CaseIterable` (`.jaw`, `.omarchy`, `.redlamp`, each with `title`: "Jaw's", "Omarchy's", "Redlamp's").
  - `public enum Slot: String, CaseIterable` (`.dark`, `.light`, with `isDark` and `title`).
  - `public struct Theme: Identifiable, Hashable` (`id`, `name`, `lightName: String?`, `source`, `dark: Roles?`, `light: Roles?`, `roles(_ slot: Slot) -> Roles?`, `name(_ slot: Slot) -> String`), `init(id:name:lightName:source:dark:light:)` with `lightName` nil by default.
  - `public struct OmarchyTheme: Identifiable, Hashable` (`id`, `name`, `dark: Bool`, `colours: [String: String]`), with `init(id:name:dark:colours:)` and `init?(id:file:)`.
  - `public struct SystemAccent: Hashable` (`dark`, `light`, both `RGBA`), with `static let blue`.
  - `Omarchy.name(_ id: String) -> String`, jaw-proto's `name`.
  - `Catalog.jawTheme`, `Catalog.themes(omarchy:accent:) -> [Theme]`, `Catalog.theme(id:omarchy:accent:) -> Theme?` and `Catalog.roles(_ slot: Slot, of: Slots, omarchy:accent:) -> Roles`.
  - `public struct Slots: Hashable` (`dark`, `light`, both IDs, `subscript(_ slot: Slot) -> String`, `choosing(_ theme: Theme, for slot: Slot, only: Bool = false) -> Slots`).
- Produces, in JawUI: `Preferences.slots: Slots` in place of `theme`, kept as `themeDark` and `themeLight`; `Preferences.accent: SystemAccent`; `Preferences(defaults:accent:keeps:)` and `Preferences.keeps`, false for a snapshot, which writes nothing back; `Preferences.roles(_ slot: Slot) -> Roles`; `Preferences.themes: [Theme]`; `Preferences.systemColoursChanged()`; `SystemAccent.current()`.
- Produces, in JawThemesTests: `servedThemes() throws -> [OmarchyTheme]`, for Tasks 5 to 7.

- [ ] **Step 1: Write the failing tests**

Create `mac/Tests/JawThemesTests/ThemeTests.swift`:

```swift
import Foundation
@testable import JawThemes
import Testing

/// Every theme there is to choose, the two slots that hold them, and what each slot draws: Jaw's
/// Tokyo Night first, Omarchy's as jawd serves them, then Redlamp's, Neutral in the accent it's given;
/// a choice filling both slots, the slot of its mode, or one; and Tokyo Night drawn where a slot's
/// theme can't be had.

/// Omarchy's themes as jawd serves them: read from `protocol/themes/`, named as jaw-proto names them,
/// in the order of their IDs.
func servedThemes() throws -> [OmarchyTheme] {
    try omarchyFiles().map { OmarchyTheme(id: $0.id, name: Omarchy.name($0.id), dark: $0.dark, colours: $0.colours) }
        .sorted { $0.id < $1.id }
}

@Test func `the catalog lists Jaw's Tokyo Night, then Omarchy's as jawd serves them, then Redlamp's, and finds each by its ID`() throws {
    let served = try servedThemes()
    let themes = Catalog.themes(omarchy: served, accent: .blue)
    let sources: [ThemeSource] = [.jaw] + Array(repeating: ThemeSource.omarchy, count: 23) + Array(repeating: ThemeSource.redlamp, count: 22)
    #expect(themes.map { $0.source } == sources)
    #expect(themes[0] == Catalog.jawTheme && Catalog.jawTheme.id == "jaw" && Catalog.jawTheme.name == "Tokyo Night")
    #expect(Catalog.jawTheme.name(.dark) == "Tokyo Night" && Catalog.jawTheme.name(.light) == "Tokyo Night Day", "each half by its own name")
    #expect(themes.filter { $0.id != "jaw" }.allSatisfy { $0.name(.dark) == $0.name && $0.name(.light) == $0.name },
            "every other theme's halves share its name")
    #expect(themes[0].dark == Catalog.jaw.dark && themes[0].light == Catalog.jaw.light)
    #expect(Array(themes.map { $0.id }[1 ... 3]) == ["omarchy:catppuccin", "omarchy:catppuccin-latte", "omarchy:ethereal"])
    #expect(Array(themes.map { $0.id }[24 ... 26]) == ["redlamp:neutral", "redlamp:redlamp", "redlamp:kanagawa"])
    #expect(themes.filter { $0.dark != nil }.count == 41 && themes.filter { $0.light != nil }.count == 28,
            "the dark slot's gallery has 41 cards and the light one's 28, as the design counts them")
    #expect(themes.filter { $0.source == .omarchy }.allSatisfy { ($0.dark == nil) != ($0.light == nil) }, "one half each")
    #expect(themes.first { $0.id == "omarchy:rose-pine" }?.name == "Rosé Pine")
    #expect(Omarchy.name("last-horizon") == "Last Horizon" && Omarchy.name("retro-82") == "Retro 82")
    for theme in themes {
        #expect(Catalog.theme(id: theme.id, omarchy: served, accent: .blue) == theme, "\(theme.id)")
    }
    #expect(Catalog.theme(id: "nord", omarchy: served, accent: .blue) == nil, "an ID names its source")
    #expect(Catalog.theme(id: "omarchy:nord", omarchy: [], accent: .blue) == nil, "Omarchy's only once jawd serves them")
    let broken = OmarchyTheme(id: "broken", name: "Broken", dark: true, colours: ["accent": "#ffffff"])
    #expect(Catalog.themes(omarchy: [broken], accent: .blue).count == 23, "one whose colours don't map is left out")
}

@Test func `a slot draws its theme's half, or Jaw's Tokyo Night's where no source has the theme or it has no half of the slot's mode`() throws {
    let served = try servedThemes()
    let nord = try #require(RedlampCatalog.families.first { $0.id == "nord" })
    let both = Slots(dark: "redlamp:nord", light: "redlamp:nord")
    #expect(Catalog.roles(.dark, of: both, omarchy: served, accent: .blue) == RedlampRoles.roles(nord.dark, dark: true))
    #expect(Catalog.roles(.light, of: both, omarchy: served, accent: .blue) == RedlampRoles.roles(nord.light, dark: false))
    let gruvbox = try #require(served.first { $0.id == "gruvbox" })
    let held = Slots(dark: "omarchy:gruvbox", light: "omarchy:gruvbox")
    #expect(Catalog.roles(.dark, of: held, omarchy: served, accent: .blue) == Omarchy.roles(gruvbox.colours, dark: true))
    #expect(Catalog.roles(.light, of: held, omarchy: served, accent: .blue) == Catalog.jaw.light, "Gruvbox has no light half")
    #expect(Catalog.roles(.dark, of: held, omarchy: [], accent: .blue) == Catalog.jaw.dark, "jawd hasn't served it")
    let unknown = Slots(dark: "nord", light: "redlamp:nowhere")
    #expect(Catalog.roles(.dark, of: unknown, omarchy: served, accent: .blue) == Catalog.jaw.dark)
    #expect(Catalog.roles(.light, of: unknown, omarchy: served, accent: .blue) == Catalog.jaw.light)
}

@Test func `a theme with both halves fills both slots, one of Omarchy's the slot of its mode, and ⌥ only the slot chosen for`() throws {
    let themes = Catalog.themes(omarchy: try servedThemes(), accent: .blue)
    func theme(_ id: String) throws -> Theme {
        try #require(themes.first { $0.id == id })
    }
    let start = Slots()
    #expect(start == Slots(dark: "jaw", light: "jaw") && start[.dark] == "jaw" && start[.light] == "jaw")
    let nord = try start.choosing(theme("redlamp:nord"), for: .dark)
    #expect(nord == Slots(dark: "redlamp:nord", light: "redlamp:nord"))
    let gruvbox = try nord.choosing(theme("omarchy:gruvbox"), for: .dark)
    #expect(gruvbox == Slots(dark: "omarchy:gruvbox", light: "redlamp:nord"))
    let latte = try gruvbox.choosing(theme("omarchy:catppuccin-latte"), for: .dark)
    #expect(latte == Slots(dark: "omarchy:gruvbox", light: "omarchy:catppuccin-latte"), "a light theme fills the light slot")
    let only = try latte.choosing(theme("jaw"), for: .dark, only: true)
    #expect(only == Slots(dark: "jaw", light: "omarchy:catppuccin-latte"))
    let wrong = try only.choosing(theme("omarchy:nord"), for: .light, only: true)
    #expect(wrong == Slots(dark: "omarchy:nord", light: "omarchy:catppuccin-latte"), "a slot never holds a theme of the other mode")
}

@Test func `Neutral draws in the accent it's given, in each appearance, and Redlamp's families don't change with it`() throws {
    let purple = SystemAccent(dark: RGBA(0xBF5AF2), light: RGBA(0xAF52DE))
    let neutral = try #require(Catalog.theme(id: "redlamp:neutral", omarchy: [], accent: purple))
    #expect(neutral.dark?.accent.hex == 0xBF5AF2 && neutral.light?.accent.hex == 0xAF52DE, "the system's purple reads as it is")
    #expect(neutral.dark?.link == neutral.dark?.accent)
    #expect(neutral != Catalog.theme(id: "redlamp:neutral", omarchy: [], accent: .blue))
    #expect(Catalog.theme(id: "redlamp:nord", omarchy: [], accent: purple) == Catalog.theme(id: "redlamp:nord", omarchy: [], accent: .blue))
}
```

Create `mac/Tests/JawUITests/PreferencesTests.swift`:

```swift
import AppKit
import JawThemes
@testable import JawUI
import Testing

/// The two slots in the app's preferences: Jaw's Tokyo Night in both at first, the `theme` key read
/// into both once while they're unset and left as it is, `themeDark` and `themeLight` kept, and the
/// style drawn from the slot of the appearance, worked out again when the slots or the accent change.

@MainActor @Test func `the slots start as Jaw's Tokyo Night, take the theme key once while unset, and are kept as themeDark and themeLight`() throws {
    #expect(try scratchPreferences("slots-fresh").slots == Slots(dark: "jaw", light: "jaw"))
    let suite = "jaw.tests.slots-kept"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defaults.removePersistentDomain(forName: suite)
    defaults.set("redlamp:nord", forKey: "theme")
    let preferences = Preferences(defaults: defaults, accent: .blue)
    #expect(preferences.slots == Slots(dark: "redlamp:nord", light: "redlamp:nord"), "read into both while they're unset")
    preferences.slots[.dark] = "omarchy:gruvbox"
    #expect(defaults.string(forKey: "themeDark") == "omarchy:gruvbox" && defaults.string(forKey: "themeLight") == "redlamp:nord")
    #expect(defaults.string(forKey: "theme") == "redlamp:nord", "left as it is, so an earlier build still reads it")
    defaults.set("jaw", forKey: "theme")
    #expect(Preferences(defaults: defaults, accent: .blue).slots == Slots(dark: "omarchy:gruvbox", light: "redlamp:nord"),
            "read only while the slots are unset")
}

@MainActor @Test func `the style is the slot's theme's half for the appearance, drawn again when the slots or the accent change`() throws {
    let preferences = try scratchPreferences("slots-style")
    var told = 0
    preferences.changed.append { told += 1 }
    #expect(preferences.style(dark: true).roles == Catalog.jaw.dark && preferences.style(dark: false).roles == Catalog.jaw.light)
    let nord = try #require(RedlampCatalog.families.first { $0.id == "nord" })
    preferences.slots = Slots(dark: "redlamp:nord", light: "redlamp:neutral")
    #expect(preferences.style(dark: true).roles == RedlampRoles.roles(nord.dark, dark: true))
    #expect(preferences.style(dark: false).roles.accent.hex == 0x007AFF, "Neutral in the system's blue")
    let purple = SystemAccent(dark: RGBA(0xBF5AF2), light: RGBA(0xAF52DE))
    preferences.accent = purple
    #expect(preferences.style(dark: false).roles.accent.hex == 0xAF52DE, "and again in the accent the system has now")
    #expect(preferences.themes.first { $0.id == "redlamp:neutral" }?.light?.accent.hex == 0xAF52DE)
    preferences.accent = purple
    #expect(told == 2, "once for the slots and once for the accent, and not for an accent that didn't change")
    #expect(preferences.themes.map { $0.id } == Catalog.themes(omarchy: [], accent: purple).map { $0.id })
}

@MainActor @Test func `a snapshot's preferences read what's kept and write nothing back, telling the app all the same`() throws {
    let suite = "jaw.tests.slots-unkept"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defaults.removePersistentDomain(forName: suite)
    defaults.set("redlamp:nord", forKey: "themeDark")
    let preferences = Preferences(defaults: defaults, accent: .blue, keeps: false)
    var told = 0
    preferences.changed.append { told += 1 }
    #expect(preferences.slots.dark == "redlamp:nord", "it reads what's kept")
    preferences.slots = Slots(dark: "jaw", light: "jaw")
    preferences.appearance = .dark
    #expect(told == 2 && preferences.slots.dark == "jaw" && preferences.appearance == .dark)
    #expect(defaults.string(forKey: "themeDark") == "redlamp:nord" && defaults.string(forKey: "appearance") == nil,
            "and writes nothing back, so a run on the build Mac leaves the next one's preferences as they were")
}
```

In `mac/Tests/JawUITests/SettingsTests.swift`, `scratchPreferences` makes its preferences in macOS's blue, so no test depends on the build Mac's accent: `return Preferences(defaults: defaults)` becomes `return Preferences(defaults: defaults, accent: .blue)`.

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawThemes`
Expected: exit 1, with `error: cannot find 'OmarchyTheme' in scope` from `ThemeTests.swift`.

- [ ] **Step 3: Write the themes and the slots**

Create `mac/JawThemes/Sources/Theme.swift`:

```swift
import Foundation

/// Where a theme comes from: Jaw's own, Omarchy's as jawd serves them, or Redlamp's, copied into jaw.
public enum ThemeSource: String, CaseIterable, Sendable, Hashable {
    case jaw, omarchy, redlamp

    /// Its section in the gallery, and what follows a theme's name, as in "Gruvbox, Omarchy's".
    public var title: String {
        switch self {
        case .jaw: "Jaw's"
        case .omarchy: "Omarchy's"
        case .redlamp: "Redlamp's"
        }
    }
}

/// The two slots a theme fills: the one Jaw draws while the Mac is dark, and the one while it's light.
public enum Slot: String, CaseIterable, Sendable, Hashable {
    case dark, light

    public var isDark: Bool {
        self == .dark
    }

    public var title: String {
        isDark ? "Dark" : "Light"
    }
}

/// A theme to choose: its ID, as a slot keeps it (`jaw`, `omarchy:<id>` or `redlamp:<id>`, since
/// Omarchy's and Redlamp's share names such as `nord`), its name, its light half's where it has one of
/// its own, its source, and its halves as the roles Jaw draws in, a dark one, a light one or both.
public struct Theme: Sendable, Hashable, Identifiable {
    public var id: String
    public var name: String
    /// What its light half is called where that isn't `name`: Jaw's Tokyo Night Day.
    public var lightName: String?
    public var source: ThemeSource
    public var dark: Roles?
    public var light: Roles?

    public init(id: String, name: String, lightName: String? = nil, source: ThemeSource, dark: Roles?, light: Roles?) {
        self.id = id
        self.name = name
        self.lightName = lightName
        self.source = source
        self.dark = dark
        self.light = light
    }

    /// The half `slot` would take, where the theme has one.
    public func roles(_ slot: Slot) -> Roles? {
        slot.isDark ? dark : light
    }

    /// What the half `slot` would take is called, as the gallery's card and the slot's tile say it.
    public func name(_ slot: Slot) -> String {
        slot.isDark ? name : lightName ?? name
    }
}

/// One of Omarchy's themes as jawd serves it: its ID and its name as jawd gives them, its mode, and its
/// colours by Omarchy's keys, `#rrggbb`, as jawd sends them and the app remembers them.
public struct OmarchyTheme: Sendable, Hashable, Identifiable {
    public var id: String
    public var name: String
    public var dark: Bool
    public var colours: [String: String]

    public init(id: String, name: String, dark: Bool, colours: [String: String]) {
        self.id = id
        self.name = name
        self.dark = dark
        self.colours = colours
    }

    /// One read from its `colors.toml`, named as jawd names it; nil where jaw-proto wouldn't read it.
    public init?(id: String, file text: String) {
        guard let read = Omarchy.file(text) else { return nil }
        self.init(id: id, name: Omarchy.name(id), dark: read.dark, colours: read.colours)
    }
}

/// The system's accent as it's drawn in a dark appearance and in a light one, which Neutral takes.
public struct SystemAccent: Sendable, Hashable {
    public var dark: RGBA
    public var light: RGBA

    public init(dark: RGBA, light: RGBA) {
        self.dark = dark
        self.light = light
    }

    /// macOS's blue, the accent until another is chosen in System Settings, which the tests and the
    /// harness draw Neutral in, so their pictures don't depend on the Mac's.
    public static let blue = SystemAccent(dark: RGBA(0x0A84FF), light: RGBA(0x007AFF))
}

public extension Omarchy {
    /// What jawd shows a theme as, as jaw-proto names it: its ID's words capitalised, and Rosé Pine
    /// with its accent.
    static func name(_ id: String) -> String {
        if id == "rose-pine" { return "Rosé Pine" }
        return id.split(separator: "-", omittingEmptySubsequences: false).map { $0.prefix(1).uppercased() + $0.dropFirst() }
            .joined(separator: " ")
    }
}

public extension Catalog {
    /// Jaw's Tokyo Night as a theme to choose, with both halves, its light one Tokyo Night Day, its ID
    /// `jaw` as before.
    static let jawTheme = Theme(id: jaw.id, name: "Tokyo Night", lightName: "Tokyo Night Day", source: .jaw, dark: jaw.dark,
                                light: jaw.light)

    /// Every theme there is to choose, in the gallery's order: Jaw's Tokyo Night; Omarchy's in the
    /// order jawd serves them, but for one whose colours don't map; and Redlamp's, Neutral in `accent`,
    /// then its families in Redlamp's order.
    static func themes(omarchy: [OmarchyTheme], accent: SystemAccent) -> [Theme] {
        let served = omarchy.compactMap { theme($0) }
        return [jawTheme] + served + [neutral(accent)] + redlampFamilies
    }

    /// The theme `id` names, from the source it names, or nil where that source hasn't it.
    static func theme(id: String, omarchy: [OmarchyTheme], accent: SystemAccent) -> Theme? {
        if id == jawTheme.id { return jawTheme }
        if id == "redlamp:neutral" { return neutral(accent) }
        if id.hasPrefix("omarchy:") { return omarchy.first { "omarchy:" + $0.id == id }.flatMap { theme($0) } }
        return RedlampCatalog.families.first { "redlamp:" + $0.id == id }.map { theme($0) }
    }

    /// What `slot` draws: its theme's half for it, or Jaw's Tokyo Night's where no source has the theme
    /// or it has no half of the slot's mode, as the terminal draws a theme it doesn't know.
    static func roles(_ slot: Slot, of slots: Slots, omarchy: [OmarchyTheme], accent: SystemAccent) -> Roles {
        theme(id: slots[slot], omarchy: omarchy, accent: accent)?.roles(slot) ?? jaw.roles(dark: slot.isDark)
    }
}

extension Catalog {
    /// One of Omarchy's, `omarchy:<id>`, with the half of its mode; nil where its colours don't map,
    /// which jawd's own reading of them keeps from happening.
    static func theme(_ o: OmarchyTheme) -> Theme? {
        guard let roles = Omarchy.roles(o.colours, dark: o.dark) else { return nil }
        return Theme(id: "omarchy:" + o.id, name: o.name, source: .omarchy, dark: o.dark ? roles : nil, light: o.dark ? nil : roles)
    }

    /// One of Redlamp's families, `redlamp:<id>`, with both halves.
    static func theme(_ family: RedlampCatalog.Family) -> Theme {
        Theme(id: "redlamp:" + family.id, name: family.name, source: .redlamp,
              dark: RedlampRoles.roles(family.dark, dark: true), light: RedlampRoles.roles(family.light, dark: false))
    }

    /// Neutral, `redlamp:neutral`, in the system's accent.
    static func neutral(_ accent: SystemAccent) -> Theme {
        Theme(id: "redlamp:neutral", name: "Neutral", source: .redlamp, dark: RedlampRoles.neutral(dark: true, accent: accent.dark),
              light: RedlampRoles.neutral(dark: false, accent: accent.light))
    }

    /// Redlamp's families as themes, mapped once, as they never change while the app runs.
    static let redlampFamilies = RedlampCatalog.families.map { theme($0) }
}
```

Create `mac/JawThemes/Sources/Slots.swift`:

```swift
import Foundation

/// The theme in each slot, by ID: the one Jaw draws while the Mac is dark, and the one while it's
/// light. Both hold Jaw's Tokyo Night at first.
public struct Slots: Sendable, Hashable {
    public var dark: String
    public var light: String

    public init(dark: String = Catalog.jaw.id, light: String = Catalog.jaw.id) {
        self.dark = dark
        self.light = light
    }

    public subscript(_ slot: Slot) -> String {
        get { slot.isDark ? dark : light }
        set {
            if slot.isDark { dark = newValue } else { light = newValue }
        }
    }

    /// The slots once `theme` is chosen for `slot`: one with both halves fills both, each with its own
    /// half, unless `only`, as ⌥-click is, which fills `slot` alone; one with a single half fills the
    /// slot of its mode, so a slot never holds a theme of the other mode.
    public func choosing(_ theme: Theme, for slot: Slot, only: Bool = false) -> Slots {
        var next = self
        switch (theme.dark != nil, theme.light != nil) {
        case (true, true) where only:
            next[slot] = theme.id
        case (true, true):
            next.dark = theme.id
            next.light = theme.id
        case (true, false):
            next.dark = theme.id
        case (false, true):
            next.light = theme.id
        case (false, false):
            break
        }
        return next
    }
}
```

- [ ] **Step 4: Run JawThemes' tests**

Run: `mac/scripts/build-mac test JawThemes`
Expected: exit 0, `** TEST SUCCEEDED **`, with four more tests than Task 3 left.

- [ ] **Step 5: Keep the slots in the preferences**

In `mac/JawUI/Sources/Style/Preferences.swift`, `Preferences` keeps the slots in place of `theme`, the system's accent beside them, and what they draw until they change. Replace the line `    public var theme: String { didSet { save() } }` with:

```swift
    /// The theme in each slot, by ID, kept as `themeDark` and `themeLight`.
    public var slots: Slots {
        didSet {
            made = [:]
            save()
        }
    }

    /// The system's accent in each appearance, which Neutral draws in, read again by
    /// `systemColoursChanged()`, and not kept, as it's the system's.
    public var accent: SystemAccent {
        didSet {
            guard accent != oldValue else { return }
            made = [:]
            listed = nil
            tell()
        }
    }
```

After `@ObservationIgnored public var changed: [() -> Void] = []`, add:

```swift
    /// Each slot's roles and every theme there is, worked out once until what they rest on changes,
    /// since the app reads its style on every layout.
    @ObservationIgnored var made: [Slot: Roles] = [:]
    @ObservationIgnored var listed: [Theme]?
    /// Whether what changes is written to `defaults`: a snapshot's preferences aren't, as every
    /// checkout's snapshots on the build Mac share the app's preferences.
    @ObservationIgnored let keeps: Bool
```

`public init(defaults: UserDefaults = .standard) {` becomes:

```swift
    /// `accent` is the system's unless one's given, as the tests give macOS's blue; `keeps` is false
    /// for a snapshot, which reads what's kept and writes nothing back.
    public init(defaults: UserDefaults = .standard, accent: SystemAccent? = nil, keeps: Bool = true) {
        self.keeps = keeps
```

and in it, the line `        theme = defaults.string(forKey: "theme") ?? Catalog.jaw.id` becomes:

```swift
        // The theme key holds the one theme there was before the slots: it's read into both while
        // they're unset, and left as it is, so an earlier build still reads it.
        let theme = defaults.string(forKey: "theme") ?? Catalog.jaw.id
        slots = Slots(dark: defaults.string(forKey: "themeDark") ?? theme, light: defaults.string(forKey: "themeLight") ?? theme)
        self.accent = accent ?? .current()
```

`save()` writes only when the preferences are kept, the slots in place of `theme`, and then tells the app:

```swift
    func save() {
        if keeps {
            defaults.set(look.rawValue, forKey: "look")
            defaults.set(density.rawValue, forKey: "density")
            defaults.set(Double(textSize), forKey: "textSize")
            defaults.set(font.rawValue, forKey: "font")
            defaults.set(appearance.rawValue, forKey: "appearance")
            defaults.set(slots.dark, forKey: "themeDark")
            defaults.set(slots.light, forKey: "themeLight")
            defaults.set(paneDim, forKey: "paneDim")
            defaults.set(paneOutline, forKey: "paneOutline")
            if let c = paneOutlineColour { defaults.set(Int(c.hex), forKey: "paneOutlineColour") } else { defaults.removeObject(forKey: "paneOutlineColour") }
        }
        tell()
    }
```

with `tell()` after it:

```swift
    /// Tells the app to draw again, as something it draws from has changed.
    func tell() {
        for change in changed {
            change()
        }
    }
```

`style(dark:)` becomes the slot's theme's, with what it rests on after it:

```swift
    public func style(dark: Bool) -> Style {
        Style(roles: roles(dark ? .dark : .light), look: look, density: density, textSize: textSize, font: font)
    }

    /// What `slot` draws: its theme's half, or Jaw's Tokyo Night's where no source has the theme. The
    /// slots and the accent are read before what's made, so a view drawn from them redraws as they change.
    public func roles(_ slot: Slot) -> Roles {
        let slots = self.slots, accent = self.accent
        if let roles = made[slot] { return roles }
        let roles = Catalog.roles(slot, of: slots, omarchy: [], accent: accent)
        made[slot] = roles
        return roles
    }

    /// Every theme there is to choose, in the gallery's order.
    public var themes: [Theme] {
        let accent = self.accent
        if let listed { return listed }
        let themes = Catalog.themes(omarchy: [], accent: accent)
        listed = themes
        return themes
    }

    /// Reads the system's accent again, as the app does when AppKit says the system's colours changed.
    public func systemColoursChanged() {
        accent = .current()
    }
```

At the end of the file, after the `nsColor` extension, add:

```swift
public extension SystemAccent {
    /// The system's accent as AppKit draws it now, in a dark appearance and in a light one.
    @MainActor static func current() -> SystemAccent {
        func drawn(_ name: NSAppearance.Name) -> RGBA {
            var colour = SystemAccent.blue.dark
            NSAppearance(named: name)?.performAsCurrentDrawingAppearance { colour = RGBA(NSColor.controlAccentColor) }
            return colour
        }
        return SystemAccent(dark: drawn(.darkAqua), light: drawn(.aqua))
    }
}
```

`RGBA(_ colour: NSColor)` is Task 1's, in `Panes.swift`; it takes the colour into sRGB, which resolves the accent in the appearance being drawn.

- [ ] **Step 6: A menu for each slot until the gallery comes**

In `mac/JawUI/Sources/Settings/Panes.swift`, `AppearancePane`'s doc comment becomes `/// Settings › Appearance: dark, light or the system's, and the theme in each slot.`, and its `Section { … } footer: { … }` becomes:

```swift
            Section {
                Picker("Appearance", selection: $preferences.appearance) {
                    Text("The system's").tag(Appearance.system)
                    Text("Dark").tag(Appearance.dark)
                    Text("Light").tag(Appearance.light)
                }
                .pickerStyle(.inline)
                ForEach(Slot.allCases, id: \.self) { slot in
                    let listed = preferences.themes.filter { $0.roles(slot) != nil }
                    Picker(slot.title, selection: pick(slot)) {
                        ForEach(listed) { theme in
                            Text("\(theme.name(slot)), \(theme.source.title)").tag(theme.id)
                        }
                        if !listed.contains(where: { $0.id == preferences.slots[slot] }) {
                            Text("\(preferences.slots[slot]), drawn as \(Catalog.jawTheme.name(slot))").tag(preferences.slots[slot])
                        }
                    }
                }
            } footer: {
                Text("Jaw draws the dark theme while the Mac is dark, and the light one while it's light.")
            }
```

A slot can hold an ID the menu doesn't list: one of Omarchy's before jawd answers, with no copy, or one given as an argument. Its own item, saying it's drawn as Tokyo Night, keeps the menu's selection one of its tags, so SwiftUI doesn't warn of a selection it has no item for on every draw.

and after `body`, add:

```swift
    /// A slot's menu: choosing a theme fills the slots as a click on its card will (`Slots.choosing`).
    func pick(_ slot: Slot) -> Binding<String> {
        Binding(
            get: { preferences.slots[slot] },
            set: { id in
                if let theme = preferences.themes.first(where: { $0.id == id }) {
                    preferences.slots = preferences.slots.choosing(theme, for: slot)
                }
            },
        )
    }
```

In `mac/JawUI/Sources/Settings/SettingsPane.swift`, Appearance's size becomes `NSSize(width: 500, height: 230)`, for the second menu.

- [ ] **Step 7: The system's accent read again as it changes, and a snapshot keeping nothing**

In `mac/JawApp/Sources/AppDelegate.swift`, after the observer of `AppleInterfaceThemeChangedNotification` (lines 38-42), add:

```swift
        NotificationCenter.default.addObserver(forName: NSColor.systemColorsDidChangeNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { AppDelegate.shared?.model.preferences.systemColoursChanged() }
        }
```

`systemColoursChanged()` tells the app only when the accent has moved, and `AppModel.start()` already restyles on each telling.

In `mac/JawApp/Sources/AppModel.swift`'s `init`, line 105, `preferences = Preferences()` becomes `preferences = Preferences(keeps: launch.snapshot == nil)`: a snapshot reads the preferences kept, and those given as arguments, and writes back nothing it changes, as it already leaves the window's size and the layouts alone, so a `--keys` step that chooses a theme on the build Mac doesn't change what the next checkout's snapshots draw.

- [ ] **Step 8: Build, test and look**

Run: `mac/scripts/build-mac test JawUITests`, then `mac/scripts/build-mac build` and `mac/scripts/build-mac test JawHarnessTests`; then `make check-parity`.
Expected: each exits 0; JawUITests runs three more tests than Task 2 left.

The build Mac's own preferences for Jaw are read too, so each run gives the slots it looks at.

Run: `mac/scripts/build-mac snapshot mac/build/settings/slots.png --keys settings:appearance -themeDark redlamp:nord -themeLight jaw`
Expected: `"snapshot":1.0` among the last lines; the picture, looked at with the Read tool, shows Appearance with The system's, Dark and Light, then Dark's menu showing "Nord, Redlamp's" and Light's "Tokyo Night Day, Jaw's", the footer under them, nothing cut.

Run: `mac/scripts/build-mac snapshot mac/build/settings/nord.png -appearance dark -themeDark redlamp:nord`
Expected: the main window drawn in Redlamp's Nord: its ground the blue-grey `#2F3541`, its accent Nord's frost blue, not Tokyo Night's navy.

- [ ] **Step 9: Commit**

```bash
git add mac/JawThemes/Sources/Theme.swift mac/JawThemes/Sources/Slots.swift mac/JawUI/Sources/Style/Preferences.swift \
    mac/JawUI/Sources/Settings/Panes.swift mac/JawUI/Sources/Settings/SettingsPane.swift mac/JawApp/Sources/AppDelegate.swift \
    mac/JawApp/Sources/AppModel.swift mac/Tests/JawThemesTests/ThemeTests.swift mac/Tests/JawUITests/PreferencesTests.swift \
    mac/Tests/JawUITests/SettingsTests.swift
git commit -F- --trailer "Co-authored-by: Cursor <cursoragent@cursor.com>" <<'EOF'
Jaw draws a theme for dark and one for light, as the owner chose on 6 November: JawThemes lists every theme there is to choose, Jaw's Tokyo Night, with Tokyo Night Day its light half's own name, Omarchy's as jawd serves them and Redlamp's, Neutral in the system's accent and its families, each with the halves it has and an ID naming its source, finds what a slot draws, Tokyo Night's half where no source has its theme or it has none of the slot's mode, and fills the slots as the design has a choice fill them, a theme with both halves in both, one of Omarchy's in the slot of its mode and ⌥ only the slot chosen for; Preferences keeps the slots as themeDark and themeLight in place of theme, which it reads into both once while they're unset and leaves for an earlier build, works out what each draws once until the slots or the accent change, and reads the system's accent in both appearances again when AppKit says the system's colours changed, a snapshot's preferences writing nothing back, as every checkout's snapshots on the build Mac share them; and until the gallery comes, Settings › Appearance chooses with a menu for each slot
EOF
```

---

### Task 4b: Every theme held wherever JawUI draws it, Redlamp's red kept to its accent, and Neutral in the system's colours, its senders in colour under a grey accent

The owner's three colour choices of 6 November, 16:57: every colour drawn as text or as a mark at 3:1 wherever it's drawn, in every theme, Tokyo Night Day included, which this section reads as wherever JawUI draws it, on the tints it lays over a ground too; the Redlamp family's red kept to its accent, its light mentions in another colour and its senders' colours away from the red; and Neutral's senders still eight colours when the Mac's accent is a grey. With them come two fixes from Task 4's review: `SystemAccent.blue` becomes what AppKit draws macOS's default accent as on macOS 27.0.1, `007AFF` in both appearances (Minor 1), and Neutral's orange, green and red are read from AppKit with the accent, as the plan's Decision says they're drawn, where Task 3 wrote in the colours macOS drew before macOS 26 (its Notes). And three small marks that sat on a tint on a tint are drawn on one ground (the plan's): a link card's thumbnail placeholder on the card's own well, and a selected library row's file symbol and link icon on the selection, without their tints, as a tint on a tint on a few marks moved a whole theme's secondary text, accent drawn as text and links. Line numbers are 5e55e068's, the branch rebased onto origin/main at 9f4c298b, before this task's edits; find each place by the text quoted once anything has moved.

One hold, `Roles.held()`, comes after each theme's own mapping, which stays as Tasks 2 and 3 made it and pinned it: `Theme`'s initialiser holds its halves, so every theme the catalog makes, of every source, goes through it, and `Catalog.jaw`, which the fallbacks, the harness and about thirty tests take whole, is held where it's made. It moves each colour JawUI draws as text or as a mark toward the text with `Contrast.fit`, one ground after another, until it reads on the worst of the grounds JawUI's draw sites put it on, and keeps a colour that reads already exactly as it is, so holding held roles changes nothing:

| Colour | Where JawUI draws it | Held at 3:1 on |
| --- | --- | --- |
| `secondary` | previews, times, details, a card's words (a link's description, a contact's detail, on the card's well), a quote's text, a picture's, an album's or a link card's thumbnail placeholder symbol on the card's well; the dim text's words and marks everywhere but the window's ground and the surface: on a selected row, your bubbles and a selected message's band, and a link card's host and the library's tile placeholder on their wells | the five bases; a well on the ground, the surface, your bubbles and the band; a quote on the surface and on your bubbles |
| `tertiary`, the dim text | ticks not yet read and the pending clock on the chat list's surface and the window's ground (lines, emoji alone), pin and mute marks, placeholders, hints, the library's headings and its empty words | the ground and the surface |
| `accentText`, new: the accent drawn as text | an unread chat's time, Draft:, a mention of someone else, Show more, You in a reply (its name and bar, on the quote), the palette's tabs, marks and keys, the library's marks, a contact card's action on its well, a card's symbols and action on the accent's tint (a file's action, downloading or on its way, its progress ring among them), the library's file symbol on its tint (on the selection itself on a selected row) and its header's mark on the accent's tint, the box's chip, the status line, KeyCap | the five bases; a well on the ground, the surface, your bubbles and the band; a quote on the surface and on your bubbles; the accent's tint on the ground, the surface, your bubbles and the band; the chip on the surface |
| `link` | links, read ticks, a link card's bar on its well, the library's link icon on its well (on the selection itself on a selected row) | the five bases, and a well on the ground, the surface, your bubbles and the band |
| `mention` | a mention of you on its own tint, the "new" marker, the status line's warning, the glance's @ | the five bases, and its own tint on the ground, the surface, your bubbles and the band |
| `online` | typing in the chat list, the header and the status line | the ground and the surface |
| `selectedOnline` | typing on a selected row | a selected row, at 4.5:1 |
| `failed` | the ! on a failed message, Not sent · Retry, the box's refusal | the five bases |
| `senders` | names in a group, a quote's name and bar on the quote, the glance | the five bases, and a quote on the surface and on your bubbles |

The five bases are the grounds JawUI fills with a theme's colours (`Roles.Base`): the window's ground (a conversation in lines, the composer, the panes, the library, the glance), the surface (the chat list, the palette, the status line, the box's field, their bubbles; others' bubbles are the surface), a selected row (the chat list's, the palette's, the library's, the glance's, the contact cards'), your bubbles, and a selected message's band in lines, the selection at 55% over the window's ground, which `MessageView` draws behind a selected message, its foot, its card and its reactions (`Roles.bandTint`). The tints are the ones JawUI lays over a base, now made once, in `Roles`, and taken by every draw site, so the hold and the screen can't differ: a card's well, the text at 7% (6% in light), under a picture, an album, a link, a contact card, and the library's tiles and, on a row not selected, its link icons (`well`); a quote's well, the text at 6% (5%) (`quoteWell`); the accent's tint, 15%, behind a file's, a place's and a download's symbols, the library's file symbol on a row not selected and its header's mark (`accentTint`); the chip, the accent at 16% (12%), on the box's field (`chipTint`); and a colour's own tint, 16% of it, which a mention of you is drawn on (`mentionTint`). The palette's hint bar, the window's ground at 40% over the surface, isn't listed: it's a mix of two listed grounds, and a colour that reads on both ends of a straight line between two grounds on the same side of it reads all along it.

Three small marks are drawn on one ground where they were drawn on a tint on a tint (the plan's; Step 11): a link card's thumbnail placeholder, its link symbol, on the card's own well rather than on a well of its own on it; a selected library row's file symbol on the selection rather than on the accent's tint over it; and a selected library row's link icon on the selection rather than on a well over it. Each still reads, as the hold covers the ground it's then drawn on, and they no longer decide a whole theme's secondary text, accent drawn as text and links: holding those three pairs moved the secondary text in 41 halves rather than 27, the accent drawn as text in 36 rather than 25 and links in 24 rather than 19, and left the text only 1.24 times as strong as the secondary text in Redlamp's One Dark dark, where it's now 1.38.

Main's attachments (9f4c298b) draw on grounds and in roles the table holds already, so they change no pinned value. A file of yours on its way, in `Cards.file(_:)`, draws its action on the accent's tint and its progress ring and × in the accent, in your bubble (or on the window's ground or the band in lines), as a file downloading does: Steps 8 and 9 take them as they take the download's, the tint to `accentTint` and the ring and × to `accentText`, the track staying the accent at 25%; the ring and × are then the accent drawn as text on the accent's tint, a pair the table has. How far it's got, its own and an audio file's, is in the secondary text on your bubble. A picture of yours on its way has a disc of the window's ground at 60% over it, its ring and × in the text, a scrim; an audio file's × sits on the voice note's disc in the ground's colour, as its ▶ does, an accent fill with a mark in the ground's colour, as an unread count is. The box's strip of attached files draws each tile on the surface with a hairline border, a file's name and its × in the text, a picture's × on a scrim of the window's ground over it, and SD or HD in the secondary text and its note in failed's red, on the composer's ground, the window's. No draw site of main's is in the dim text.

- **The accent's fills keep the colour the theme wrote:** the unread badge, the tints above and the ones the selection and your bubbles are made from, a filled button, the voice note's disc, the poll's discs and bars, the rings' tracks, the focus, selection and reaction outlines, the pane outline and the composer's caret. `accent` stays exactly as the theme wrote it; the accent drawn as text is a role of its own, `accentText`, which every mapping gives the accent and the hold moves. That's the least change: twenty-eight draw sites take `accentText`, against about as many fills that would take a new fill role, and `accent` keeps its meaning, as the design and every test have it ("the accent, as Omarchy wrote it"). An unread count, drawn in the ground's colour (white in light) on the accent, reads at 3.42:1 at the least (Rosé Pine), unchanged, as do the ▶, ♪ or × on the voice note's disc, drawn in the same colour on the same fill.
- **The dim text stays the quietest, on every ground** (the plan's decision). The dim text is held at 3:1 on the window's ground and the surface; everywhere else it's drawn, it takes the secondary text's colour, held at 3:1 there, as holding the dim text there would bring it level with the secondary text, or near it, and as WhatsApp draws your bubble's grey ticks in its time's colour. On a selected row, your bubbles and a selected message's band it's drawn only as marks and a few short words (sent and delivered ticks and the pending clock, and a poll's open circles and its "voted" line, in your bubbles and on the band; pin, mute, ticks and clock on a selected chat row, the palette's chevron, a selected library row's date, the contact cards' "No number"), which take `Roles.faint(on:)`; on a well it's drawn twice, a link card's host and the library's tile placeholder, which take `roles.secondary`. Held on the ground and the surface alone, the dim text moves in 59 halves, its contrast on the ground growing by a median of ×1.12 (×1.81 at the most), and comes within 15% of the secondary text in one, Redlamp's Everforest light (`7C8A88` beside `74847C`, 3.26:1 against 3.58:1 on its ground); Tokyo Night's goes from `565F89` to `66709A`. Held on the two wells too, it came within 15% of the secondary text in 16 halves and all but met it in Everforest light (3.53:1 against 3.58:1); held on the band too, it would in 5; drawn there as it is, it would read under 3:1 on a well in 244 of the 276 pairs, the four wells a link card's host is drawn on (1.76:1 at the least, Redlamp's Catppuccin dark in your bubble), and on the band in 58 of the 69 (2.19:1, Omarchy's Last Horizon). The layout is told a message is selected (`MessageContext.selected`), in lines only: in bubbles the band is drawn behind the bubble, whose ground its words keep. White's faint ticks and pin marks, 1.67:1 on its surface, read at 3:1 there.
- **Left out:** separators, borders and rules (the "new" marker's line, a heading's rule), as authored, which carry no words; the accent's fills above, and the senders' as an avatar's fill, whose initials only gain as the senders move toward the text; the sidebar, drawn on the system's material in the system's selection; words over a picture's scrim (its pills, the band of why and what to do over a picture, an album's +N, the play and coming circles) or over the header's ground at 72% over the conversation as it scrolls under it, which no colour of the theme's alone decides; the dimming of panes not in focus, which the owner sets; the layout editor's selection, laid over its picture of the panes; the voice note's waveform and a poll's bars, the secondary text at 35 to 70%, pictures rather than words; and the text itself, the end every colour moves toward: in a mono span on its 8% tint, and at 80% in a group's preview, `Rui: `, it reads at 3.15:1 at the least (Redlamp's Everforest light on its selection).

What it moves, at Omarchy's 4c1b4cb and Redlamp's 9fa9d91, Neutral in macOS 27.0.1's colours (`SystemAccent.blue`): something in 67 of the 69 halves, Omarchy's Hackerman and Redlamp's Catppuccin dark coming through as mapped; the dim text in 59, mentions in 31, the secondary text in 27, failed in 26, the accent drawn as text in 25, links in 19, and 74 senders' colours in 21 halves; online and a selected row's online nowhere, as both mappings hold them already. What decides it: a card's well in your bubble (a link's description, a contact's detail, a placeholder) for the secondary text in 18 of its 27 halves and a selected row in 9; the surface for the dim text in 57 of 59, and the window's ground in 2; the accent's tint behind a card's symbols in your bubble or on a selected message's band for the accent drawn as text in 17 of 25, and a selected row, its words, in 7; a selected row for links in 13 of 19 and a link card's well in your bubble, its bar, in 6; the mention's own tint in your bubble or on the band in 28 of 31; a selected row for failed in 22 of 26. Every moved colour lands at 3 to 3.046:1 on the worst of its grounds. For every colour the hold moves, each of its grounds and tints lies on the far side of it from the text, so moving one way serves them all (37 senders' colours in 15 halves, online in 3, a selected row's online in 4 and one mention have a ground on the text's side of them, and read on it as they are, unmoved); and the text reads at 4.5:1 on each base, so every colour gets there. The furthest move takes a colour 56% of the way to the text, Redlamp's One Dark dark's secondary text, `6B758A` to `8E97A7`, for a card's well in your bubble, which leaves the text reading 1.38 times as strongly as it on the ground, the least of any half (1.49 with the four grounds alone); the furthest coloured one 41% of the way, Redlamp's Everforest light's mention, `C3733C` to `987151`, for its own tint in your bubble.

Tokyo Night's dark half moves its secondary text from `737AA2` to `7E87AF` (for a card's well in your bubble) and its dim text from `565F89` to `66709A` (for its surface); Tokyo Night Day's accent drawn as text and its links go from `2E7DE9` to `2D68C4` (for its selection, as with the four grounds), its red from `F52A65` to `C0316D` (for its selection), its mention from `B15C00` to `9A5814` (for its own tint on a selected message's band), its dim text from `848CB5` to `757FAC` (for its ground) and six of its senders.

The Redlamp family, which keeps the brand's red to its accent (`primaryRoles` = `.accent`): its light half's `accent` field is the safelight `E0402E` itself, a hair from the accent `D8352A`, so its mentions take the brand's filament, as its dark half's do (`FFB08A`), darkened by Redlamp's bands to read on paper (`AC7760`) and by the hold on its own tint in your bubbles (`936654`), a brown, not a red. Redlamp's light palette has no other colour for it: its paper and ink, three greys (`muted`, `border`, `neutral`), three reds (`primary`, the safelight drawn on paper; `accent`, the safelight itself; and `destructive`, which failed takes), and `success`, which online takes. Its senders' eight hues spread 37.5° apart over the 300° outside 30° either side of `primary`'s, from 48.75° past it to 48.75° short of it, rather than 45° apart from `primary`'s own: none comes within 48° of the red once held. Only the family that keeps `primary` out of its selection does this, which is Redlamp's alone; every other family's senders come through bit for bit as before. Held, its red drawn as text goes from `E0402E` to `E46353` in dark, for its selection, and from `D8352A` to `CF3429` in light, for the accent's tint behind a card's symbols in your bubble, and its links to `E46353` and `D7352A`, for the selection; its fills, the unread badges among them, keep the safelight.

Neutral takes the system's colours, `SystemAccent`: the accent chosen in System Settings, as before, and now also the system's orange, green and red for its mentions, online and failed, read from AppKit in each appearance (`NSColor.systemOrange`, `systemGreen` and `systemRed`) where and when the accent is, in `SystemAccent.current()`, again when AppKit says the system's colours changed (`NSColor.systemColorsDidChangeNotification`, which the app listens for already), where Task 3 wrote in `FF9F0A`, `30D158` and `FF453A` dark and `FF9500`, `34C759` and `FF3B30` light. The tests and the harness draw Neutral in fixed values, `SystemAccent.blue`, macOS 27.0.1's as the build Mac draws them: the accent `007AFF` in both appearances (it was `0A84FF` in dark, which AppKit no longer resolves the default accent to), orange `FF9230` dark and `FF8D28` light, green `30D158` and `34C759`, red `FF4245` and `FF383C`. The hold applies on top, as for every theme: Redlamp's bands take Neutral's light orange to `C97225` and its green to `2E9B49`, as they took the older colours, and the hold moves its secondary text, accent drawn as text and links in both halves (`1A87FC` for both in dark; `036EE4` as text and `0271EA` for links in light), its dim text in dark, and its mention and failed in light.

`SystemAccent` keeps its name, its `dark` and `light`, which are the accent, and its initialiser, which gains `orange`, `green` and `red`, each a `SystemColour` (a dark and a light `RGBA`), defaulting to macOS 27.0.1's; `RedlampRoles.neutral(dark:accent:)` takes the whole `SystemAccent` and picks the half of each it draws. That's the least change: everything that makes or passes a `SystemAccent` (`.blue`, the tests' purple, `Catalog.themes`, `theme(id:)` and `roles(_:of:)`, `Preferences.accent` and its `didSet`, which redraws when any of them changes, `systemColoursChanged()`, the tests' `Preferences(defaults:accent:)` and Tasks 5 to 7's calls) stays as it is, and the notification that reads the accent again reads the rest with it. A sibling type would add a parameter to each of those places; a new name would touch every one. Only `Catalog.neutral`, `current()`, Neutral's mapping and the tests that call it with an `RGBA` change. `current()`'s fallback, where AppKit gives no appearance of the name, takes the fixed colour of the appearance asked for, where it took the dark half for both.

Under a saturation of 0.2, as Graphite's grey is, Neutral's senders take the system's blue's hue and saturation (`SystemAccent.blue`'s half for the appearance) at the accent's lightness, so they come out as eight colours, not eight greys (Task 3's review, Minor 2: all `717171` in light). Nothing in the code knows Graphite's colours, which AppKit gives only with Graphite chosen, so the test takes the two greys Task 3's review found the senders collapse in, `98989D` in dark, macOS's system grey, and `8C8C8C` in light.

The tests rewritten here also pin where each moved colour lands, within 0.05 of its target on the ground it was moved for (Task 2's review, Minor 1; Task 3's, Minor 3): `theme.rs`'s greys and colours, the Mac's surface hold, Redlamp's bands and the hold itself. The furthest of the mappings' lands 0.034 from its target (Catppuccin Latte's links on the surface), the hold's 0.046.

A mention of you is drawn on its own tint, which moves with it, so the hold reads its grounds again after each pass and passes until one moves nothing (`Roles.lifted(_:on:at:)`). Each pass moves each channel toward the text's or leaves it, never past it, so no more than 765 passes can move a colour, three channels of 255 steps, and the loop stops at 766 whatever the grounds. In the 69 halves a mention takes three passes at the most, and any of a half's grounds' own colours, as far from reading on its own tint as a colour gets, five.

**Files:**
- Create: `mac/JawThemes/Sources/Held.swift`, `mac/Tests/JawThemesTests/HoldTests.swift`, `mac/Tests/JawUITests/HeldInkTests.swift`
- Modify: `mac/JawThemes/Sources/Roles.swift` (`accent`'s doc comment at 21-22, `tokyoNight` at 85, `tokyoNightDay` at 111, `Catalog.jaw` at 61-63), `mac/JawThemes/Sources/Omarchy.swift` (`roles(_:dark:)`, line 68), `mac/JawThemes/Sources/Theme.swift` (`Theme`'s doc comment at 30-32 and `init` at 42-49, `SystemAccent` at 84-97, `Catalog.neutral` at 152-156), `mac/JawThemes/Sources/RedlampRoles.swift` (all of it), `mac/JawUI/Sources/Style/Preferences.swift` (`accent`'s doc comment at 76-77, `systemColoursChanged()`'s at 186, `SystemAccent`'s extension at 211-221), `mac/JawUI/Sources/Settings/Panes.swift` (`RGBA.init(_:)`'s doc comment at 147), `mac/JawUI/Sources/Components/Glyphs.swift` (`Ticks.of` at 38-47, `ChatChip` at 105, 148 and 162), `mac/JawUI/Sources/Components/Tone.swift` (line 14), `mac/JawUI/Sources/ChatList/ChatListController.swift` (`ChatCell`: 381-383, 435, 442, 445, 494, 517, 531), `mac/JawUI/Sources/Layout/MessageLayout.swift` (`MessageContext`'s doc comment at 47-49 and `sending` at 62, the builder's `init` at 376-377, 494, 495, `stateMark` at 586-594, `footParts` at 600-608, `footer` at 610-629, 666, 688, 742, 750, 751, 789, 806, 830), `mac/JawUI/Sources/Layout/Cards.swift` (271-273, 465, 538, 544, 563, 566, 567, 570, 575, 576, 579, 581, 589, 590, 625, 640, 675-679, 694), `mac/JawUI/Sources/Layout/ContactCards.swift` (129), `mac/JawUI/Sources/Conversation/MessageView.swift` (104), `mac/JawUI/Sources/Conversation/ConversationController.swift` (`rebuild()` at 348-370, `select(_:)` at 771-785), `mac/JawUI/Sources/Palette/Palette.swift` (514, 517, 572, 574, 580, 593), `mac/JawUI/Sources/Library/LibraryDrawing.swift` (150-152, 164, 225-226, 242-243, 249-255, 271, 277, 280, 281, 284, 311, 314, 330), `mac/JawUI/Sources/Library/LibraryController.swift` (919, 921), `mac/JawUI/Sources/Views/Views.swift` (209), `mac/JawUI/Sources/Window/GlanceView.swift` (94), `mac/JawUI/Sources/Conversation/ContactCardsView.swift` (186), `mac/JawUI/Sources/Conversation/ContactForm.swift` (339), `mac/JawHarness/Sources/Scenes/FoundationScenes.swift` (`RolesScene`, 33, 40 and the second band's note)
- Test: `mac/Tests/JawThemesTests/OmarchyTests.swift`, `mac/Tests/JawThemesTests/RedlampTests.swift`, `mac/Tests/JawThemesTests/RolesTests.swift`, `mac/Tests/JawThemesTests/ThemeTests.swift`, `mac/Tests/JawUITests/ComponentTests.swift`, `mac/Tests/JawUITests/PreferencesTests.swift`, `mac/Tests/JawUITests/LinkPreviewTests.swift` (86-87), and the reference pictures in `mac/Tests/JawUITests/References/` that change (Step 14)

**Interfaces:**
- Consumes: `Contrast.ratio(_:_:)` and `Contrast.fit(_:on:toward:low:high:target:)`, `RGBA.opacity(_:)` and `RGBA.over(_:)`, `Omarchy.roles(_:dark:)` and `Omarchy.terminal(_:)` (Task 2); `RedlampCatalog.families`, `RedlampCatalog.Brand` (`safelight`, `filament`, `ring`, `steel`, internal), `RGBA(hue:saturation:lightness:)` (internal), `RedlampRoles.roles(_:dark:)`, `RedlampRoles.neutral(dark:accent:)`, `RedlampRoles.hsl(_:)` (Task 3); `Theme`, `Slot`, `SystemAccent` with `.blue`, `Catalog.jawTheme`, `Catalog.themes(omarchy:accent:)`, `Catalog.theme(id:omarchy:accent:)` and `Catalog.roles(_:of:omarchy:accent:)`, `Preferences.accent`, `Preferences.systemColoursChanged()` and `SystemAccent.current()` (Task 4); `Catalog.tokyoNight` and `Catalog.tokyoNightDay` (internal); `RGBA(_ colour: NSColor)` (JawUI, internal); main's `MessageContext.sending`, `Sending(step:percent:)` and `Cards`' file on its way (9f4c298b); in the tests `omarchyFiles()`, `reads(_:_:)`, `terminalMoves`, `surfaceMoves`, `redlampMoves` and `servedThemes()`; JawUITests' `spec(_:dark:)`, `laid(_:look:width:group:)`, `laid(_:_:width:)`, `sampleLink(_:)`, `shown(_:_:)`, `PaneHost`, `messageView(_:_:)`, `outgoing(_:id:status:)` (UploadTests), `libraryStyle(_:dark:density:)`, `libraryLaid(_:width:style:items:capped:)`, `rows(_:)`, `drawn(_:_:style:selected:)` and `drawingContext(_:scoped:)`; and JawUI's, internal or public, that HeldInkTests reads: `ConversationController`'s `rows`, `contexts`, `spec` and `select(_:)`, `MessageView`'s `init(frame:)`, `roles`, `laid`, `selected` and `paint(_:)`, `MessageLayout.lay(_:width:context:style:fonts:me:)`, `LayoutSpec`'s `style`, `fonts` and `me`, `Laid`'s `parts`, `pictures`, `text`, `body` and `height`, `LibraryDrawn.under`, `LibraryDrawing.tile(_:size:selected:_:)`, `Symbols.image(_:_:_:on:)`, `NSAttributedString.Key.ctColour` and `.jawBackground`, `Quote(id:from:summary:)`, `Tone.color(_:)`, `Samples` and `ChatCell`'s `show`, `line`, `ticks`, `badges` and `isSelected`.
- Produces, in JawThemes:
  - `public var accentText: RGBA` in `Roles`, after `accent`: the accent where JawUI draws it as text or as a mark; every mapping gives it the accent, and the hold moves it.
  - `public extension Roles` with `enum Base: String, CaseIterable, Sendable` (`.ground`, `.surface`, `.selection`, `.yours`, `.band`), `enum Ground: Hashable, Sendable` (`.plain(Base)`, `.well(Base)`, `.quote(Base)`, `.accentTint(Base)`, `.chip`, `.ownTint(Base)`), `enum Ink: String, CaseIterable, Sendable` (`.secondary`, `.tertiary`, `.accentText`, `.link`, `.mention`, `.online`, `.selectedOnline`, `.failed`, `.senders`), `struct Hold: Sendable, Hashable` (`ink: Ink`, `on: [Ground]`, `at: Double`; `init(_ ink: Ink, on: [Ground], at: Double = 3)`), `static let holds: [Hold]`, the tints `var well`, `quoteWell`, `accentTint`, `chipTint`, `mentionTint` and `bandTint: RGBA` and `func ownTint(of colour: RGBA) -> RGBA`, `subscript(_ base: Base) -> RGBA`, `func colour(of place: Ground, under colour: RGBA) -> RGBA`, `func colours(_ ink: Ink) -> [RGBA]`, `func faint(on base: Base) -> RGBA`, `func lifted(_ colour: RGBA, on grounds: [Ground], at: Double) -> (colour: RGBA, passes: Int)` and `func held() -> Roles`.
  - `public struct SystemColour: Sendable, Hashable` (`dark`, `light`, both `RGBA`; `init(dark:light:)`; `static let orange`, `green` and `red`, macOS 27.0.1's); `SystemAccent` gains `orange`, `green` and `red: SystemColour`, its `init(dark:light:orange:green:red:)` defaulting the three to `SystemColour`'s, and `.blue` becomes `007AFF` in both appearances.
  - `RedlampRoles.neutral(dark: Bool, accent: SystemAccent) -> Roles`, where it took an `RGBA`.
  - `Theme.init(id:name:lightName:source:dark:light:)` holds the halves it's given; `Catalog.jaw` is Tokyo Night's halves held.
  - `RedlampRoles.hues(from primary: Double, rare: Bool) -> [Double]`, `RedlampRoles.tinted(_ accent: RGBA, dark: Bool) -> RGBA`, and `rare: Bool = false` on `RedlampRoles.held(…)` and `RedlampRoles.senders(from:on:dark:rare:)`, all internal.
- Produces, in JawUI: `SystemAccent.current()` reading the accent, orange, green and red; `MessageContext.selected: Bool`, false unless the controller says so, as main's `sending` is empty; `Ticks.of(_ status: MessageStatus?, _ roles: Roles, on base: Roles.Base = .surface) -> (Int, RGBA)?`; `ChatCell.place: Roles.Base`; `MessageLayout.Builder.cardGround: Roles.Base`, and the builder's `stateMark(on:)`, `footParts(_:at:on:)` and `footer(time:on:)`; `LibraryDrawing.file`, `link` and `twoLines` gain `selected: Bool`, and a selected row's file symbol and link icon are drawn without their tints; a link card's thumbnail placeholder is drawn on the card's own well; a link card's host and a library tile's placeholder, the dim text's two draws on a well, take the secondary text's colour; and the draw sites above in `accentText`, `faint(on:)` and the tints of `Roles`.
- Produces, in JawThemesTests: `mappedHalves() throws -> [(name: String, roles: Roles)]` and `readsWhereDrawn(_ r: Roles, _ name: String)`, which RedlampTests' Graphite test takes too; in JawUITests: `runColours(_ line: CTLine) -> [CGColor]` and `tickColour(_ l: Laid) -> CGColor?`.

- [ ] **Step 1: Write JawThemes' failing tests**

Create `mac/Tests/JawThemesTests/HoldTests.swift`:

```swift
import Foundation
@testable import JawThemes
import Testing

/// The hold every theme's roles go through after their own mapping (Owner, 6 Nov): each colour JawUI
/// draws as text or as a mark at 3:1 on every ground its draw sites put it on, the tints it lays over
/// a ground among them, a selected row's online at 4.5:1 on its selection, and the accent's fills, the
/// grounds, the text and the borders as the theme wrote them; what it moves in each half and where
/// each moved colour lands, as a port of it found on 6 November; a mention of you on its own tint, in
/// passes that end; and every theme the catalog lists, held.

/// Every half the catalog makes, as its own mapping makes it, before the hold, by the name the lists
/// below give it: Jaw's Tokyo Night, Omarchy's by ID, Neutral in macOS 27.0.1's colours, and Redlamp's
/// families.
func mappedHalves() throws -> [(name: String, roles: Roles)] {
    var out: [(name: String, roles: Roles)] = [("jaw dark", Catalog.tokyoNight), ("jaw light", Catalog.tokyoNightDay)]
    for theme in try omarchyFiles() {
        out.append(("omarchy:\(theme.id)", try #require(Omarchy.roles(theme.colours, dark: theme.dark))))
    }
    out.append(("redlamp:neutral dark", RedlampRoles.neutral(dark: true, accent: .blue)))
    out.append(("redlamp:neutral light", RedlampRoles.neutral(dark: false, accent: .blue)))
    for family in RedlampCatalog.families {
        out.append(("redlamp:\(family.id) dark", RedlampRoles.roles(family.dark, dark: true)))
        out.append(("redlamp:\(family.id) light", RedlampRoles.roles(family.light, dark: false)))
    }
    return out
}

/// Whether held roles read where JawUI draws them: each colour in `Roles.holds` at its contrast on
/// every one of its grounds, a colour's own tint made from it, as `Contrast` measures it on the
/// colours the screen is given.
func readsWhereDrawn(_ r: Roles, _ name: String) {
    for hold in Roles.holds {
        for colour in r.colours(hold.ink) {
            for place in hold.on {
                let ratio = Contrast.ratio(colour.hex, r.colour(of: place, under: colour).hex)
                #expect(ratio >= hold.at, "\(name): \(hold.ink) on \(place) at \(ratio)")
            }
        }
    }
}

@Test func `the hold is JawUI's draw sites: each colour drawn as text or as a mark, on each ground and tint it's drawn on`() {
    let bases = Roles.Base.allCases.map(Roles.Ground.plain)
    let wells: [Roles.Ground] = [.well(.ground), .well(.surface), .well(.yours), .well(.band)]
    let quotes: [Roles.Ground] = [.quote(.surface), .quote(.yours)]
    let tints: [Roles.Ground] = [.accentTint(.ground), .accentTint(.surface), .accentTint(.yours), .accentTint(.band)]
    #expect(Roles.Base.allCases == [.ground, .surface, .selection, .yours, .band])
    #expect(Roles.holds == [
        Roles.Hold(.secondary, on: bases + wells + quotes),
        Roles.Hold(.tertiary, on: [.plain(.ground), .plain(.surface)]),
        Roles.Hold(.accentText, on: bases + wells + quotes + tints + [.chip]),
        Roles.Hold(.link, on: bases + wells),
        Roles.Hold(.mention, on: bases + [.ownTint(.ground), .ownTint(.surface), .ownTint(.yours), .ownTint(.band)]),
        Roles.Hold(.online, on: [.plain(.ground), .plain(.surface)]),
        Roles.Hold(.selectedOnline, on: [.plain(.selection)], at: 4.5),
        Roles.Hold(.failed, on: bases),
        Roles.Hold(.senders, on: bases + quotes),
    ])
    #expect(Roles.holds.map(\.ink) == Roles.Ink.allCases, "every colour held, once")
    let r = Catalog.jaw.dark
    #expect([r[.ground], r[.surface], r[.selection], r[.yours]] == [r.ground, r.surface, r.selection, r.yours])
    #expect(r.bandTint == r.selection.opacity(0.55) && r[.band] == r.bandTint.over(r.ground), "a selected message's band in lines")
    #expect(r.well == r.text.opacity(0.07) && r.quoteWell == r.text.opacity(0.06) && r.accentTint == r.accent.opacity(0.15)
        && r.chipTint == r.accent.opacity(0.16) && r.mentionTint == r.mention.opacity(0.16), "the tints as JawUI lays them, in dark")
    let l = Catalog.jaw.light
    #expect(l.well == l.text.opacity(0.06) && l.quoteWell == l.text.opacity(0.05) && l.chipTint == l.accent.opacity(0.12), "and in light")
    let grey = RGBA(0x808080)
    #expect(r.colour(of: .plain(.band), under: grey) == r[.band] && r.colour(of: .well(.yours), under: grey) == r.well.over(r.yours))
    #expect(r.colour(of: .quote(.yours), under: grey) == r.quoteWell.over(r.yours))
    #expect(r.colour(of: .accentTint(.yours), under: grey) == r.accentTint.over(r.yours))
    #expect(r.colour(of: .chip, under: grey) == r.chipTint.over(r.surface), "the chip, on the box's field, the surface")
    #expect(r.colour(of: .ownTint(.band), under: grey) == grey.opacity(0.16).over(r[.band]), "a colour's own tint, made from it")
    #expect(r.colours(.senders) == r.senders && r.colours(.accentText) == [r.accentText])
    #expect(r.faint(on: .ground) == r.tertiary && r.faint(on: .surface) == r.tertiary)
    #expect(r.faint(on: .selection) == r.secondary && r.faint(on: .yours) == r.secondary && r.faint(on: .band) == r.secondary,
            "the dim text's words and marks on a selected row, your bubbles and a selected message's band take the secondary text's colour, which is held there")
}

/// What the hold moves, colour by colour, in each half as its mapping leaves it, as a port of it found
/// on 6 November at Omarchy's 4c1b4cb and Redlamp's 9fa9d91, Neutral in macOS 27.0.1's colours. Online
/// and a selected row's online read wherever they're drawn already, so they move nowhere.
let heldMoves: [Roles.Ink: [String]] = [
    .secondary: [
        "jaw dark", "omarchy:last-horizon", "redlamp:neutral dark", "redlamp:neutral light", "redlamp:redlamp light",
        "redlamp:kanagawa light", "redlamp:tokyo-night dark", "redlamp:tokyo-night light", "redlamp:moonlight dark",
        "redlamp:rose-pine dark", "redlamp:rose-pine light", "redlamp:nord dark", "redlamp:everforest dark",
        "redlamp:everforest light", "redlamp:dracula dark", "redlamp:solarized dark", "redlamp:one-dark dark",
        "redlamp:one-dark light", "redlamp:monokai dark", "redlamp:sakura dark", "redlamp:copper dark",
        "redlamp:mc-vinyl-noir dark", "redlamp:mc-neon-dusk dark", "redlamp:mc-burnt-tape dark", "redlamp:mc-burnt-tape light",
        "redlamp:mc-phantom-frequency dark", "redlamp:mc-velvet-static dark",
    ],
    .tertiary: [
        "jaw dark", "jaw light", "omarchy:catppuccin-latte", "omarchy:catppuccin", "omarchy:ethereal", "omarchy:everforest",
        "omarchy:flexoki-light", "omarchy:gruvbox", "omarchy:kanagawa", "omarchy:last-horizon", "omarchy:matte-black",
        "omarchy:miasma", "omarchy:nord", "omarchy:osaka-jade", "omarchy:retro-82", "omarchy:ristretto", "omarchy:rose-pine",
        "omarchy:solitude", "omarchy:starship", "omarchy:tokyo-night", "omarchy:vantablack", "omarchy:white",
        "redlamp:neutral dark", "redlamp:redlamp dark", "redlamp:redlamp light", "redlamp:kanagawa dark", "redlamp:kanagawa light",
        "redlamp:tokyo-night light", "redlamp:moonlight light", "redlamp:catppuccin light", "redlamp:rose-pine dark",
        "redlamp:nord dark", "redlamp:nord light", "redlamp:everforest dark", "redlamp:everforest light", "redlamp:gruvbox dark",
        "redlamp:gruvbox light", "redlamp:dracula dark", "redlamp:dracula light", "redlamp:solarized dark",
        "redlamp:solarized light", "redlamp:one-dark light", "redlamp:monokai light", "redlamp:ocean dark", "redlamp:ocean light",
        "redlamp:sakura dark", "redlamp:sakura light", "redlamp:copper dark", "redlamp:copper light", "redlamp:mc-vinyl-noir dark",
        "redlamp:mc-vinyl-noir light", "redlamp:mc-neon-dusk dark", "redlamp:mc-neon-dusk light", "redlamp:mc-burnt-tape dark",
        "redlamp:mc-burnt-tape light", "redlamp:mc-phantom-frequency dark", "redlamp:mc-phantom-frequency light",
        "redlamp:mc-velvet-static dark", "redlamp:mc-velvet-static light",
    ],
    .accentText: [
        "jaw light", "omarchy:last-horizon", "omarchy:miasma", "omarchy:nord", "omarchy:osaka-jade", "omarchy:rose-pine",
        "omarchy:solitude", "omarchy:white", "redlamp:neutral dark", "redlamp:neutral light", "redlamp:redlamp dark",
        "redlamp:redlamp light", "redlamp:tokyo-night light", "redlamp:rose-pine light", "redlamp:nord dark",
        "redlamp:nord light", "redlamp:everforest light", "redlamp:gruvbox light", "redlamp:solarized dark",
        "redlamp:solarized light", "redlamp:monokai dark", "redlamp:monokai light", "redlamp:sakura light",
        "redlamp:copper light", "redlamp:mc-vinyl-noir light",
    ],
    .link: [
        "jaw light", "omarchy:catppuccin-latte", "omarchy:flexoki-light", "omarchy:kanagawa", "omarchy:rose-pine",
        "omarchy:solitude", "redlamp:neutral dark", "redlamp:neutral light", "redlamp:redlamp dark", "redlamp:redlamp light",
        "redlamp:tokyo-night light", "redlamp:nord dark", "redlamp:nord light", "redlamp:everforest light",
        "redlamp:gruvbox light", "redlamp:solarized dark", "redlamp:solarized light", "redlamp:monokai dark",
        "redlamp:copper light",
    ],
    .mention: [
        "jaw light", "omarchy:catppuccin-latte", "omarchy:flexoki-light", "omarchy:kanagawa", "omarchy:last-horizon",
        "omarchy:matte-black", "omarchy:miasma", "omarchy:osaka-jade", "omarchy:rose-pine", "redlamp:neutral light",
        "redlamp:redlamp light", "redlamp:kanagawa light", "redlamp:tokyo-night light", "redlamp:moonlight light",
        "redlamp:catppuccin light", "redlamp:rose-pine light", "redlamp:nord light", "redlamp:everforest light",
        "redlamp:gruvbox light", "redlamp:dracula light", "redlamp:solarized light", "redlamp:one-dark dark",
        "redlamp:monokai light", "redlamp:ocean light", "redlamp:sakura light", "redlamp:copper light",
        "redlamp:mc-vinyl-noir light", "redlamp:mc-neon-dusk light", "redlamp:mc-burnt-tape light",
        "redlamp:mc-phantom-frequency light", "redlamp:mc-velvet-static light",
    ],
    .failed: [
        "jaw light", "omarchy:flexoki-light", "omarchy:gruvbox", "omarchy:kanagawa", "omarchy:last-horizon", "omarchy:lumon",
        "omarchy:miasma", "omarchy:nord", "omarchy:retro-82", "omarchy:solitude", "redlamp:neutral light", "redlamp:redlamp dark",
        "redlamp:tokyo-night light", "redlamp:moonlight light", "redlamp:nord dark", "redlamp:nord light",
        "redlamp:everforest dark", "redlamp:everforest light", "redlamp:gruvbox dark", "redlamp:dracula dark",
        "redlamp:dracula light", "redlamp:solarized dark", "redlamp:one-dark light", "redlamp:ocean light", "redlamp:copper dark",
        "redlamp:mc-burnt-tape dark",
    ],
]

/// How many of each half's eight senders the hold moves.
let heldSenders: [String: Int] = [
    "jaw light": 6, "omarchy:catppuccin-latte": 6, "omarchy:everforest": 1, "omarchy:flexoki-light": 7, "omarchy:gruvbox": 1,
    "omarchy:kanagawa": 5, "omarchy:last-horizon": 4, "omarchy:lumon": 2, "omarchy:lupine": 1, "omarchy:matte-black": 2,
    "omarchy:miasma": 5, "omarchy:nord": 3, "omarchy:osaka-jade": 4, "omarchy:retro-82": 4, "omarchy:rose-pine": 6,
    "omarchy:solitude": 4, "redlamp:redlamp dark": 3, "redlamp:tokyo-night dark": 1, "redlamp:one-dark dark": 2,
    "redlamp:monokai dark": 3, "redlamp:ocean dark": 4,
]

@Test func `every half reads where JawUI draws it once held, the hold moving what a port of it found, each colour to within 0.05 of its floor`() throws {
    var moved: [Roles.Ink: [String]] = [:]
    var senders: [String: Int] = [:]
    for (name, mapped) in try mappedHalves() {
        let held = mapped.held()
        readsWhereDrawn(held, name)
        #expect(held.held() == held, "\(name): roles that are held come through the hold as they are")
        #expect([held.ground, held.surface, held.selection, held.yours, held.text, held.separator, held.accent]
            == [mapped.ground, mapped.surface, mapped.selection, mapped.yours, mapped.text, mapped.separator, mapped.accent],
            "\(name): the grounds, the text, the borders and the accent's fills as the theme wrote them")
        for hold in Roles.holds {
            let (was, now) = (mapped.colours(hold.ink), held.colours(hold.ink))
            for (before, after) in zip(was, now) where after != before {
                let worst = hold.on.map { Contrast.ratio(after.hex, held.colour(of: $0, under: after).hex) }.min() ?? 0
                #expect(worst <= hold.at + 0.05, "\(name): \(hold.ink) lands at \(worst) on the worst of its grounds")
            }
            if hold.ink == .senders {
                let n = zip(was, now).count { $0 != $1 }
                if n > 0 { senders[name] = n }
            } else if was != now {
                moved[hold.ink, default: []].append(name)
            }
        }
    }
    for ink in Roles.Ink.allCases where ink != .senders {
        #expect(moved[ink, default: []].sorted() == heldMoves[ink, default: []].sorted(), "\(ink)")
    }
    #expect(senders == heldSenders)
}

@Test func `a mention of you is held on its own tint, which moves with it, in passes that always end`() throws {
    let grounds = try #require(Roles.holds.first { $0.ink == .mention }).on
    var most = 0
    for (name, mapped) in try mappedHalves() {
        let (colour, passes) = mapped.lifted(mapped.mention, on: grounds, at: 3)
        #expect(colour == mapped.held().mention, "\(name)")
        most = max(most, passes)
    }
    #expect(most == 3, "three passes at the most in any half, as a port of the hold found")
    let r = Catalog.jaw.dark
    let (far, passes) = r.lifted(r.surface, on: [.ownTint(.surface)], at: 3)
    #expect(far.hex == 0x767F9D && passes == 3, "the surface's own colour on its own tint, as far from reading as a colour gets, in three")
    #expect(Contrast.ratio(far.hex, r.colour(of: .ownTint(.surface), under: far).hex) >= 3)
    let none = r.lifted(r.mention, on: [], at: 3)
    #expect(none.colour == r.mention && none.passes == 0, "with no grounds, nothing to move")
}

@Test func `every theme the catalog lists comes held, Jaw's Tokyo Night too, and an unread count reads on the accent's badge`() throws {
    let themes = Catalog.themes(omarchy: try servedThemes(), accent: .blue)
    let drawn = themes.flatMap { theme in Slot.allCases.compactMap { theme.roles($0) } }
    let held = try mappedHalves().map { $0.roles.held() }
    #expect(drawn.count == 69 && held.count == 69 && Set(drawn) == Set(held), "the catalog's halves are the mappings' held")
    #expect(Catalog.jaw.dark == Catalog.tokyoNight.held() && Catalog.jaw.light == Catalog.tokyoNightDay.held(),
            "and so is the family the slots fall back on and the harness and the pictures draw in")
    let counts = drawn.map { r in Contrast.ratio((r.isDark ? r.ground : RGBA(0xFFFFFF)).hex, r.accent.hex) }
    #expect(counts.allSatisfy { $0 >= 3 } && abs((counts.min() ?? 0) - 3.4245) < 0.0001,
            "the count on its badge, in the accent as written, at 3.42:1 at the least, in Rosé Pine")
}
```

In `mac/Tests/JawThemesTests/OmarchyTests.swift`, the two tests of what moves also check where it lands, as `theme.rs`'s own test does (Task 2's review, Minor 1). `` `each theme moves the greys theme.rs moves, and no others` `` (lines 73-88) becomes:

```swift
@Test func `each theme moves the greys theme.rs moves, and no others, each grey and colour it moves landing within 0.05 of its target`() throws {
    for theme in try omarchyFiles() {
        let c = theme.colours.compactMapValues(Omarchy.hex)
        let t = try #require(Omarchy.terminal(c))
        let greys: [(String, UInt32, UInt32, UInt32)] = [
            ("surface", c["lighter_background"]!, t.surface, t.bg), ("selection", c["selection"]!, t.selection, t.surface),
            ("dim text", c["dark_foreground"]!, t.comment, t.bg), ("borders", c["muted"]!, t.gutter, t.bg),
        ]
        var moved: [String] = []
        for (what, was, now, ground) in greys where now != was {
            moved.append("\(what) \(Contrast.ratio(now, ground) > Contrast.ratio(was, ground) ? "raised" : "lowered")")
        }
        let listed = terminalMoves.filter { $0.themes.contains(theme.id) }.flatMap { $0.moved }
        #expect(moved.sorted() == listed.sorted(), "\(theme.id)")
        let picked = Contrast.fit(c["selection"]!, on: t.bg, toward: t.fg, low: 1.15, target: 1.25)
        var lands: [(String, UInt32, UInt32, UInt32, Double)] = [
            ("surface", c["lighter_background"]!, t.surface, t.bg, 1.1), ("dim text", c["dark_foreground"]!, t.comment, t.bg, 3),
            ("borders", c["muted"]!, t.gutter, t.bg, 1.8), ("selection", c["selection"]!, picked, t.bg, 1.25),
            ("selection on the surface", picked, t.selection, t.surface, 1.15), ("link", c["cyan"]!, t.link, t.bg, 3),
            ("mention", c["orange"]!, t.mention, t.bg, 3), ("online", c["green"]!, t.online, t.bg, 3),
            ("notice", c["yellow"]!, t.notice, t.bg, 3),
        ]
        for (was, now) in zip(["red", "yellow", "blue", "magenta", "cyan", "green", "orange", "bright_cyan"].map { c[$0]! }, t.senders) {
            lands.append(("a sender", was, now, t.bg, 3))
        }
        for (what, was, now, ground, target) in lands where now != was {
            #expect(abs(Contrast.ratio(now, ground) - target) <= 0.05, "\(theme.id): \(what) lands at \(Contrast.ratio(now, ground))")
        }
    }
}
```

and `` `the Mac holds the terminal's colours at 3:1 on its surface too, which moves nine themes' and no others'` `` (lines 104-121) becomes:

```swift
@Test func `the Mac holds the terminal's colours at 3:1 on its surface too, which moves nine themes' and no others', each to within 0.05 of 3:1 there`() throws {
    for theme in try omarchyFiles() {
        let c = theme.colours.compactMapValues(Omarchy.hex)
        let t = try #require(Omarchy.terminal(c))
        let r = try #require(Omarchy.roles(theme.colours, dark: theme.dark))
        let red = Contrast.fit(c["red"]!, on: t.bg, toward: t.fg, low: 3, target: 3)
        var moved: [String] = []
        for (what, was, now) in [("link", t.link, r.link), ("mention", t.mention, r.mention), ("online", t.online, r.online),
                                 ("failed", red, r.failed)] where now.hex != was
        {
            moved.append(what)
            #expect(abs(Contrast.ratio(now.hex, t.surface) - 3) <= 0.05, "\(theme.id): \(what) lands at \(Contrast.ratio(now.hex, t.surface))")
        }
        let senders = zip(t.senders, r.senders).count { $0.0 != $0.1.hex }
        if senders > 0 { moved.append("senders \(senders)") }
        for (was, now) in zip(t.senders, r.senders) where now.hex != was {
            #expect(abs(Contrast.ratio(now.hex, t.surface) - 3) <= 0.05, "\(theme.id): a sender lands at \(Contrast.ratio(now.hex, t.surface))")
        }
        #expect(moved.sorted() == (surfaceMoves[theme.id] ?? []).sorted(), "\(theme.id)")
        #expect(r.accent.hex == t.accent && r.ground.hex == t.bg && r.tertiary.hex == t.comment, "\(theme.id): the rest as the terminal has it")
        #expect(r.accentText == r.accent, "\(theme.id): the accent drawn as text, as written until the hold")
    }
}
```

In `mac/Tests/JawThemesTests/RedlampTests.swift`:

- In `redlampMoves`, the `"mention"` list begins with the Redlamp family's light half, whose mention is now the filament held by the bands: `"mention": [` is followed by `"redlamp light", "kanagawa light", "tokyo-night light", …`, the rest as it is.
- `` `Redlamp's halves map by the Mac design's table, held by the bands, which leave five dark halves as they are` `` (lines 54-83) becomes:

```swift
@Test func `Redlamp's halves map by the Mac design's table, held by the bands, which leave five dark halves as they are and land what they move within 0.05 of its target`() {
    var moved: [String: [String]] = [:]
    var untouched: [String] = []
    for family in RedlampCatalog.families {
        for (half, p, dark) in [("dark", family.dark, true), ("light", family.light, false)] {
            let r = RedlampRoles.roles(p, dark: dark)
            let tint = p.primaryRoles.contains(.selection) ? p.primary : p.neutral
            let accent = p.primaryRoles.contains(.accent) ? p.primary : p.neutral
            let mention = p.accent == RedlampCatalog.Brand.safelight ? RedlampCatalog.Brand.filament : p.accent
            let table: [(String, RGBA, RGBA)] = [
                ("dim text", p.muted.opacity(0.28 / 0.45).over(p.background), r.tertiary), ("mention", mention, r.mention),
                ("online", p.success, r.online), ("secondary", p.muted, r.secondary), ("accent", accent, r.accent),
                ("failed", p.destructive, r.failed), ("selection", tint.opacity(dark ? 0.2 : 0.16).over(p.card), r.selection),
                ("yours", tint.opacity(dark ? 0.18 : 0.12).over(p.background), r.yours),
            ]
            let name = "\(family.id) \(half)"
            let differ = table.filter { $0.1.hex != $0.2.hex }.map { $0.0 }
            for role in differ {
                moved[role, default: []].append(name)
            }
            if differ.isEmpty { untouched.append(name) }
            let (g, s, t) = (r.ground.hex, r.surface.hex, r.text.hex)
            for (role, was, now) in table where now.hex != was.hex {
                let target = role == "selection" || role == "yours" ? 4.5 : 3
                let reached = switch role {
                case "dim text": Contrast.ratio(now.hex, g)
                case "selection", "yours": Contrast.ratio(t, now.hex)
                default: min(Contrast.ratio(now.hex, g), Contrast.ratio(now.hex, s))
                }
                #expect(abs(reached - target) <= 0.05, "\(name): \(role) lands at \(reached), its target \(target)")
            }
            #expect(r.ground.hex == p.background.hex && r.surface.hex == p.card.hex && r.text.hex == p.foreground.hex, "\(name)")
            #expect(r.link == r.accent && r.accentText == r.accent && r.separator.hex == p.border.hex,
                    "\(name): links and the accent drawn as text as the accent, borders as authored")
        }
    }
    #expect(untouched == ["redlamp dark", "kanagawa dark", "catppuccin dark", "gruvbox dark", "ocean dark"])
    for (role, halves) in redlampMoves {
        #expect(moved[role, default: []].sorted() == halves.sorted(), "\(role)")
    }
    #expect(Set(moved.keys) == Set(redlampMoves.keys))
}
```

- `` `the Redlamp family keeps its red to the accent and links, its selection and your bubbles from its grey` `` (lines 85-91) becomes, for both halves (Task 3's review, Minor 1):

```swift
@Test func `the Redlamp family keeps its red to the accent and links in both halves, its mentions the filament and its senders' hues away from the red`() {
    let redlamp = RedlampCatalog.families[0]
    let dark = RedlampRoles.roles(redlamp.dark, dark: true)
    #expect(dark.accent.hex == 0xE0402E && dark.link.hex == 0xE0402E, "the safelight")
    #expect(dark.selection.hex == RedlampCatalog.Brand.ring.opacity(0.2).over(redlamp.dark.card).hex, "ring, not red")
    #expect(dark.yours.hex == RedlampCatalog.Brand.ring.opacity(0.18).over(redlamp.dark.background).hex)
    #expect(dark.mention.hex == 0xFFB08A, "the filament, as it is")
    let light = RedlampRoles.roles(redlamp.light, dark: false)
    #expect(light.accent.hex == 0xD8352A && light.link.hex == 0xD8352A, "the safelight drawn on paper")
    #expect(light.selection.hex == RedlampCatalog.Brand.steel.opacity(0.16).over(redlamp.light.card).hex, "steel, not red")
    #expect(light.yours.hex == RedlampCatalog.Brand.steel.opacity(0.12).over(redlamp.light.background).hex)
    #expect(redlamp.light.accent == RedlampCatalog.Brand.safelight && light.mention.hex == 0xAC7760,
            "its accent field is the safelight, so its mentions take the filament, darkened to read on paper, not a red beside the unread counts")
    let (heldDark, heldLight) = (dark.held(), light.held())
    #expect(heldDark.accentText.hex == 0xE46353 && heldDark.link.hex == 0xE46353 && heldDark.accent == dark.accent,
            "held where they're drawn: the red as text and links a step lighter, to read on the dark selection, its fills the safelight")
    #expect(heldLight.accentText.hex == 0xCF3429 && heldLight.link.hex == 0xD7352A && heldLight.mention.hex == 0x936654,
            "and darker in light: as text for the accent's tint behind a card's symbols in your bubbles, links for the selection, the mention for its own tint in your bubbles")
    #expect(dark.senders.map(\.hex) == [0xE0D12E, 0x80E02E, 0x2EE04B, 0x2EE0BA, 0x2E96E0, 0x7C77EB, 0xB75AE7, 0xE340B4])
    #expect(light.senders.map(\.hex) == [0x736715, 0x447315, 0x167722, 0x15735B, 0x1C6C9C, 0x2A2AD8, 0x972AD8, 0xB82292])
    for (half, r, primary) in [("dark", heldDark, redlamp.dark.primary), ("light", heldLight, redlamp.light.primary)] {
        let red = RedlampRoles.hsl(primary).hue
        for (i, sender) in r.senders.enumerated() {
            let apart = abs(RedlampRoles.hsl(sender).hue - red)
            #expect(min(apart, 360 - apart) >= 30, "\(half): sender \(i) is \(min(apart, 360 - apart))° from the safelight's hue")
        }
    }
}
```

- In `` `every half of Redlamp's reads, its senders at 4.5:1 on their bubble, and Neutral's in the system's blue` ``, the loop over Neutral (lines 102-107) becomes:

```swift
    for dark in [true, false] {
        let r = RedlampRoles.neutral(dark: dark, accent: .blue)
        reads(r, "neutral \(dark ? "dark" : "light")")
        #expect(r.ground.hex == (dark ? 0x1D1D1D : 0xECECEC), "Redlamp's panel grey")
        #expect(r.accent.hex == 0x007AFF, "the system's accent, which reads as it is")
        #expect([r.mention.hex, r.online.hex, r.failed.hex] == (dark ? [0xFF9230, 0x30D158, 0xFF4245] : [0xC97225, 0x2E9B49, 0xFF383C]),
                "the system's orange, green and red as macOS 27.0.1 draws them, light's orange and green darkened to read on its ground")
    }
    let other = SystemAccent(dark: RGBA(0x0A84FF), light: RGBA(0x007AFF), red: SystemColour(dark: RGBA(0xFF6961), light: RGBA(0xD70015)))
    #expect(RedlampRoles.neutral(dark: true, accent: other).failed.hex == 0xFF6961
        && RedlampRoles.neutral(dark: false, accent: other).failed.hex == 0xD70015, "Neutral's failed is the system's red, whatever AppKit gives")
```

- At the end of the file, add:

```swift
@Test func `a half's senders are eight hues 45° apart from primary's, or, where it keeps primary rare, 37.5° apart away from it`() {
    #expect(RedlampRoles.hues(from: 10, rare: false) == [10, 55, 100, 145, 190, 235, 280, 325])
    #expect(RedlampRoles.hues(from: 10, rare: true) == [58.75, 96.25, 133.75, 171.25, 208.75, 246.25, 283.75, 321.25],
            "48.75° past it to 48.75° short of it, none within 30° of it")
    #expect(RedlampCatalog.families.filter { !$0.dark.primaryRoles.contains(.selection) }.map(\.id) == ["redlamp"],
            "only the Redlamp family keeps its primary rare, so every other family's senders are as they were")
}

/// Graphite's accent is a grey, which only AppKit gives with Graphite chosen; these are the two Task 3's
/// review found Neutral's senders all but one colour in: macOS's system grey in dark, and a grey with no
/// hue at all in light.
@Test func `with a grey accent, as Graphite's is, Neutral's senders take the system's blue's hue and come out as eight colours`() {
    let graphite = SystemAccent(dark: RGBA(0x98989D), light: RGBA(0x8C8C8C))
    for dark in [true, false] {
        let name = "Neutral in graphite, \(dark ? "dark" : "light")"
        let grey = dark ? graphite.dark : graphite.light
        let r = RedlampRoles.neutral(dark: dark, accent: graphite)
        reads(r, name)
        readsWhereDrawn(r.held(), name)
        let blue = RedlampRoles.hsl(dark ? SystemAccent.blue.dark : SystemAccent.blue.light)
        let tinted = RGBA(hue: blue.hue, saturation: blue.saturation, lightness: RedlampRoles.hsl(grey).lightness)
        #expect(r.senders == RedlampRoles.senders(from: tinted, on: r.surface.hex, dark: dark), "\(name): the blue's hue at the grey's lightness")
        #expect(Set(r.held().senders.map(\.hex)).count == 8, "\(name): eight colours, not eight greys")
    }
    #expect(RedlampRoles.neutral(dark: true, accent: graphite).senders.map(\.hex)
        == [0x3696FF, 0x9D78FF, 0xFF36FB, 0xFF4D76, 0xFF9F36, 0xC8FF36, 0x36FF3A, 0x36FFD1])
    #expect(RedlampRoles.neutral(dark: false, accent: graphite).senders.map(\.hex)
        == [0x006CE2, 0x5719FF, 0xC900C5, 0xE20034, 0xAD5A00, 0x597A00, 0x008403, 0x007F62])
    #expect(RedlampRoles.neutral(dark: false, accent: .blue).senders.map(\.hex)
        == [0x006DE3, 0x4500FF, 0xC900C5, 0xE30034, 0xAD5A00, 0x597A00, 0x008203, 0x007F62], "an accent with a hue keeps its own")
    let neutral = Catalog.theme(id: "redlamp:neutral", omarchy: [], accent: graphite)
    #expect(Set(neutral?.dark?.senders.map(\.hex) ?? []).count == 8 && Set(neutral?.light?.senders.map(\.hex) ?? []).count == 8)
}
```

In `mac/Tests/JawThemesTests/RolesTests.swift`:

- The last line of `` `jaw's theme is the terminal's Tokyo Night by role` `` (line 9) becomes:

```swift
    #expect(dark.link == dark.accent && dark.accentText == dark.accent, "Tokyo Night's links and its accent drawn as text are its blue")
    #expect(dark.secondary.hex == 0x7E87AF && dark.tertiary.hex == 0x66709A,
            "its secondary text a step lighter for a card's well in your bubble, its dim text a shade lighter for its surface")
    let light = Catalog.jaw.light
    #expect(light.accent == RGBA(0x2E7DE9) && light.accentText.hex == 0x2D68C4 && light.link.hex == 0x2D68C4,
            "Tokyo Night Day's blue fills as it's written, and is a step deeper where it's drawn as text and as links, to read on its selection")
```

- The doc comment of `` `Jaw's Tokyo Night reads as every theme must, in both halves` `` (lines 39-40) becomes:

```swift
/// Tokyo Night Day reads as held: its dim text, raised from 2.54:1 on its ground to read at 3:1 there,
/// at 3.01:1, and its red, moved from F52A65 to C0316D to read at 3:1 on its selection, at 4.16:1 on
/// its ground.
```

In `mac/Tests/JawThemesTests/ThemeTests.swift`:

- A slot draws its theme's half as the hold leaves it: line 47, `#expect(Catalog.roles(.dark, of: both, omarchy: served, accent: .blue) == RedlampRoles.roles(nord.dark, dark: true))`, ends `== RedlampRoles.roles(nord.dark, dark: true).held())`; line 48 likewise ends `== RedlampRoles.roles(nord.light, dark: false).held())`; and line 51 ends `== Omarchy.roles(gruvbox.colours, dark: true)?.held())`.
- In `` `Neutral draws in the accent it's given, in each appearance, and Redlamp's families don't change with it` ``, line 82, `#expect(neutral.dark?.link == neutral.dark?.accent)`, becomes the lines below: Neutral's halves are now its mapping held, and `SystemAccent` carries the system's orange, green and red.

```swift
    #expect(neutral.dark == RedlampRoles.neutral(dark: true, accent: purple).held(), "its halves are Neutral's mapping in the purple, held")
    #expect(purple.orange == .orange && purple.green == .green && purple.red == .red,
            "an accent given without the system's other colours takes macOS 27.0.1's")
    #expect(SystemAccent.blue == SystemAccent(dark: RGBA(0x007AFF), light: RGBA(0x007AFF),
                                              orange: SystemColour(dark: RGBA(0xFF9230), light: RGBA(0xFF8D28)),
                                              green: SystemColour(dark: RGBA(0x30D158), light: RGBA(0x34C759)),
                                              red: SystemColour(dark: RGBA(0xFF4245), light: RGBA(0xFF383C))),
            "macOS 27.0.1's default accent, the same blue in both appearances, and its orange, green and red, as the build Mac draws them")
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawThemes`
Expected: exit 1, a build failure from the tests naming what isn't there yet, among them `'Hold' is not a member type of struct 'JawThemes.Roles'` from `HoldTests.swift`, `value of type 'Roles' has no member 'accentText'` and `cannot find 'SystemColour' in scope`.

- [ ] **Step 3: The hold, the accent drawn as text, and the system's colours**

Create `mac/JawThemes/Sources/Held.swift`:

```swift
import Foundation

/// The colours JawUI draws as text or as marks, held readable wherever it draws them, after a theme's
/// own mapping and whichever source it comes from (Owner, 6 Nov): each moved toward the text, by the
/// straight line `theme.rs` moves a colour along, until it reads at 3:1 on the worst of the grounds
/// JawUI draws it on, the tints it lays over a ground among them, a selected row's online at 4.5:1 on
/// its selection. The accent's fills (the unread badge, the tints themselves, the focus outline), the
/// grounds, the text and the borders keep the colours the theme wrote.
public extension Roles {
    /// A ground JawUI fills with a theme's colours: the window's (a conversation in lines, the composer,
    /// the panes, the library, the glance), the surface (the chat list, the palette, the status line,
    /// the box's field, their bubbles), a selected row, your bubbles, and the band a selected message
    /// is drawn on in lines.
    enum Base: String, CaseIterable, Sendable {
        case ground, surface, selection, yours, band
    }

    /// A ground JawUI draws text or marks on: a base, or a tint it lays over one.
    enum Ground: Hashable, Sendable {
        /// The base itself.
        case plain(Base)
        /// A card's well on it (`well`): a picture's, an album's, a link's (its thumbnail's placeholder
        /// too) or a contact card's, and the library's tiles and, on a row not selected, link icons.
        case well(Base)
        /// A quote's well on it (`quoteWell`).
        case quote(Base)
        /// The accent's tint on it (`accentTint`), behind a card's symbols, the library's file symbol
        /// on a row not selected, and its header's mark.
        case accentTint(Base)
        /// The box's chip (`chipTint`), on its field, the surface.
        case chip
        /// The colour drawn on it, at 16% (`ownTint(of:)`): a mention of you.
        case ownTint(Base)
    }

    /// A colour JawUI draws as text or as a mark.
    enum Ink: String, CaseIterable, Sendable {
        case secondary, tertiary, accentText, link, mention, online, selectedOnline, failed, senders
    }

    /// A colour, the grounds JawUI draws it on, and the contrast it's held at on each.
    struct Hold: Sendable, Hashable {
        public var ink: Ink
        public var on: [Ground]
        public var at: Double

        public init(_ ink: Ink, on: [Ground], at: Double = 3) {
            self.ink = ink
            self.on = on
            self.at = at
        }
    }

    /// Each colour and the grounds JawUI's draw sites put it on. Cards, with their wells and the
    /// accent's tints, sit on the window's ground, the surface, your bubbles and a selected message's
    /// band, quotes on the surface and your bubbles; a selected library row draws its file and link
    /// marks on the selection itself, without their tints. The dim text is held on the window's ground
    /// and the surface alone: everywhere else JawUI draws it, on a selected row, your bubbles, the band
    /// (`faint(on:)`) and a card's or a tile's well, its words and marks take the secondary text's
    /// colour, as the dim text held at 3:1 there would come level with the secondary text, or near it.
    /// A selected row draws online as `selectedOnline`.
    static let holds: [Hold] = {
        let bases = Base.allCases.map(Ground.plain)
        let wells: [Ground] = [.well(.ground), .well(.surface), .well(.yours), .well(.band)]
        let quotes: [Ground] = [.quote(.surface), .quote(.yours)]
        let tints: [Ground] = [.accentTint(.ground), .accentTint(.surface), .accentTint(.yours), .accentTint(.band)]
        return [
            Hold(.secondary, on: bases + wells + quotes),
            Hold(.tertiary, on: [.plain(.ground), .plain(.surface)]),
            Hold(.accentText, on: bases + wells + quotes + tints + [.chip]),
            Hold(.link, on: bases + wells),
            Hold(.mention, on: bases + [.ownTint(.ground), .ownTint(.surface), .ownTint(.yours), .ownTint(.band)]),
            Hold(.online, on: [.plain(.ground), .plain(.surface)]),
            Hold(.selectedOnline, on: [.plain(.selection)], at: 4.5),
            Hold(.failed, on: bases),
            Hold(.senders, on: bases + quotes),
        ]
    }()

    /// A card's well: the text at 7%, 6% in light.
    var well: RGBA {
        text.opacity(isDark ? 0.07 : 0.06)
    }

    /// A quote's well: the text at 6%, 5% in light.
    var quoteWell: RGBA {
        text.opacity(isDark ? 0.06 : 0.05)
    }

    /// The accent's tint behind its symbols: the accent at 15%.
    var accentTint: RGBA {
        accent.opacity(0.15)
    }

    /// The box's chip: the accent at 16%, 12% in light.
    var chipTint: RGBA {
        accent.opacity(isDark ? 0.16 : 0.12)
    }

    /// What a mention of you is drawn on: its own colour's tint.
    var mentionTint: RGBA {
        ownTint(of: mention)
    }

    /// The band a selected message is drawn on in lines: the selection at 55%.
    var bandTint: RGBA {
        selection.opacity(0.55)
    }

    /// The tint of a colour drawn on its own colour: it at 16%.
    func ownTint(of colour: RGBA) -> RGBA {
        colour.opacity(0.16)
    }

    /// A base's colour, the band as it's drawn on the window's ground.
    subscript(_ base: Base) -> RGBA {
        switch base {
        case .ground: ground
        case .surface: surface
        case .selection: selection
        case .yours: yours
        case .band: bandTint.over(ground)
        }
    }

    /// The colour of `place` with `colour` drawn on it, which a colour's own tint is made from.
    func colour(of place: Ground, under colour: RGBA) -> RGBA {
        switch place {
        case let .plain(base): self[base]
        case let .well(base): well.over(self[base])
        case let .quote(base): quoteWell.over(self[base])
        case let .accentTint(base): accentTint.over(self[base])
        case .chip: chipTint.over(surface)
        case let .ownTint(base): ownTint(of: colour).over(self[base])
        }
    }

    /// A colour's values: the eight senders', or the one.
    func colours(_ ink: Ink) -> [RGBA] {
        switch ink {
        case .secondary: [secondary]
        case .tertiary: [tertiary]
        case .accentText: [accentText]
        case .link: [link]
        case .mention: [mention]
        case .online: [online]
        case .selectedOnline: [selectedOnline]
        case .failed: [failed]
        case .senders: senders
        }
    }

    /// What JawUI draws the dim text's words and marks in on `base`: the dim text on the window's
    /// ground and the surface, where it's held, and the secondary text's colour on a selected row, your
    /// bubbles and a selected message's band, where only that is.
    func faint(on base: Base) -> RGBA {
        base == .ground || base == .surface ? tertiary : secondary
    }

    /// `colour` moved toward the text until it reads at `at` on each of `grounds`, one after another
    /// with `Contrast.fit`, in passes, each working the grounds out from the colour the pass starts
    /// from, as a colour's own tint moves with it, until a pass moves nothing; and how many passes
    /// moved it. A pass moves each channel toward the text's or leaves it, never past it, so no more
    /// than 765 can move a colour, three channels of 255 steps, and the loop ends at 766 whatever the
    /// grounds.
    func lifted(_ colour: RGBA, on grounds: [Ground], at: Double) -> (colour: RGBA, passes: Int) {
        let end = text.hex
        var now = colour
        for passes in 0 ..< 766 {
            let start = now
            let moved = grounds.reduce(start.hex) { hex, place in
                Contrast.fit(hex, on: self.colour(of: place, under: start).hex, toward: end, low: at, target: at)
            }
            if moved == start.hex { return (start, passes) }
            now = RGBA(moved)
        }
        return (now, 766)
    }

    /// These roles with each colour in `holds` lifted to read at its contrast on every one of its
    /// grounds. A colour that reads already is kept exactly as it is, so roles that are held come
    /// through unmoved.
    func held() -> Roles {
        var out = self
        for hold in Self.holds {
            func lift(_ colour: RGBA) -> RGBA {
                lifted(colour, on: hold.on, at: hold.at).colour
            }
            switch hold.ink {
            case .secondary: out.secondary = lift(secondary)
            case .tertiary: out.tertiary = lift(tertiary)
            case .accentText: out.accentText = lift(accentText)
            case .link: out.link = lift(link)
            case .mention: out.mention = lift(mention)
            case .online: out.online = lift(online)
            case .selectedOnline: out.selectedOnline = lift(selectedOnline)
            case .failed: out.failed = lift(failed)
            case .senders: out.senders = senders.map(lift)
            }
        }
        return out
    }
}
```

In `mac/JawThemes/Sources/Roles.swift`:

- `accent` and its doc comment (lines 21-22) become, with `accentText` after them:

```swift
    /// Focus, selection, unread counts: the accent's fills and outlines, as the theme wrote it.
    public var accent: RGBA
    /// The accent where JawUI draws it as text or as a mark: an unread chat's time, Draft:, a mention of
    /// someone else, Show more, You in a reply, the palette's and the library's marks, a card's action;
    /// the accent, until `held()` moves it to read where it's drawn.
    public var accentText: RGBA
```

- In both halves of Tokyo Night, `tokyoNight` (line 85) and `tokyoNightDay` (line 111), `accent: blue, link: blue,` becomes `accent: blue, accentText: blue, link: blue,`.
- `Catalog.jaw` and its doc comment (lines 61-63) become:

```swift
    /// jaw's own, first and the default: ooze's Tokyo Night, the terminal's palette, with Tokyo Night
    /// Day as its light half, both held as every theme is (`Roles.held()`). Redlamp's families join it in Mac-M5.
    public static let jaw = ThemeFamily(id: "jaw", name: "jaw", dark: tokyoNight.held(), light: tokyoNightDay.held())
```

In `mac/JawThemes/Sources/Omarchy.swift`'s `roles(_:dark:)`, line 68, `link: RGBA(surfaced(t.link)), yours: RGBA(t.accent).opacity(dark ? 0.18 : 0.12).over(RGBA(t.bg)),` becomes `accentText: RGBA(t.accent), link: RGBA(surfaced(t.link)), yours: RGBA(t.accent).opacity(dark ? 0.18 : 0.12).over(RGBA(t.bg)),`.

In `mac/JawThemes/Sources/Theme.swift`:

- `Theme`'s doc comment (lines 30-32) ends `…, and its halves as the roles Jaw draws in, a dark one, a light one or both, each held where JawUI draws it (`Roles.held()`), whichever source it comes from.`, and its initialiser (lines 42-49) becomes:

```swift
    public init(id: String, name: String, lightName: String? = nil, source: ThemeSource, dark: Roles?, light: Roles?) {
        self.id = id
        self.name = name
        self.lightName = lightName
        self.source = source
        self.dark = dark?.held()
        self.light = light?.held()
    }
```

  Every theme the catalog makes comes through here: Jaw's (`Catalog.jawTheme`, from `Catalog.jaw`, held already, which the hold leaves as it is), Omarchy's, each of Redlamp's families and Neutral.
- `SystemAccent` and its doc comment (lines 84-97) become:

```swift
/// One of the system's colours as AppKit draws it in a dark appearance and in a light one.
public struct SystemColour: Sendable, Hashable {
    public var dark: RGBA
    public var light: RGBA

    public init(dark: RGBA, light: RGBA) {
        self.dark = dark
        self.light = light
    }

    /// macOS's orange, green and red as AppKit draws them on macOS 27.0.1, as the build Mac does, which
    /// the tests and the harness draw Neutral in, so their pictures don't depend on the Mac's.
    public static let orange = SystemColour(dark: RGBA(0xFF9230), light: RGBA(0xFF8D28))
    public static let green = SystemColour(dark: RGBA(0x30D158), light: RGBA(0x34C759))
    public static let red = SystemColour(dark: RGBA(0xFF4245), light: RGBA(0xFF383C))
}

/// The system's colours Neutral takes, as AppKit draws them in a dark appearance and in a light one: the
/// accent chosen in System Settings, `dark` and `light`, and the orange, green and red its mentions,
/// online and failed take.
public struct SystemAccent: Sendable, Hashable {
    public var dark: RGBA
    public var light: RGBA
    public var orange: SystemColour
    public var green: SystemColour
    public var red: SystemColour

    public init(dark: RGBA, light: RGBA, orange: SystemColour = .orange, green: SystemColour = .green, red: SystemColour = .red) {
        self.dark = dark
        self.light = light
        self.orange = orange
        self.green = green
        self.red = red
    }

    /// macOS's blue, the accent until another is chosen in System Settings, which AppKit resolves to
    /// `007AFF` in both appearances on macOS 27.0.1, as the build Mac draws it, with macOS's orange,
    /// green and red: what the tests and the harness draw Neutral in, so their pictures don't depend on
    /// the Mac's.
    public static let blue = SystemAccent(dark: RGBA(0x007AFF), light: RGBA(0x007AFF))
}
```

- `Catalog.neutral` and its doc comment (lines 152-156) become:

```swift
    /// Neutral, `redlamp:neutral`, in the system's colours.
    static func neutral(_ accent: SystemAccent) -> Theme {
        Theme(id: "redlamp:neutral", name: "Neutral", source: .redlamp, dark: RedlampRoles.neutral(dark: true, accent: accent),
              light: RedlampRoles.neutral(dark: false, accent: accent))
    }
```

- [ ] **Step 4: Redlamp's red, and Neutral in the system's colours, its senders in colour under a grey accent**

`mac/JawThemes/Sources/RedlampRoles.swift` becomes:

```swift
import Foundation

/// jaw's roles from Redlamp's families, by the Mac design's table, then held as Omarchy's are: every
/// colour at 3:1 against the ground and the surface, secondary text too, the dim text at 2.5 to 4.5:1,
/// and the selection and your bubbles moved toward their ground until text reads at 4.5:1 on them, so
/// a family made for a photo editor's panels reads in a chat, readable over exact (Owner, 6 Nov).
public enum RedlampRoles {
    /// A family's half: `primary`, or `neutral` where `primaryRoles` keeps it out, takes the accent and
    /// links, the selection at 20% over `card` (16% in light) and your bubbles at 18% over `background`
    /// (12%); `accent` takes mentions, `success` online and `destructive` failed; the senders are eight
    /// hues from `primary`'s (`hues(from:rare:)`). The Redlamp family keeps the brand's red to its
    /// accent (Owner, 6 Nov): its light half's `accent` is the safelight itself, so its mentions take the
    /// brand's filament, as its dark half's do, and its senders' hues keep away from the red.
    public static func roles(_ p: ThemePalette, dark: Bool) -> Roles {
        let tint = p.primaryRoles.contains(.selection) ? p.primary : p.neutral
        return held(
            dark: dark, ground: p.background, surface: p.card, text: p.foreground,
            selection: tint.opacity(dark ? 0.2 : 0.16).over(p.card), yours: tint.opacity(dark ? 0.18 : 0.12).over(p.background),
            secondary: p.muted, tertiary: p.muted.opacity(0.28 / 0.45).over(p.background), separator: p.border,
            accent: p.primaryRoles.contains(.accent) ? p.primary : p.neutral,
            mention: p.accent == RedlampCatalog.Brand.safelight ? RedlampCatalog.Brand.filament : p.accent, online: p.success,
            failed: p.destructive, hue: p.primary, rare: !p.primaryRoles.contains(.selection),
        )
    }

    /// Neutral, Redlamp's own grey: its standard tokens' greys at their alphas over its panel, white
    /// over 11.5% grey in dark and black over 92.5% in light; the system's colours, as AppKit draws them
    /// in the appearance (`SystemAccent`), its accent the system's, which Redlamp's Neutral leaves its
    /// accent to, and its orange, green and red for mentions, online and failed, held as a family's are;
    /// its senders' hues from the accent's, or from the system's blue where the accent is a grey
    /// (`tinted(_:dark:)`).
    public static func neutral(dark: Bool, accent system: SystemAccent) -> Roles {
        let accent = dark ? system.dark : system.light
        let ink = dark ? RGBA(0xFFFFFF) : RGBA(0x000000)
        let ground = dark ? RGBA(r: 0.115, g: 0.115, b: 0.115) : RGBA(r: 0.925, g: 0.925, b: 0.925)
        let surface = RGBA(0xFFFFFF).opacity(dark ? 0.05 : 0.55).over(ground)
        return held(
            dark: dark, ground: ground, surface: surface, text: ink.opacity(dark ? 0.9 : 0.88).over(ground),
            selection: ink.opacity(dark ? 0.1 : 0.08).over(surface), yours: accent.opacity(dark ? 0.18 : 0.12).over(ground),
            secondary: ink.opacity(dark ? 0.45 : 0.5).over(ground), tertiary: ink.opacity(dark ? 0.28 : 0.3).over(ground),
            separator: ink.opacity(dark ? 0.07 : 0.09).over(ground), accent: accent,
            mention: dark ? system.orange.dark : system.orange.light, online: dark ? system.green.dark : system.green.light,
            failed: dark ? system.red.dark : system.red.light, hue: tinted(accent, dark: dark),
        )
    }

    /// The roles from what a half gives each, held: colours at 3:1 on the ground and then the surface,
    /// the dim text in its band, the selection and your bubbles under text, a selected row's online at
    /// 4.5:1 on its selection, links and the accent drawn as text as the accent, the borders as
    /// authored, and the senders from `hue`, kept away from it where `rare`.
    static func held(dark: Bool, ground: RGBA, surface: RGBA, text: RGBA, selection: RGBA, yours: RGBA, secondary: RGBA,
                     tertiary: RGBA, separator: RGBA, accent: RGBA, mention: RGBA, online: RGBA, failed: RGBA, hue: RGBA,
                     rare: Bool = false) -> Roles
    {
        let (g, s, t) = (ground.hex, surface.hex, text.hex)
        func both(_ c: RGBA) -> UInt32 {
            Contrast.fit(Contrast.fit(c.hex, on: g, toward: t, low: 3, target: 3), on: s, toward: t, low: 3, target: 3)
        }
        let picked = under(t, selection.hex, toward: s)
        let on = both(online)
        let a = RGBA(both(accent))
        return Roles(
            isDark: dark, ground: RGBA(g), surface: RGBA(s), selection: RGBA(picked), text: RGBA(t), secondary: RGBA(both(secondary)),
            tertiary: RGBA(Contrast.fit(tertiary.hex, on: g, toward: t, low: 2.5, high: 4.5, target: 3)), separator: RGBA(separator.hex),
            accent: a, accentText: a, link: a, yours: RGBA(under(t, yours.hex, toward: g)), mention: RGBA(both(mention)), online: RGBA(on),
            selectedOnline: RGBA(Contrast.fit(on, on: picked, toward: t, low: 4.5, target: 4.5)), failed: RGBA(both(failed)),
            senders: senders(from: hue, on: s, dark: dark, rare: rare),
        )
    }

    /// `fill` moved toward `ground` until `text` reads at 4.5:1 on it.
    static func under(_ text: UInt32, _ fill: UInt32, toward ground: UInt32) -> UInt32 {
        guard Contrast.ratio(text, fill) < 4.5 else { return fill }
        var (near, far) = (0.0, 1.0)
        for _ in 0 ..< 30 {
            let t = (near + far) / 2
            if Contrast.ratio(text, Contrast.mix(fill, ground, t)) < 4.5 {
                near = t
            } else {
                far = t
            }
        }
        return Contrast.mix(fill, ground, far)
    }

    /// Eight hues from `primary`'s (`hues(from:rare:)`), its saturation and lightness kept, each
    /// lightened in dark or darkened in light half a percent at a time until it reads at 4.5:1 on
    /// `surface`.
    static func senders(from primary: RGBA, on surface: UInt32, dark: Bool, rare: Bool = false) -> [RGBA] {
        let (hue, saturation, lightness) = hsl(primary)
        return hues(from: hue, rare: rare).map { h in
            var l = lightness
            var colour = RGBA(hue: h, saturation: saturation, lightness: l).hex
            var steps = 0
            while Contrast.ratio(colour, surface) < 4.5, steps < 200 {
                l = min(max(l + (dark ? 0.005 : -0.005), 0), 1)
                colour = RGBA(hue: h, saturation: saturation, lightness: l).hex
                steps += 1
            }
            return RGBA(colour)
        }
    }

    /// The senders' eight hues from `primary`'s: 45° apart, the first `primary`'s own; or, for a half
    /// that keeps `primary` rare, as the Redlamp family keeps the brand's red to its accent, 37.5° apart
    /// over the 300° outside 30° either side of it, from 48.75° past it to 48.75° short of it, so no
    /// sender's name takes the red (Owner, 6 Nov).
    static func hues(from primary: Double, rare: Bool) -> [Double] {
        (0 ..< 8).map { k in rare ? primary + 48.75 + 37.5 * Double(k) : primary + 45 * Double(k) }
    }

    /// What Neutral's senders take their hues from: the system's accent, or, where that's a grey, under
    /// 0.2 saturation as Graphite's is, the system's blue at the accent's lightness, so a group's eight
    /// senders still come out as eight colours (Owner, 6 Nov).
    static func tinted(_ accent: RGBA, dark: Bool) -> RGBA {
        let (_, saturation, lightness) = hsl(accent)
        guard saturation < 0.2 else { return accent }
        let blue = hsl(dark ? SystemAccent.blue.dark : SystemAccent.blue.light)
        return RGBA(hue: blue.hue, saturation: blue.saturation, lightness: lightness)
    }

    /// A colour's hue in degrees, its saturation and its lightness, as HSL has them.
    static func hsl(_ c: RGBA) -> (hue: Double, saturation: Double, lightness: Double) {
        let (high, low) = (max(c.r, c.g, c.b), min(c.r, c.g, c.b))
        let lightness = (high + low) / 2
        guard high > low else { return (0, 0, lightness) }
        let d = high - low
        let saturation = lightness > 0.5 ? d / (2 - high - low) : d / (high + low)
        let hue: Double = switch high {
        case c.r: (c.g - c.b) / d + (c.g < c.b ? 6 : 0)
        case c.g: (c.b - c.r) / d + 2
        default: (c.r - c.g) / d + 4
        }
        return (hue * 60, saturation, lightness)
    }
}
```

What changes from Task 3's: the mention of a half whose `accent` field is the brand's safelight, which only the Redlamp family's light half has; the senders' hues where a half keeps `primary` out of its selection, which only the Redlamp family does, `45 * Double(k)` being `Double(k) * 45` exactly, so every other family's senders are the same to the bit; Neutral taking the system's colours whole, and its senders' hue under a grey accent; and `accentText` beside `link`.

- [ ] **Step 5: Run JawThemes' tests**

Run: `mac/scripts/build-mac test JawThemes`
Expected: exit 0, `** TEST SUCCEEDED **`, `✔ Test run with 24 tests in 0 suites passed`: Task 4 left 18, and `HoldTests.swift` adds four and `RedlampTests.swift` two. If `heldMoves`, `heldSenders` or a pinned colour differs from what the code finds, check `Roles.held()`, `Roles.colour(of:under:)` and `RedlampRoles` against Steps 3 and 4 and the table above before changing a list, and say so in the report: the lists are what a port of the arithmetic found on 6 November, the port first checked against what Tasks 2 and 3 pin.

- [ ] **Step 6: Write JawUI's failing tests**

Create `mac/Tests/JawUITests/HeldInkTests.swift`:

```swift
import AppKit
import CoreText
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// Where JawUI draws a theme's accent as text it takes the accent's held colour, and the accent's fills
/// the colour the theme wrote; the dim text's words and marks take the secondary text's colour
/// everywhere but the window's ground and the surface, where only the secondary text is held: on your
/// bubbles, a selected row, a selected message's band and a well; the tints JawUI lays over a ground
/// are the ones the hold reads the colours on them on (Owner, 6 Nov); and three small marks are drawn
/// on one ground, not on a tint on a tint.

/// The colours of a line's runs.
func runColours(_ line: CTLine) -> [CGColor] {
    (CTLineGetGlyphRuns(line) as! [CTRun]).compactMap { run in
        (CTRunGetAttributes(run) as NSDictionary)[NSAttributedString.Key.ctColour].map { $0 as! CGColor }
    }
}

/// The colour of the first ticks a layout draws.
func tickColour(_ l: Laid) -> CGColor? {
    l.parts.lazy.compactMap { part -> CGColor? in if case let .ticks(_, _, colour, _) = part { colour } else { nil } }.first
}

@MainActor @Test func `the accent drawn as text takes its held colour, and the unread count's badge the accent the theme wrote`() throws {
    var roles = Catalog.jaw.dark
    roles.accentText = RGBA(0xFF00FF)
    #expect(Tone.accent.color(roles) == roles.accentText, "a key, as KeyCap draws one")
    let cell = ChatCell(frame: NSRect(x: 0, y: 0, width: 300, height: 56))
    cell.show(Samples.direct, typists: [], roles: roles, open: false, inCommunity: false, avatar: nil)
    let draft = cell.line(Samples.direct, roles)
    #expect(draft.string.hasPrefix("Draft: ") && draft.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor == roles.accentText.nsColor)
    #expect(cell.badges(Samples.group, roles).first?.fill == roles.accent, "the unread count's badge is the accent's fill")
    let s = LayoutSpec(style: Style(roles: roles), me: ["person:1"])
    let mentions = try #require(Samples.messages.first { $0.spans.contains { $0.t == .mention } })
    let frame = try #require(laid(mentions, s).text?.frame)
    let runs = (CTFrameGetLines(frame) as! [CTLine]).flatMap(runColours)
    #expect(runs.contains(roles.accentText.cgColor) && runs.contains(roles.mention.cgColor) && !runs.contains(roles.accent.cgColor),
            "@Rui in the accent's held colour, @Pedro, a mention of you, in the mention colour")
    var reply = try #require(Samples.messages.first { $0.reply != nil })
    reply.reply = Quote(id: "3EB0000000000002", from: "You", summary: "did you get the photos from Sunday?")
    let quoted = laid(reply, s)
    #expect(quoted.parts.contains { if case let .fill(rect, colour, _) = $0 { rect.width == 3 && colour == roles.accentText.cgColor } else { false } },
            "You in a reply: its bar")
    #expect(quoted.parts.contains { if case let .line(l, _) = $0 { runColours(l) == [roles.accentText.cgColor] } else { false } }, "and its name")
    var context = MessageContext(group: false, firstOfRun: true, showTime: true, day: nil)
    context.sending = ["OUT": Sending(step: .uploading, percent: 40)]
    let going = MessageLayout.lay(outgoing(.document), width: 640, context: context, style: s.style, fonts: s.fonts, me: s.me)
    let rings = going.parts.compactMap { part -> CGColor? in if case let .ring(_, _, _, colour, _) = part { colour } else { nil } }
    #expect(rings.contains(roles.accentText.cgColor) && rings.contains(roles.accent.opacity(0.25).cgColor) && !rings.contains(roles.accent.cgColor),
            "a file of yours on its way, as a download: its progress in the accent drawn as text, its track the accent at 25%")
    let marks = going.parts.compactMap { part -> Data? in if case let .image(image, _) = part { image.dataProvider?.data as Data? } else { nil } }
    let x = try #require(Symbols.image("xmark", 9, roles.accentText)).0.dataProvider?.data as Data?
    #expect(x != nil && marks.contains { $0 == x }, "and its × in the accent drawn as text")
}

@MainActor @Test func `the dim text's marks take the secondary text's colour on your bubbles and a selected row, and the dim text's own on the ground and the surface`() throws {
    let roles = Catalog.jaw.dark
    #expect(Ticks.of(.delivered, roles)?.1 == roles.tertiary && Ticks.of(.sent, roles, on: .yours)?.1 == roles.secondary)
    #expect(Ticks.of(.read, roles, on: .selection)?.1 == roles.link, "read ticks in the link colour wherever they're drawn")
    var direct = Samples.direct
    direct.draft = nil
    direct.last?.status = .delivered
    let cell = ChatCell(frame: NSRect(x: 0, y: 0, width: 300, height: 56))
    cell.show(direct, typists: [], roles: roles, open: false, inCommunity: false, avatar: nil)
    #expect(cell.ticks(direct, roles)?.1 == roles.tertiary, "on the chat list's surface")
    cell.isSelected = true
    #expect(cell.ticks(direct, roles)?.1 == roles.secondary, "on a selected row")
    direct.last?.status = .pending
    let pending = cell.line(direct, roles)
    #expect(pending.string.hasPrefix("◷ ") && pending.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor == roles.secondary.nsColor)
    let sent = try #require(Samples.messages.first { $0.mine && $0.status == .sent })
    #expect(tickColour(laid(sent, look: .bubbles)) == roles.secondary.cgColor, "in your bubble")
    #expect(tickColour(laid(sent, look: .lines)) == roles.tertiary.cgColor, "on the window's ground, in lines")
    let emoji = try #require(Samples.messages.first { $0.mine && $0.status == .delivered && $0.kind == .text })
    #expect(tickColour(laid(emoji, look: .bubbles)) == roles.tertiary.cgColor, "emoji alone, under no bubble")
}

@MainActor @Test func `a message selected in lines has its dim marks drawn in the secondary text's colour on its band, as the conversation tells its layout`() async throws {
    let roles = Catalog.jaw.dark
    let sent = try #require(Samples.messages.first { $0.mine && $0.status == .sent })
    let lines = spec(.lines, dark: true)
    func ticks(selected: Bool) -> CGColor? {
        var context = MessageContext(group: false, firstOfRun: true, showTime: true, day: nil)
        context.selected = selected
        return tickColour(MessageLayout.lay(sent, width: 640, context: context, style: lines.style, fonts: lines.fonts, me: lines.me))
    }
    #expect(ticks(selected: false) == roles.tertiary.cgColor && ticks(selected: true) == roles.secondary.cgColor)
    let host = PaneHost()
    let (c, window) = await shown(Samples.messages, host)
    defer { window.close() }
    c.spec = lines
    let i = try #require(c.rows.firstIndex { $0.id == sent.id })
    c.select(sent.id)
    #expect(c.contexts[i].selected, "selected in lines: its layout is told")
    #expect(try messageView(c, i).laid.flatMap(tickColour) == roles.secondary.cgColor, "and its ticks are drawn on its band")
    c.select(nil)
    #expect(!c.contexts[i].selected)
    c.spec = spec(.bubbles, dark: true)
    c.select(sent.id)
    #expect(!c.contexts[i].selected, "in bubbles the band is behind the bubble, whose ground its words keep")
}

@MainActor @Test func `the tints JawUI lays over a ground are the ones the hold reads the colours drawn on them on, in both halves`() throws {
    func fills(_ l: Laid) -> [CGColor] {
        l.parts.compactMap { part -> CGColor? in if case let .fill(_, colour, _) = part { colour } else { nil } }
    }
    for dark in [true, false] {
        let s = spec(.bubbles, dark: dark)
        let roles = s.style.roles
        let half = dark ? "dark" : "light"
        #expect(fills(laid(sampleLink("L2"), s)).contains(roles.well.cgColor), "\(half): a link card's well")
        let reply = try #require(Samples.messages.first { $0.reply != nil })
        #expect(fills(laid(reply, s)).contains(roles.quoteWell.cgColor), "\(half): a quote's")
        let file = try #require(Samples.messages.first { $0.kind == .document })
        #expect(fills(laid(file, s)).contains(roles.accentTint.cgColor), "\(half): the accent's, behind a file's symbol")
        let mentions = try #require(Samples.messages.first { $0.spans.contains { $0.t == .mention } })
        let frame = try #require(laid(mentions, s).text?.frame)
        let tints = (CTFrameGetLines(frame) as! [CTLine]).flatMap { line in
            (CTLineGetGlyphRuns(line) as! [CTRun]).compactMap { run in
                (CTRunGetAttributes(run) as NSDictionary)[NSAttributedString.Key.jawBackground].map { $0 as! CGColor }
            }
        }
        #expect(!tints.isEmpty && tints.allSatisfy { $0 == roles.mentionTint.cgColor }, "\(half): a mention of you, on its own colour's tint")
        let view = MessageView(frame: NSRect(x: 0, y: 0, width: 640, height: 200))
        view.roles = roles
        view.laid = laid(sampleLink("L2"), s)
        view.selected = true
        let l = try #require(view.laid)
        let (w, h) = (640, Int(ceil(l.height)))
        let ctx = try #require(CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                                         space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        ctx.setFillColor(roles.ground.cgColor)
        ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
        ctx.translateBy(x: 0, y: CGFloat(h))
        ctx.scaleBy(x: 1, y: -1)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: true)
        view.paint(view.bounds)
        NSGraphicsContext.restoreGraphicsState()
        let pixel = try #require(ctx.data).assumingMemoryBound(to: UInt8.self) + (Int(l.body.midY) * w + w - 3) * 4
        let band = roles[.band].hex
        let off = [(pixel[0], band >> 16), (pixel[1], band >> 8), (pixel[2], band)].map { abs(Int($0.0) - Int($0.1 & 0xFF)) }
        #expect(off.allSatisfy { $0 <= 2 }, "\(half): a selected message's band beside their bubble, the selection at 55% over the window's ground, off by \(off)")
    }
}

@MainActor @Test func `a link card's thumbnail waits on the card's own well, its link symbol drawn there, with no well of its own`() throws {
    for look in [Look.bubbles, .lines] {
        let s = spec(look, dark: true)
        let roles = s.style.roles
        for id in ["L1", "L3"] {
            let l = laid(sampleLink(id), s)
            let wells = l.parts.filter { if case let .fill(_, colour, _) = $0 { colour == roles.well.cgColor } else { false } }
            #expect(wells.count == 1, "\(id) in \(look): the card's well alone, the placeholder's symbol on it")
            #expect(l.pictures.count == 1, "\(id) in \(look): the thumbnail's slot, which its picture fills when it comes")
        }
    }
}

@MainActor @Test func `a selected row of the library draws a file's symbol and a link's icon on the selection, without their tints`() throws {
    for look in [Look.bubbles, .lines] {
        let style = libraryStyle(look)
        let roles = style.roles
        func fills(_ d: LibraryDrawn) -> [CGColor] {
            d.under.compactMap { part -> CGColor? in if case let .fill(_, colour, _) = part { colour } else { nil } }
        }
        for (kind, tint) in [(LibraryKind.file, roles.accentTint), (.link, roles.well)] {
            let row = try #require(rows(libraryLaid(kind, style: style)).first)
            #expect(fills(drawn(row, style: style)).contains(tint.cgColor), "\(kind) in \(look): on its tint, not selected")
            let selected = fills(drawn(row, style: style, selected: true))
            #expect(!selected.contains(tint.cgColor) && selected.contains(roles.selection.cgColor),
                    "\(kind) in \(look): selected, on the selection itself")
        }
    }
}

@MainActor @Test func `the dim text drawn on a well takes the secondary text's colour: a link card's host, and a tile's placeholder`() throws {
    let s = spec(.bubbles, dark: true)
    let roles = s.style.roles
    let card = laid(sampleLink("L2"), s)
    let runs = card.parts.flatMap { part -> [CGColor] in if case let .line(l, _) = part { runColours(l) } else { [] } }
    #expect(!runs.contains(roles.tertiary.cgColor),
            "the host, the card's one word in the dim text's colour before, now in the secondary text's: nothing in the card in the dim text's")
    var photo = try #require(Samples.library.first { $0.kind == .photo })
    photo.media = nil
    let c = drawingContext()
    let tile = LibraryDrawing.tile(photo, size: CGSize(width: 120, height: 120), selected: false, c)
    let symbols = tile.under.compactMap { part -> Data? in if case let .image(image, _) = part { image.dataProvider?.data as Data? } else { nil } }
    let secondary = try #require(Symbols.image("photo", 20, c.roles.secondary)).0.dataProvider?.data as Data?
    #expect(secondary != nil && symbols.contains { $0 == secondary }, "a tile with no picture yet: its placeholder in the secondary text's colour, on its well")
}
```

In `mac/Tests/JawUITests/ComponentTests.swift`, line 20, `#expect(Tone.accent.color(roles) == roles.accent)` becomes `#expect(Tone.accent.color(roles) == roles.accentText, "text in the accent takes its held colour")`.

In `mac/Tests/JawUITests/PreferencesTests.swift`, line 33, `#expect(preferences.style(dark: true).roles == RedlampRoles.roles(nord.dark, dark: true))` ends `== RedlampRoles.roles(nord.dark, dark: true).held())`.

- [ ] **Step 7: Run them to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 1, a build failure from `HeldInkTests.swift`, among them `extra argument 'on' in call` and `value of type 'MessageContext' has no member 'selected'`.

- [ ] **Step 8: Draw the tints from the roles**

The tints JawUI lays over a ground are taken from `Roles`, where the hold reads them, in place of the opacities each draw site writes itself:

- `mac/JawUI/Sources/Layout/Cards.swift`: the well, lines 271-273, `var well: RGBA {` over `roles.text.opacity(roles.isDark ? 0.07 : 0.06)`, becomes `var well: RGBA {` over `roles.well`; and the accent's tints behind a file's symbol (line 538, `parts.append(.fill(icon, roles.accent.opacity(0.15).cgColor, 6))`), the action of a file of yours on its way (line 563, main's, `parts.append(.fill(action, roles.accent.opacity(0.15).cgColor, 14))`), a file's action to download (line 570, the same) and a place's symbol (line 589) take `roles.accentTint.cgColor` where they take `roles.accent.opacity(0.15).cgColor`. The rings' tracks at lines 565 and 574, the accent at 25%, stay.
- `mac/JawUI/Sources/Library/LibraryDrawing.swift`: `well(_:)`, lines 150-152, returns `c.roles.well` where it returns `c.roles.text.opacity(c.roles.isDark ? 0.07 : 0.06)`; a file's tint, line 280, takes `r.accentTint.cgColor` where it takes `r.accent.opacity(0.15).cgColor`.
- `mac/JawUI/Sources/Library/LibraryController.swift`: the header's mark, line 919, `mark.layer?.backgroundColor = roles.accent.opacity(0.15).cgColor`, becomes `mark.layer?.backgroundColor = roles.accentTint.cgColor`.
- `mac/JawUI/Sources/Layout/MessageLayout.swift`: a mention of you, line 495, `if mine { a[.jawBackground] = roles.mention.opacity(0.16).cgColor }`, becomes `if mine { a[.jawBackground] = roles.mentionTint.cgColor }`; a quote's well, line 750, `parts.append(.fill(rect, roles.text.opacity(roles.isDark ? 0.06 : 0.05).cgColor, 8))`, becomes `parts.append(.fill(rect, roles.quoteWell.cgColor, 8))`.
- `mac/JawUI/Sources/Conversation/MessageView.swift`: a selected message's band, line 104, `ctx.setFillColor(roles.selection.opacity(0.55).cgColor)`, becomes `ctx.setFillColor(roles.bandTint.cgColor)`.
- `mac/JawUI/Sources/Components/Glyphs.swift`: the chip, line 162, `layer.backgroundColor = roles.accent.opacity(roles.isDark ? 0.16 : 0.12).cgColor`, becomes `layer.backgroundColor = roles.chipTint.cgColor`.

The drop hint's tint in `GridView.swift`, which carries no words, and the scrims over pictures stay as they are.

- [ ] **Step 9: Draw the accent as text in its held colour**

The accent's fills stay `roles.accent`: the badges, the tints, a filled button, the voice note's disc, whose ▶, ♪ or × is in the ground's colour, the poll's discs and its bar, the rings' tracks, the focus, selection and reaction outlines, the drop hint, the pane outline and the composer's caret. Where it's drawn as text or as a mark, `roles.accent` becomes `roles.accentText` (`c.roles.accent` becomes `c.roles.accentText`, `r.accent` becomes `r.accentText`):

- `mac/JawUI/Sources/ChatList/ChatListController.swift`: an unread chat's time, line 435 (`chat.unread > 0 && !chat.muted ? roles.accent : roles.secondary`), and Draft:, line 517 (`[.foregroundColor: roles.accent.nsColor]`).
- `mac/JawUI/Sources/Components/Glyphs.swift`: the chip's name, line 148 (`name.textColor = roles.accent.nsColor`); its doc comment, line 105, says `its picture beside its name in the accent drawn as text:` where it said `its picture beside its name in the accent:`.
- `mac/JawUI/Sources/Components/Tone.swift`, line 14: `case .accent: roles.accent` becomes `case .accent: roles.accentText`, as a tone is a piece of text's or a symbol's colour.
- `mac/JawUI/Sources/Layout/MessageLayout.swift`: a mention of someone else, line 494 (`(mine ? roles.mention : roles.accent)`), Show more, line 666 (`line("Show more", fonts.small, roles.accent)`), and You in a reply in bubbles, line 751, and in lines, line 806 (`reply.from == "You" ? roles.accent : roles.sender(Self.colourIndex(reply.from))`), its name and its bar both.
- `mac/JawUI/Sources/Layout/Cards.swift`: what can be done about a file, line 465 (`.ctColour: roles.accent.cgColor`); a file's symbol, line 544; a file of yours on its way, main's, its progress ring, line 566 (`max(CGFloat(going.percent) / 100, 0.02), roles.accent.cgColor`), and its ×, line 567 (`symbol("xmark", 9, roles.accent, centredIn: action)`), its track at line 565 staying the accent at 25%; a download's progress ring, line 575 (`max(m.media?.progress ?? 0, 0.02), roles.accent.cgColor`), its track at line 574 staying the accent at 25%; the download's symbols, lines 576, 579 and 581; and a place's symbol, line 590.
- `mac/JawUI/Sources/Layout/ContactCards.swift`: a contact card's action, line 129 (`line(action, fonts.body, roles.accent)`).
- `mac/JawUI/Sources/Palette/Palette.swift`: the strip's kind in use, lines 514 and 517 (`? roles.accent : roles.secondary`); a picked chat's tick, line 574; a row's key, line 580; and the marks in a title, line 593 (`span.t == .mark ? roles.accent : roles.text`).
- `mac/JawUI/Sources/Library/LibraryDrawing.swift`: the marks in a caption, line 271 (`mark ? c.roles.accent : c.roles.secondary`), and a file's symbol, line 281 (`symbol(Self.fileSymbol(f), 13, r.accent, …`).
- `mac/JawUI/Sources/Library/LibraryController.swift`: the header's mark, line 921 (`hierarchicalColor: roles.accent.nsColor`).
- `mac/JawUI/Sources/Views/Views.swift`: the status line's state, line 209 (`warn ? roles.mention : roles.accent`).
- `mac/JawUI/Sources/Window/GlanceView.swift`: You in the glance, line 94 (`m.mine ? roles.accent : roles.sender(m.from?.colour ?? 0)`).

- [ ] **Step 10: Draw the dim text in the secondary text's colour on your bubbles, a selected row, a selected message's band and a well**

In `mac/JawUI/Sources/Components/Glyphs.swift`, `Ticks.of` (lines 38-47) becomes:

```swift
    /// How many ticks a status has, and in which colour: one sent and two delivered in the dim text's
    /// colour on `base` (`Roles.faint(on:)`), two in the link colour read; nil for the states said in
    /// words or symbols.
    public static func of(_ status: MessageStatus?, _ roles: Roles, on base: Roles.Base = .surface) -> (Int, RGBA)? {
        switch status {
        case .sent: (1, roles.faint(on: base))
        case .delivered: (2, roles.faint(on: base))
        case .read, .played: (2, roles.link)
        case .pending, .failed, nil: nil
        }
    }
```

In `mac/JawUI/Sources/ChatList/ChatListController.swift`'s `ChatCell`:

- after `isSelected` (lines 381-383), add:

```swift

    /// The ground the row's words and marks are drawn on: the selection while it's selected, the
    /// surface otherwise.
    var place: Roles.Base {
        isSelected ? .selection : .surface
    }
```

- the pin and the mute mark, lines 442 and 445, take `colour: roles.faint(on: place)` where they take `colour: roles.tertiary`;
- `ticks(_:_:)`, line 494, `return Ticks.of(last.status, roles)` becomes `return Ticks.of(last.status, roles, on: place)`;
- the pending clock, line 531, takes `roles.faint(on: place).nsColor` where it takes `roles.tertiary.nsColor`.

In `mac/JawUI/Sources/Layout/MessageLayout.swift`:

- `MessageContext` learns whether its message is selected, as main's `sending` is kept: its doc comment (lines 47-49) becomes

```swift
/// What a message's lines depend on beyond the message: whether its sender's name and its time
/// show, the day above it, whether a long one is unfolded, how far its files of yours on their way
/// have got, and whether it's selected in lines.
```

  and after `public var sending: [String: Sending] = [:]` (line 62) comes

```swift
    /// The message is selected in lines, where it's drawn on the selection's band, so its foot and its
    /// card draw their dim marks as on a selected row; in bubbles it keeps its bubble's ground.
    public var selected = false
```

  Its initialiser stays as it is: every place that makes a context leaves it false but the conversation's controller, below.
- in the builder, after `init`'s closing brace (after `roles = style.roles` at line 376 and the `}` under it), add:

```swift

        /// The ground a message's words, foot and card are drawn on: your bubble or theirs in bubbles;
        /// in lines the window's ground, or the band a selected message is drawn on.
        var cardGround: Roles.Base {
            style.look == .bubbles ? (m.mine ? .yours : .surface) : context.selected ? .band : .ground
        }
```

- `stateMark()`, `footParts(_:at:)` and `footer(time:)` (lines 586-629) take the ground their foot is drawn on, the window's unless they're told otherwise:

```swift
        /// What's said for your message's state in text, on `base`: pending and failed; ticks are drawn.
        func stateMark(on base: Roles.Base) -> (String, RGBA)? {
            guard m.mine, let status = m.status else { return nil }
            switch status {
            case .pending: return ("◷", roles.faint(on: base))
            case .failed: return ("!", roles.failed)
            case .sent, .delivered, .read, .played: return nil
            }
        }

        var tickSize: CGFloat {
            CTFontGetSize(fonts.time) + 1
        }

        /// A foot line drawn at a point on `base`, with the ticks drawn in the room it leaves for them.
        func footParts(_ foot: CTLine, at point: CGPoint, on base: Roles.Base = .ground) -> [Laid.Part] {
            var out: [Laid.Part] = [.line(foot, point)]
            if m.mine, let (count, colour) = Ticks.of(m.status, roles, on: base) {
                let w = Ticks.width(count, size: tickSize)
                out.append(.ticks(count, CGPoint(x: point.x + width(of: foot) - w, y: point.y), colour.cgColor, tickSize))
            }
            return out
        }

        /// The time and what's said around it on `base`: a star, Edited, your ticks.
        func footer(time: Bool = true, on base: Roles.Base = .ground) -> CTLine {
            let out = NSMutableAttributedString()
            let quiet: [NSAttributedString.Key: Any] = [.ctFont: fonts.time, .ctColour: roles.secondary.cgColor]
            if m.starred { out.append(NSAttributedString(string: "★ ", attributes: quiet)) }
            if m.edited, !m.deleted { out.append(NSAttributedString(string: "Edited ", attributes: [.ctFont: fonts.smallItalic, .ctColour: roles.secondary.cgColor])) }
            if time { out.append(NSAttributedString(string: Format.clock(m.at), attributes: quiet)) }
            if let (mark, colour) = stateMark(on: base) {
                out.append(NSAttributedString(string: " " + mark, attributes: [.ctFont: fonts.small, .ctColour: colour.cgColor]))
            } else if m.mine, let (count, _) = Ticks.of(m.status, roles) {
                // Room for the ticks: an invisible x, kerned to their width, since trailing spaces
                // aren't measured.
                let x = NSAttributedString(string: "x", attributes: [.ctFont: fonts.time])
                let xWidth = width(of: CTLineCreateWithAttributedString(x))
                out.append(NSAttributedString(string: " ", attributes: [.ctFont: fonts.time]))
                out.append(NSAttributedString(string: "x", attributes: [.ctFont: fonts.time, .ctColour: CGColor.clear,
                                                                        .kern: Ticks.width(count, size: tickSize) - xWidth]))
            }
            return CTLineCreateWithAttributedString(out)
        }
```

- in `bubble()`, line 688, `let foot = footer()` becomes `let foot = footer(on: cardGround)`, and line 742, `parts.append(contentsOf: footParts(foot, at: CGPoint(x: x + bw - p - footWidth, y: footY)))`, becomes `parts.append(contentsOf: footParts(foot, at: CGPoint(x: x + bw - p - footWidth, y: footY), on: cardGround))`;
- in `lines()`, line 789, `let foot = footer(time: context.showTime || self.m.edited || self.m.mine)` becomes `let foot = footer(time: context.showTime || self.m.edited || self.m.mine, on: cardGround)`, and line 830, `parts.append(contentsOf: footParts(foot, at: CGPoint(x: width - m - footWidth, y: footY)))`, becomes `parts.append(contentsOf: footParts(foot, at: CGPoint(x: width - m - footWidth, y: footY), on: cardGround))`. `bigEmoji()` (line 774) and `alone()` draw their foot under the picture, off the band, on the window's ground, so they keep calling `footer()` and `footParts(_:at:)` as they do.

In `mac/JawUI/Sources/Layout/Cards.swift`, a poll's open circles (line 625, `symbol(several ? "square" : "circle", 14, roles.tertiary, centredIn: tick)`) and its voted line (line 640, `text(Self.pollVoted(poll), fonts.small, roles.tertiary, …`) take `roles.faint(on: cardGround)` where they take `roles.tertiary`; and a link card's host, drawn on the card's well, line 694, `text(link.host, fonts.small, roles.tertiary, x: x, top: top, most: most)`, becomes `text(link.host, fonts.small, roles.secondary, x: x, top: top, most: most)`, as the dim text isn't held on a well.

In `mac/JawUI/Sources/Library/LibraryDrawing.swift`'s `tile`, the placeholder of a tile with no picture yet, drawn on the tile's well, line 164, `symbol(item.kind == .video ? "play.rectangle" : "photo", 20, r.tertiary, centredIn: rect, into: &d.under)`, becomes `symbol(item.kind == .video ? "play.rectangle" : "photo", 20, r.secondary, centredIn: rect, into: &d.under)`, for the same reason.

In `mac/JawUI/Sources/Conversation/ConversationController.swift`, the controller tells the layout which message is selected, in lines:

- in `rebuild()` (lines 348-370), after main's block that fills `cs[i].sending` and before `contexts = cs` (line 369), add:

```swift
        if spec.style.look == .lines, let id = selected, let i = rows.firstIndex(where: { $0.id == id }) {
            cs[i].selected = true
        }
```

- `select(_:)` (lines 771-785) becomes:

```swift
    public func select(_ id: String?) {
        let previous = selected
        selected = id
        for changed in [previous, id].compactMap({ $0 }) {
            guard let i = rows.firstIndex(where: { $0.id == changed }) else { continue }
            let lines = spec.style.look == .lines
            if lines { contexts[i].selected = selected == changed }
            if let cell = table.view(atColumn: 0, row: i, makeIfNecessary: false) as? MessageView {
                cell.selected = selected == changed
                if lines { cell.laid = layouts.laid(rows[i], width: layoutWidth, spec: spec, context: contexts[i]) }
            }
        }
        if let id, let i = rows.firstIndex(where: { $0.id == id }) {
            table.scrollRowToVisible(i)
        }
        composer.selecting = id != nil
    }
```

  A selected message's layout differs from its unselected one only in its colours, so its height stays and the table needn't measure it again; `relayout()`, which a change of look runs, runs `rebuild()`; and main's `took(_:)`, which lays a file of yours on its way out again as it goes, lays it out from `contexts[i]`, so a selected message keeps its band's colours as its ring fills.

In `mac/JawUI/Sources/Palette/Palette.swift`'s `row(_:selected:)`, an action's chevron, line 572, takes `.foregroundStyle(roles.faint(on: selected ? .selection : .surface).color)` where it takes `.foregroundStyle(roles.tertiary.color)`.

In `mac/JawUI/Sources/Library/LibraryDrawing.swift`, a row's date takes the selection's tone, so `file`, `link` and `twoLines` are told whether the row is selected:

- in `row(_:size:selected:_:)`, lines 242-243 become:

```swift
        case .file: file(item, icon: icon, x: x, right: right, size: size, selected: selected, c, into: &d)
        case .link: link(item, icon: icon, x: x, right: right, size: size, selected: selected, c, into: &d)
```

- `file`'s signature (line 277) becomes `static func file(_ item: LibraryRow, icon: CGRect, x: CGFloat, right: CGFloat, size: CGSize, selected: Bool, _ c: Context, into d: inout LibraryDrawn) {`, and its last line (284) `twoLines(f?.name.isEmpty == false ? f!.name : "a file", details, x: x, right: right, size: size, selected: selected, item, c, into: &d)`;
- `link`'s signature (line 311) becomes `static func link(_ item: LibraryRow, icon: CGRect, x: CGFloat, right: CGFloat, size: CGSize, selected: Bool, _ c: Context, into d: inout LibraryDrawn) {`, and its last line (330) `twoLines(title, details, x: x, right: right, size: size, selected: selected, item, c, into: &d)`;
- `twoLines` (lines 249-255) becomes, to its `when`:

```swift
    /// Two lines: a title, with when at its right, over details.
    static func twoLines(_ title: String, _ details: CTLine, x: CGFloat, right: CGFloat, size: CGSize, selected: Bool, _ item: LibraryRow,
                         _ c: Context, into d: inout LibraryDrawn)
    {
        let r = c.roles
        let name = font(.body), small = font(.caption)
        let when = line(Self.when(item, c), small, r.faint(on: selected ? .selection : c.style.look == .bubbles ? .surface : .ground))
```

  the rest as it is.

In `mac/JawUI/Sources/Conversation/ContactCardsView.swift`'s `CardRow`, line 186, `Text("No number").textStyle(.callout).tone(.tertiary, roles)` becomes `Text("No number").textStyle(.callout).foregroundStyle(roles.faint(on: selected ? .selection : .surface).color)`; and in `mac/JawUI/Sources/Conversation/ContactForm.swift`'s `FormCardRow`, line 339, `.textStyle(.callout).tone(numbers.isEmpty ? .tertiary : .secondary, roles).lineLimit(1)` becomes `.textStyle(.callout).foregroundStyle((numbers.isEmpty ? roles.faint(on: chosen ? .selection : .surface) : roles.secondary).color).lineLimit(1)`.

- [ ] **Step 11: Draw three small marks on one ground**

Three marks sat on a tint laid on a tint, and holding them there moved a whole theme's secondary text, accent drawn as text and links for a few small marks (the plan's): they're drawn on one ground, which the hold covers, and the hold's table has no tint on a tint.

In `mac/JawUI/Sources/Layout/Cards.swift`'s `linkPreview(_:)`, the thumbnail's placeholder (lines 675-679):

```swift
            // Its well until the image queue has it, and where it can't be read: jawd keeps it as the
            // JPEG it came as, so nothing else is decoded.
            let thumb = CGRect(x: r.minX + 6, y: (r.midY - Self.thumbnail / 2).rounded(), width: Self.thumbnail, height: Self.thumbnail)
            parts.append(.fill(thumb, well.cgColor, Radius.small))
            symbol("link", 18, roles.secondary, centredIn: thumb)
```

becomes:

```swift
            // Its link symbol, on the card's own well, until the image queue has it, and where it can't
            // be read: jawd keeps it as the JPEG it came as, so nothing else is decoded.
            let thumb = CGRect(x: r.minX + 6, y: (r.midY - Self.thumbnail / 2).rounded(), width: Self.thumbnail, height: Self.thumbnail)
            symbol("link", 18, roles.secondary, centredIn: thumb)
```

In `mac/Tests/JawUITests/LinkPreviewTests.swift`'s `` `a preview with neither title nor description has no card, and one whose thumbnail can't be read keeps its place` ``, lines 86-87, which looked for the thumbnail's own well, become:

```swift
    #expect(read.parts.contains { if case let .image(_, rect) = $0 { slot.rect.contains(CGPoint(x: rect.midX, y: rect.midY)) } else { false } },
            "its link symbol on the card's well where it goes, which shows until it's decoded, and where it can't be")
```

HeldInkTests' `a link card's thumbnail waits on the card's own well…` checks there's no second well.

In `mac/JawUI/Sources/Library/LibraryDrawing.swift`, where `file` and `link` know whether their row is selected since Step 10:

- `row(_:size:selected:_:)`'s doc comment (lines 225-226) becomes:

```swift
    /// A voice message's, a file's or a link's row: on a card in bubbles, between hairlines in lines,
    /// with the kind's mark at its left, a file's and a link's on their tint, or on the selection itself
    /// while the row's selected, its words, and who shared it, where and when.
```

- in `file`, the tint behind its symbol (line 280, `d.under.append(.fill(icon, r.accentTint.cgColor, Radius.small + 2))` as Step 8 left it) becomes `if !selected { d.under.append(.fill(icon, r.accentTint.cgColor, Radius.small + 2)) }`;
- in `link`, the well behind its icon (line 314, `d.under.append(.fill(icon, well(c).cgColor, Radius.small + 2))`) becomes `if !selected { d.under.append(.fill(icon, well(c).cgColor, Radius.small + 2)) }`. A link's preview, where it has one, is still drawn in the icon's place over it.

- [ ] **Step 12: Read the system's orange, green and red with its accent**

In `mac/JawUI/Sources/Style/Preferences.swift`:

- `accent`'s doc comment (lines 76-77) becomes:

```swift
    /// The system's colours in each appearance, its accent, orange, green and red, which Neutral draws
    /// in, read again by `systemColoursChanged()`, and not kept, as they're the system's.
```

- `systemColoursChanged()`'s doc comment (line 186) becomes `/// Reads the system's accent, orange, green and red again, as the app does when AppKit says the system's colours changed.`
- `SystemAccent`'s extension, lines 211-221, becomes:

```swift
public extension SystemAccent {
    /// The system's accent, orange, green and red as AppKit draws them now, in a dark appearance and in
    /// a light one; where AppKit has no appearance of the name, the fixed colours of the one asked for.
    @MainActor static func current() -> SystemAccent {
        func drawn(_ colour: NSColor, otherwise fixed: SystemColour) -> SystemColour {
            func half(_ name: NSAppearance.Name, _ otherwise: RGBA) -> RGBA {
                var out = otherwise
                NSAppearance(named: name)?.performAsCurrentDrawingAppearance { out = RGBA(colour) }
                return out
            }
            return SystemColour(dark: half(.darkAqua, fixed.dark), light: half(.aqua, fixed.light))
        }
        let accent = drawn(.controlAccentColor, otherwise: SystemColour(dark: blue.dark, light: blue.light))
        return SystemAccent(dark: accent.dark, light: accent.light, orange: drawn(.systemOrange, otherwise: .orange),
                            green: drawn(.systemGreen, otherwise: .green), red: drawn(.systemRed, otherwise: .red))
    }
}
```

In `mac/JawUI/Sources/Settings/Panes.swift`, `RGBA.init(_ colour: NSColor)`'s doc comment, line 147, `/// From a colour picked, in sRGB.`, becomes `/// From a colour AppKit gives, a colour picked or one of the system's, in sRGB.`, as `current()` reads four of the system's through it.

The app already calls `systemColoursChanged()` on `NSColor.systemColorsDidChangeNotification` (`AppDelegate.swift`, lines 43-45), and `accent`'s `didSet` redraws when any of the four changes, so nothing else is needed.

- [ ] **Step 13: The harness's Roles scene shows the accent drawn as text**

In `mac/JawHarness/Sources/Scenes/FoundationScenes.swift`'s `RolesScene`, `named` (line 33) gains `("accent as text", roles.accentText),` after `("accent", roles.accent),`; in `textColours` (line 40), `("accent", roles.accent),` becomes `("accent as text", roles.accentText),`; and the second band's note begins `"Secondary and tertiary text, the accent drawn as text, the links and read ticks, mentions, online and failed,` where it began `"Secondary and tertiary text, the accent's counts, the links and read ticks, mentions, online and failed,`.

- [ ] **Step 14: Run the UI tests, and take the reference pictures that changed**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 1. Every failure is a reference picture's, `<name>: <n> pixels differ from the reference`, and every other test passes, `HeldInkTests.swift`'s seven among them. Which pictures fail depends on what each shows. Tokyo Night's own colours move under the comparison's 24 a channel (its dim text 17, `565F89` to `66709A`; its secondary text 13, `737AA2` to `7E87AF`), as do Tokyo Night Day's dim text (15, `848CB5` to `757FAC`) and mention (23, `B15C00` to `9A5814`). What moves past it: Tokyo Night Day's accent drawn as text and links (37, `2E7DE9` to `2D68C4`), its red (`F52A65` to `C0316D`) and six of its senders; in either, the dim text drawn in the secondary text's colour off the window's ground and the surface (your bubbles' grey ticks and clock, a selected row's marks, a link card's host, a library tile's placeholder); and the three small marks (a link card's waiting thumbnail, which loses its second well, and a selected library row's file or link mark, which loses its tint). A picture failing for anything else, or with `its size changed`, is a fault: stop and say so.

List them and take each again:

```bash
mkdir -p mac/build/settings
ssh -o BatchMode=yes build-mac 'grep -oE "[a-z0-9-]+: [0-9]+ pixels differ from the reference" ~/src/build-mac/jaw/jaw-mac-settings/mac/build/check.log' \
    | cut -d: -f1 | sort -u > mac/build/settings/changed.txt
cat mac/build/settings/changed.txt
while read -r name; do rm "mac/Tests/JawUITests/References/$name.png"; done < mac/build/settings/changed.txt
```

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 1, the same pictures each recording `<name>: no reference yet; wrote <name>.png to look at and commit`, nothing else failing.

Run: `mac/scripts/build-mac references`, then look at each picture `changed.txt` names, in `mac/Tests/JawUITests/References/`, with the Read tool.
Expected: each the picture it was, its layout and every word where they were, but for its colours and three small marks: in a light one Tokyo Night Day's links, read ticks, unread times, Draft:, mentions of others, Show more and You in a reply a deeper blue than its unread badges, and its failed messages and first sender a deeper rose; in either, your bubbles' grey ticks and clock, a selected row's pin, mute mark and ticks, a link card's host and a library tile's placeholder in the time's grey; a link card waiting for its thumbnail showing its link symbol on the card's well, with no square of its own; a selected file or link in the library showing its mark on the selection, with no tint behind it; and, in a dark one taken again for any of those, Tokyo Night's dim text and times a shade lighter (`66709A`, `7E87AF`), the dim text still the quieter. If one shows anything else changed, stop and say so.

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 0, `** TEST SUCCEEDED **`, `✔ Test run with 425 tests in 1 suite passed`, seven more than the 418 the branch has before this task.

- [ ] **Step 15: Build, test the rest and look**

Run: `mac/scripts/build-mac test JawHarnessTests`, `mac/scripts/build-mac build` and `make check-parity`.
Expected: each exits 0, JawHarnessTests with its 29 tests; the build runs `check-components` and `check-harness`, which pass, as nothing here sets a font, a corner or a line width by hand.

Run: `mac/scripts/build-mac snapshot mac/build/settings/day.png -appearance light -themeLight jaw`
Expected: `"snapshot":1.0` among the last lines; the main window in Tokyo Night Day, looked at with the Read tool: the chat list's unread times a deeper blue (`2D68C4`) than the unread badges beside them (`2E7DE9`), the badges' counts white on them, and nothing cut or moved.

Run: `mac/scripts/build-mac snapshot mac/build/settings/redlamp.png -appearance light -themeLight redlamp:redlamp`
Expected: the main window on Redlamp's paper (`F3EEE8`): the unread badges in its accent, the safelight drawn on paper (`D8352A`), and the times beside them a hair darker (`CF3429`); a mention of you, where the demo's chat shows one, in a brown (`936654`), not red, for the owner to look at; and a group's senders' names in yellows, greens, blues and purples, none red.

Run: `mac/scripts/build-mac snapshot mac/build/settings/neutral.png -appearance dark -themeDark redlamp:neutral`
Expected: the main window in Neutral's dark greys, drawn in the build Mac's own system colours, read by `SystemAccent.current()`: with macOS's default accent the unread badges in `007AFF` and the times beside them a hair lighter (`1A87FC`), and wherever the demo shows one, a mention of you or the "new" marker in macOS 27.0.1's dark orange (`FF9230`). If the build Mac's accent isn't the default, the badges take it, which is right; say which it was.

- [ ] **Step 16: Commit**

```bash
git add mac/JawThemes/Sources mac/Tests/JawThemesTests mac/JawUI/Sources mac/Tests/JawUITests \
    mac/JawHarness/Sources/Scenes/FoundationScenes.swift
git commit -F- --trailer "Co-authored-by: Cursor <cursoragent@cursor.com>" <<'EOF'
Every colour Jaw draws as text or as a mark reads at 3:1 wherever it's drawn, in every theme, Tokyo Night Day included, as the owner chose on 6 November: each theme's roles go through one hold after their own mapping, where Theme is made and where Catalog.jaw is, which moves each colour toward the text along theme.rs's straight line until it reads at 3:1 on every ground JawUI draws it on, the window's, the surface, a selected row, your bubbles and a selected message's band in lines, and the tints JawUI lays over them, a card's and a quote's well, the accent's tint behind its symbols, the box's chip and a mention of you on its own tint, which moves with it in passes that end, a selected row's online at 4.5:1, and keeps the accent's fills, the grounds, the text and the borders as the theme wrote them, the tints now made once in Roles and taken by every draw site, a file of yours on its way, which main's attachments draw, taking the accent's tint and the accent drawn as text as a download does; three small marks that sat on a tint on a tint, a link card's waiting thumbnail and a selected library row's file and link marks, are drawn on one ground, so a few marks no longer move a whole theme's secondary text, accent drawn as text and links; the accent drawn as text takes a role of its own, accentText, while the unread badge keeps the accent; the dim text is held on the window's ground and the surface, and everywhere else it's drawn, your bubbles, a selected row, a selected message's band and a card's or a tile's well, its words and marks take the secondary text's colour, as WhatsApp draws its grey ticks in its time's, so the dim text stays quieter than the secondary text, and the conversation tells a message's layout it's selected in lines; the Redlamp family keeps its red to its accent, its light half's mentions taking the brand's filament, darkened to read on paper, and its senders' eight hues spread 37.5° apart away from the red; Neutral takes the system's orange, green and red from AppKit with its accent, again when the system's colours change, and the tests and the harness draw it in macOS 27.0.1's, its default accent 007AFF in both appearances, as AppKit resolves it there; under a grey accent such as Graphite's Neutral's senders take the system's blue's hue, so they come out as eight colours; and the tests pin the hold's table, what it moves in each of the 69 halves and where each moved colour lands, within 0.05 of its target, as they now do for theme.rs's moves, the surface's hold and Redlamp's bands, the reference pictures that changed taken again
EOF
```

Then remove the scratch list, `rm mac/build/settings/changed.txt`; the pictures `day.png`, `redlamp.png` and `neutral.png` stay with the others for the reviewer, as the Global Constraints have them.

---

### Task 5: Omarchy's themes from jawd, remembered for the next launch

**Files:**
- Create: `mac/JawKit/Sources/Protocol/Themes.swift`, `mac/JawUI/Sources/Settings/OmarchyState.swift`
- Modify: `mac/JawKit/Sources/Client/Incoming.swift` (`Expect` at line 5, `Reply` at 9-20, `IncomingLine`'s replies at 91-99), `mac/JawKit/Sources/Client/Jawd.swift` (after `status()` at 244-246, `answered` at 431-472, `Asked` at 563-589, `refused`'s `case .status, .quiet:` at 743, `JawUpdate` at 776-829), `mac/JawUI/Sources/Style/Preferences.swift` (Task 4's `slots`, `accent`, `roles(_:)` and `themes`), `mac/JawUI/Sources/Settings/Panes.swift` (`AppearancePane`'s footer), `mac/JawApp/Sources/AppModel.swift` (`var reach` at line 23, `apply`'s `.hello` at 178-187 and its last case at 319)
- Test: `mac/Tests/JawKitTests/FixtureTests.swift`, `mac/Tests/JawKitTests/ClientTests.swift`, `mac/Tests/JawKitTests/LiveTests.swift`, `mac/Tests/JawUITests/PreferencesTests.swift`

**Interfaces:**
- Consumes: `OmarchyTheme`, `Catalog.themes(omarchy:accent:)`, `Catalog.roles(_:of:omarchy:accent:)`, `Preferences`' `slots`, `accent`, `made`, `listed`, `tell()` and `roles(_:)` (Task 4); `Roles.held()` (Task 4b); JawKit's `ask`, `NoParams`, `Failure`, `ErrorCode.unknownMethod`; `Situation.Reach`; and in the tests `FakeJawd.answer(_:id:)`, `connected`, `drain`, `requests`, `DemoJawd`, `Held`, `repo` and `scratchPreferences`.
- Produces, in JawKit: `public struct ServedTheme: Codable, Hashable` (`id`, `name`, `mode: ServedTheme.Mode` (`.dark`, `.light`), `colors: [String: String]`); `Expect.themes`; `Reply.themes([ServedTheme])`; `Jawd.themes()`; `Jawd.Asked.themes`; `JawUpdate.themes([ServedTheme])` and `JawUpdate.noThemes(older: Bool, why: String)`.
- Produces, in JawUI: `public enum OmarchyState: Hashable` (`.connecting`, `.asking`, `.starting`, `.unreachable`, `.older`, `.refused(String)`), with `init(_ reach: Situation.Reach)` and `line: String?`; `Preferences.served: [OmarchyTheme]?`, `Preferences.omarchyState: OmarchyState` and `Preferences.omarchyLine: String?`; the copies `themeDarkColours` and `themeLightColours`.
- Produces, in the app: `themes` asked after each hello, its answer given to `Preferences.served`, and the Omarchy section's state following `reach`.

- [ ] **Step 1: Write JawKit's failing tests**

In `mac/Tests/JawKitTests/FixtureTests.swift`, in `every result and push fixture decodes`, after `case "status": .reply(.status)` add `        case "themes": .reply(.themes)`; in `each request JawKit writes equals its fixture`, after the `status.request.json` line add:

```swift
    try same("themes.request.json", written(id: 21, "themes", NoParams?.none))
```

and at the end of the file add:

```swift
@Test func `themes' answer decodes as jaw-proto serves Omarchy's themes, each with its mode and its 25 colours`() throws {
    guard case let .reply(_, .themes(themes)) = try incoming("themes.result.json", .reply(.themes)) else {
        Issue.record("not themes")
        return
    }
    #expect(themes.map(\.id) == ["catppuccin", "catppuccin-latte"] && themes.map(\.name) == ["Catppuccin", "Catppuccin Latte"])
    #expect(themes.map(\.mode) == [.dark, .light] && themes.allSatisfy { $0.colors.count == 25 })
    #expect(themes.first?.colors["background"] == "#1e1e2e" && themes.first?.colors["brown"] == "#7b5b55")
}
```

In `mac/Tests/JawKitTests/ClientTests.swift`, inside `ClientTests`, after `the terminal's panes are asked for by its name and come whole, and a jawd without them says so`, add:

```swift
    @Test func `Omarchy's themes come as jawd serves them, and a jawd from before them is told apart from a drop`() async throws {
        let fake = try FakeJawd { method, params, id in
            method == "themes" ? [FakeJawd.answer("themes.result.json", id: id)] : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.themes()
        let all = await drain(jawd) { all in all.contains { if case .themes = $0 { true } else { false } } }
        let themes = all.compactMap { if case let .themes(t) = $0 { t } else { nil } }.first ?? []
        #expect(themes.map(\.id) == ["catppuccin", "catppuccin-latte"] && themes.map(\.mode) == [.dark, .light])
        #expect(requests(fake, "themes") == [.null], "asked with no params")
        let older = jawd.refused(.themes, .jawd(JawError(code: ErrorCode.unknownMethod, message: "unknown method: themes")))
        let busy = jawd.refused(.themes, .jawd(JawError(code: ErrorCode.limit, message: "jawd is busy")))
        guard case let .noThemes(isOlder, _)? = older.first, case let .noThemes(busyIsOlder, why)? = busy.first else {
            Issue.record("nothing said: \(older), \(busy)")
            return
        }
        #expect(isOlder && !busyIsOlder && why == "jawd is busy")
        #expect(jawd.refused(.themes, .lost).isEmpty && jawd.refused(.themes, .unreachable).isEmpty,
                "a drop says nothing, as the next hello asks again")
    }
```

In `mac/Tests/JawKitTests/LiveTests.swift`, inside `LiveTests`, after `the terminal's panes come from a real jawd as it keeps them, and a chat is looked up by its JID`, add:

```swift
    @Test func `a real jawd serves Omarchy's 23 themes as protocol/themes has them, in the order of their IDs, each with its mode and its 25 colours`() async throws {
        let demo = try DemoJawd()
        defer { demo.stop() }
        let held = Held(demo.socket)
        defer { held.jawd.close() }
        #expect(await held.until { h in h.updates.contains { if case .hello = $0 { true } else { false } } })
        held.jawd.themes()
        #expect(await held.until { h in h.updates.contains { if case .themes = $0 { true } else { false } } })
        let themes = held.updates.compactMap { if case let .themes(t) = $0 { t } else { nil } }.first ?? []
        let folder = repo.appending(path: "protocol/themes")
        let files = try FileManager.default.contentsOfDirectory(atPath: folder.path).filter { $0.hasSuffix(".toml") }
            .map { String($0.dropLast(5)) }.sorted()
        #expect(themes.count == 23 && themes.map(\.id) == files, "every file protocol/themes holds, in the order of their IDs")
        #expect(themes.filter { $0.mode == .light }.map(\.id) == ["catppuccin-latte", "flexoki-light", "lupine", "rose-pine", "white"])
        #expect(themes.allSatisfy { $0.colors.count == 25 } && themes.first { $0.id == "rose-pine" }?.name == "Rosé Pine")
        for theme in themes {
            // Read as `key = "value"` lines, which is all a colors.toml is.
            var written: [String: String] = [:]
            for line in try String(contentsOf: folder.appending(path: "\(theme.id).toml"), encoding: .utf8).split(separator: "\n") {
                let parts = line.split(separator: "=", maxSplits: 1).map { $0.trimmingCharacters(in: .whitespaces) }
                if parts.count == 2 { written[parts[0]] = parts[1].trimmingCharacters(in: CharacterSet(charactersIn: "\"")).lowercased() }
            }
            #expect(theme.mode.rawValue == written["mode"], "\(theme.id)'s mode, as its file has it")
            #expect(theme.colors["background"] == written["background"] && theme.colors["accent"] == written["accent"],
                    "\(theme.id)'s ground and accent, as its file has them")
        }
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawKit`
Expected: exit 1, with `error: type 'Expect' has no member 'themes'` from `FixtureTests.swift` and `error: value of type 'Jawd' has no member 'themes'` from `ClientTests.swift`.

- [ ] **Step 3: Ask for the themes**

Create `mac/JawKit/Sources/Protocol/Themes.swift`:

```swift
import Foundation

/// One of Omarchy's themes as `themes` serves it, as jaw-proto's `Theme` has it: its ID, the name it's
/// shown as, its mode, and its 25 colours by Omarchy's keys, `#rrggbb`.
public struct ServedTheme: Codable, Sendable, Hashable {
    public enum Mode: String, Codable, Sendable, Hashable {
        case dark, light
    }

    public var id: String
    public var name: String
    public var mode: Mode
    public var colors: [String: String]

    public init(id: String, name: String, mode: Mode, colors: [String: String]) {
        self.id = id
        self.name = name
        self.mode = mode
        self.colors = colors
    }
}

/// `themes`' result.
struct ServedThemes: Decodable {
    let themes: [ServedTheme]
}
```

In `mac/JawKit/Sources/Client/Incoming.swift`:
- `Expect`'s cases become `case json, sent, chat, info, chatInfo, status, layout, saved, folder, themes`.
- `Reply` gains, after `case folder(FolderRow?)`:

```swift
    /// Omarchy's themes, in the order jawd serves them.
    case themes([ServedTheme])
```

- In `IncomingLine.init(from:)`, after `case .reply(.folder): …`, add:

```swift
                case .reply(.themes): value = try .reply(id, .themes(c.decode(ServedThemes.self, forKey: .result).themes))
```

In `mac/JawKit/Sources/Client/Jawd.swift`:
- After `status()`, add:

```swift
    /// Asks for Omarchy's themes, which come as `.themes`; a jawd that doesn't serve them is said as
    /// `.noThemes`, and a drop says nothing, as the app asks again at the next hello.
    public func themes() {
        ask("themes", NoParams?.none, .themes, expect: .themes)
    }
```

- In `answered`, after `case let (.status, .status(status)): out.append(.status(status))`, add:

```swift
        case let (.themes, .themes(themes)):
            out.append(.themes(themes))
```

- In `Asked`, after `case status`, add `        case themes`.
- In `refused`, before `case .status, .quiet:`, add:

```swift
        case .themes:
            if failure == .lost || failure == .unreachable { return [] }
            return [.noThemes(older: failure.code == ErrorCode.unknownMethod, why: failure.description)]
```

- In `JawUpdate`, after `case status(Status)`, add:

```swift
    /// Omarchy's themes, as `themes` answered.
    case themes([ServedTheme])
    /// jawd didn't serve its themes: an older one, which doesn't know `themes`, or another refusal, in
    /// jawd's words.
    case noThemes(older: Bool, why: String)
```

- [ ] **Step 4: Run JawKit's tests**

Run: `mac/scripts/build-mac test JawKit`
Expected: exit 0, `** TEST SUCCEEDED **`, three more tests than before, the live one among them run against `target/debug/jawd` (jawd 660701c5).

- [ ] **Step 5: Write the preferences' failing tests**

In `mac/Tests/JawUITests/PreferencesTests.swift`, add:

```swift
/// One of Omarchy's themes as jawd serves it, read from `protocol/themes/`.
func omarchy(_ id: String) throws -> OmarchyTheme {
    let file = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent().appending(path: "protocol/themes/\(id).toml")
    return try #require(OmarchyTheme(id: id, file: String(contentsOf: file, encoding: .utf8)))
}

@MainActor @Test func `a slot's Omarchy theme is remembered as jawd served it, drawn before jawd answers, and dropped once jawd serves themes without it`() throws {
    let suite = "jaw.tests.slots-remembered"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defaults.removePersistentDomain(forName: suite)
    let gruvbox = try omarchy("gruvbox"), latte = try omarchy("catppuccin-latte")
    let first = Preferences(defaults: defaults, accent: .blue)
    var toldFirst = 0
    first.changed.append { toldFirst += 1 }
    first.served = [gruvbox, latte]
    #expect(toldFirst == 0, "jawd's first answer leaves Jaw's in both slots, so nothing is drawn again")
    let chosen = try #require(first.themes.first { $0.id == "omarchy:gruvbox" })
    first.slots = first.slots.choosing(chosen, for: .dark)
    let dark = try #require(Omarchy.roles(gruvbox.colours, dark: true)).held()
    #expect(first.roles(.dark) == dark && first.roles(.light) == Catalog.jaw.light)
    let copy = defaults.dictionary(forKey: "themeDarkColours") as? [String: String]
    #expect(copy?["id"] == "gruvbox" && copy?["name"] == "Gruvbox" && copy?["background"] == "#282828" && copy?.count == 27)
    #expect(defaults.dictionary(forKey: "themeLightColours") == nil, "the light slot holds Jaw's")

    let next = Preferences(defaults: defaults, accent: .blue)
    var told = 0
    next.changed.append { told += 1 }
    #expect(next.served == nil && next.roles(.dark) == dark, "the next launch draws Gruvbox before jawd answers")
    #expect(next.themes.filter { $0.source == .omarchy }.map { $0.id } == ["omarchy:gruvbox"], "the slots' themes, from their copies")
    next.served = [gruvbox, latte]
    #expect(told == 0, "jawd serving the colours the copy has draws nothing again, on the launch's path")
    var newer = gruvbox
    newer.colours["accent"] = "#ff0000"
    next.served = [newer, latte]
    #expect(told == 1 && next.roles(.dark) == Omarchy.roles(newer.colours, dark: true)?.held(), "jawd's colours replace the copy, drawn once")
    #expect((defaults.dictionary(forKey: "themeDarkColours") as? [String: String])?["accent"] == "#ff0000")
    next.served = [latte]
    #expect(told == 2 && next.roles(.dark) == Catalog.jaw.dark && next.slots.dark == "omarchy:gruvbox",
            "drawn as Tokyo Night, and the choice kept")
    #expect(defaults.dictionary(forKey: "themeDarkColours") == nil)
    defaults.set(["id": "nord", "name": "Nord"].merging(gruvbox.colours) { $1 }, forKey: "themeDarkColours")
    #expect(Preferences(defaults: defaults, accent: .blue).roles(.dark) == Catalog.jaw.dark, "a copy of another theme isn't drawn")
}

@MainActor @Test func `until jawd serves its themes, the Omarchy section says why, and nothing while the app connects or asks`() throws {
    #expect(OmarchyState(.waiting).line == nil && OmarchyState(.up).line == nil)
    #expect(OmarchyState(.starting(failed: nil)).line == "Starting jawd")
    #expect(OmarchyState(.unreachable(why: "no socket", retry: 2)).line
        == "Omarchy's themes come from jawd, which isn't answering; they show once it does")
    #expect(OmarchyState(.starting(failed: "jawd isn't in the app")) == .unreachable, "a jawd that couldn't start isn't answering")
    #expect(OmarchyState.older.line == "This jawd is older and serves no themes; restart it")
    #expect(OmarchyState.refused("jawd is busy").line == "Omarchy's themes didn't come: jawd is busy")
    let preferences = try scratchPreferences("omarchy-state")
    #expect(preferences.omarchyState == .connecting && preferences.omarchyLine == nil)
    preferences.omarchyState = .older
    #expect(preferences.omarchyLine == OmarchyState.older.line)
    preferences.served = [try omarchy("gruvbox")]
    #expect(preferences.omarchyLine == nil, "once jawd has served its themes there's nothing to say, even when it goes")
}
```

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 1, with `error: value of type 'Preferences' has no member 'served'`.

- [ ] **Step 6: Keep jawd's themes and the copies**

Create `mac/JawUI/Sources/Settings/OmarchyState.swift`:

```swift
import Foundation

/// What the gallery's Omarchy section says while jawd hasn't served its themes: nothing while the app
/// connects or asks, as jawd answers at once; "Starting jawd" while the jawd the app started starts;
/// that jawd isn't answering while it can't be reached or couldn't start; that it's older and serves
/// no themes when it doesn't know `themes`; and any other refusal in jawd's words.
public enum OmarchyState: Sendable, Hashable {
    case connecting, asking, starting, unreachable, older
    case refused(String)

    /// From how jawd is reached, before its answer to `themes`.
    public init(_ reach: Situation.Reach) {
        switch reach {
        case .waiting: self = .connecting
        case .starting(failed: nil): self = .starting
        case .starting: self = .unreachable
        case .up: self = .asking
        case .unreachable: self = .unreachable
        }
    }

    /// The line the section shows in place of the themes it hasn't, or nil.
    public var line: String? {
        switch self {
        case .connecting, .asking: nil
        case .starting: "Starting jawd"
        case .unreachable: "Omarchy's themes come from jawd, which isn't answering; they show once it does"
        case .older: "This jawd is older and serves no themes; restart it"
        case let .refused(why): "Omarchy's themes didn't come: \(why)"
        }
    }
}
```

In `mac/JawUI/Sources/Style/Preferences.swift`:

- `slots`' `didSet` becomes:

```swift
        didSet {
            made = [:]
            listed = nil
            remember()
            save()
        }
```

- After `accent`, add:

```swift
    /// Omarchy's themes as jawd served them this run, nil until it has. They stay when jawd goes away,
    /// as its answer still holds. The app is told only when what a slot draws changes, so jawd's first
    /// answer, which comes as a launch's first rows do, restyles no window while the slots hold Jaw's or
    /// Redlamp's themes or a copy whose colours jawd serves as they were; Settings, which observes
    /// this, redraws either way.
    public var served: [OmarchyTheme]? = nil {
        didSet {
            guard served != oldValue else { return }
            let drawn = Slot.allCases.map { made[$0] ?? Catalog.roles($0, of: slots, omarchy: oldValue ?? remembered(slots), accent: accent) }
            made = [:]
            listed = nil
            remember()
            if Slot.allCases.map({ roles($0) }) != drawn { tell() }
        }
    }

    /// What the gallery's Omarchy section says while jawd hasn't served its themes.
    public var omarchyState: OmarchyState = .connecting
```

- `roles(_:)` and `themes` take Omarchy's themes from jawd, or the copies until it answers:

```swift
    /// What `slot` draws: its theme's half, or Jaw's Tokyo Night's where no source has the theme. What
    /// it rests on is read before what's made, so a view drawn from it redraws as that changes.
    public func roles(_ slot: Slot) -> Roles {
        let slots = self.slots, accent = self.accent, served = self.served
        if let roles = made[slot] { return roles }
        let roles = Catalog.roles(slot, of: slots, omarchy: served ?? remembered(slots), accent: accent)
        made[slot] = roles
        return roles
    }

    /// Every theme there is to choose, in the gallery's order: Omarchy's as jawd serves them, or until
    /// it has, the slots' own from their copies.
    public var themes: [Theme] {
        let slots = self.slots, accent = self.accent, served = self.served
        if let listed { return listed }
        let themes = Catalog.themes(omarchy: served ?? remembered(slots), accent: accent)
        listed = themes
        return themes
    }

    /// The line the Omarchy section shows in place of jawd's themes, or nil once they're in.
    public var omarchyLine: String? {
        served == nil ? omarchyState.line : nil
    }

    /// The slots' Omarchy themes as jawd last served them, from the copies kept beside the slots; a copy
    /// of a theme other than its slot's is passed over.
    func remembered(_ slots: Slots) -> [OmarchyTheme] {
        Slot.allCases.compactMap { slot in
            guard var colours = defaults.dictionary(forKey: Self.copy(slot)) as? [String: String],
                  let id = colours.removeValue(forKey: "id"), let name = colours.removeValue(forKey: "name"),
                  slots[slot] == "omarchy:" + id
            else { return nil }
            return OmarchyTheme(id: id, name: name, dark: slot.isDark, colours: colours)
        }
    }

    /// Where a slot's copy of its Omarchy theme is kept.
    static func copy(_ slot: Slot) -> String {
        slot.isDark ? "themeDarkColours" : "themeLightColours"
    }

    /// Keeps a copy of each slot's Omarchy theme as jawd serves it, with its ID and name, and drops one
    /// jawd no longer serves, so the next launch draws what this one does before jawd answers; a
    /// snapshot's preferences keep none.
    func remember() {
        guard keeps, let served else { return }
        for slot in Slot.allCases {
            if let theme = served.first(where: { "omarchy:" + $0.id == slots[slot] && $0.dark == slot.isDark }) {
                defaults.set(theme.colours.merging(["id": theme.id, "name": theme.name]) { $1 }, forKey: Self.copy(slot))
            } else {
                defaults.removeObject(forKey: Self.copy(slot))
            }
        }
    }
```

In `mac/JawUI/Sources/Settings/Panes.swift`, `AppearancePane`'s footer says what the Omarchy section will, until the gallery comes:

```swift
            } footer: {
                VStack(alignment: .leading) {
                    Text("Jaw draws the dark theme while the Mac is dark, and the light one while it's light.")
                    if let line = preferences.omarchyLine { Text(line) }
                }
            }
```

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 0, `** TEST SUCCEEDED **`, two more tests than Task 4b left.

- [ ] **Step 7: The app asks after each hello**

In `mac/JawApp/Sources/AppModel.swift`:

- `    var reach: Reach = .waiting` (line 23) becomes:

```swift
    var reach: Reach = .waiting {
        didSet { preferences.omarchyState = OmarchyState(reach) }
    }
```

- In `apply`, the `.hello` case's last line, `pictures.files = .folder(w.root)`, is followed by:

```swift
            jawd.themes()
```

- Before `case .push, .chats, .layout, .folders, .roster:`, add:

```swift
        case let .themes(served):
            preferences.served = served.map { OmarchyTheme(id: $0.id, name: $0.name, dark: $0.mode == .dark, colours: $0.colors) }
        case let .noThemes(older, why):
            preferences.omarchyState = older ? .older : .refused(why)
```

`preferences.served`'s `didSet` tells the app, which restyles every window, only when a slot's roles change: jawd's first answer at a launch, on the path the 250 ms budget is held on, restyles nothing while the slots hold Jaw's or Redlamp's themes or a copy jawd serves as it was, and a later hello's answer nothing while jawd's themes are as they were.

- [ ] **Step 8: Build, test and look**

Run: `mac/scripts/build-mac build`, `mac/scripts/build-mac test JawHarnessTests` and `make check-parity`.
Expected: each exits 0; check-parity counts `themes` among the methods the Mac sends, which the terminal, reading Omarchy's themes from jaw-proto, has no need to.

Run: `mac/scripts/build-mac snapshot mac/build/settings/omarchy.png --keys settings:appearance -themeDark omarchy:gruvbox -themeLight jaw`
Expected: Dark's menu shows "Gruvbox, Omarchy's", and opened it would list Jaw's Tokyo Night, Omarchy's 18 dark and Redlamp's 22; no line under the footer, as the demo's jawd has served its themes.

Run: `mac/scripts/build-mac snapshot mac/build/settings/gruvbox.png -appearance dark -themeDark omarchy:gruvbox`
Expected: the main window drawn in Omarchy's Gruvbox, its ground `#282828` and its text `#d4be98`. A snapshot's preferences write nothing back (Task 4's `keeps`), so it leaves no copy of Gruvbox in the build Mac's preferences for Jaw.

- [ ] **Step 9: Commit**

```bash
git add mac/JawKit/Sources/Protocol/Themes.swift mac/JawKit/Sources/Client/Incoming.swift mac/JawKit/Sources/Client/Jawd.swift \
    mac/JawUI/Sources/Settings/OmarchyState.swift mac/JawUI/Sources/Style/Preferences.swift mac/JawUI/Sources/Settings/Panes.swift \
    mac/JawApp/Sources/AppModel.swift mac/Tests/JawKitTests/FixtureTests.swift mac/Tests/JawKitTests/ClientTests.swift \
    mac/Tests/JawKitTests/LiveTests.swift mac/Tests/JawUITests/PreferencesTests.swift
git commit -F- --trailer "Co-authored-by: Cursor <cursoragent@cursor.com>" <<'EOF'
Omarchy's themes come to the Mac from jawd, as the Settings and themes design has them: JawKit asks themes, which it reads as ServedTheme by Omarchy's keys and hands up as JawUpdate.themes, and tells a jawd from before them, which answers that it doesn't know the method, from any other refusal, saying nothing of a drop since the app asks again after each hello; Preferences keeps what jawd served, which stays when jawd goes away, and a copy of each slot's Omarchy theme as themeDarkColours and themeLightColours, so the next launch draws it from its first frame, jawd's colours replacing the copy as they come and a theme jawd no longer serves drawn as Tokyo Night with the choice kept; OmarchyState follows how jawd is reached, saying nothing while the app connects or asks, and that jawd is starting, isn't answering or is older and serves no themes; and the slots' menus list Omarchy's beside Jaw's and Redlamp's
EOF
```

---

### Task 6: The slots and the gallery

**Files:**
- Create: `mac/JawUI/Sources/Settings/ThemeCard.swift`, `mac/JawUI/Sources/Settings/Gallery.swift`, `mac/JawUI/Sources/Settings/ArrowKeys.swift`
- Modify: `mac/JawUI/Sources/Settings/Panes.swift` (`AppearancePane`, as Tasks 4 and 5 left it), `mac/JawUI/Sources/Settings/SettingsPane.swift` (`size`), `mac/JawUI/Sources/Style/Preferences.swift` (after `omarchyState`), `mac/JawApp/Sources/SettingsWindowController.swift` (Task 1's), `mac/JawApp/Sources/Bench.swift` (`press`'s key event, lines 179-187)
- Test: `mac/Tests/JawUITests/GalleryTests.swift`, and the reference pictures `mac/Tests/JawUITests/References/theme-gallery-dark.png` and `theme-gallery-light.png`

**Interfaces:**
- Consumes: `Theme` with `name(_:)`, `Slot`, `Slots`, `Catalog.jaw`, `Catalog.jawTheme`, `Catalog.themes(omarchy:accent:)`, `Omarchy.name`, `Appearance` (Task 4); `Preferences.themes`, `roles(_:)`, `slots`, `appearance`, `served`, `omarchyState`, `omarchyLine` and `Preferences(defaults:accent:)` (Tasks 4 and 5); `OmarchyState` (Task 5); `SettingsPane` and `SettingsWindowController` (Task 1); `Roles.link` (Task 2); `Roles.accentText` and `Roles.mentionTint` (Task 4b); `TextStyle`, `Space`, `Radius`, `Stroke`, `RGBA.color`; the tests' `scratchPreferences`, `omarchy(_:)`, `picture(_:on:)` and `compare(_:_:)`.
- Produces, in JawUI:
  - `public struct ThemeChat: View` (`init(_ roles: Roles)`, `static let size`, 144 by 104).
  - `public struct ThemeCard: View` (`init(theme:slot:roles:ticked:hovered:label:)`), `public struct GalleryGrid: View` (`init(themes:slot:slots:choose:)`, `choose: (Theme, _ only: Bool) -> Void`), `public struct GallerySection: View` (`init(section:slot:slots:choose:)`), `public struct SettingsSection<Content: View>: View` (`init(_ title:footer:content:)`) and `public struct SlotTileView: View` (`init(tile:roles:outlined:)`).
  - `public struct Gallery: Hashable` (`init(_ themes: [Theme], for slot: Slot, omarchyLine: String?)`, `slot`, `sections: [Gallery.Section]` (`source`, `themes`, `line`, `header`, `footer`, `init(source:themes:line:)`), `cards`, `enum Arrow` (`.left`, `.right`, `.up`, `.down`), `moving(from:_:) -> Theme?`, `static func label(_ theme: Theme, for slot: Slot, _ slots: Slots) -> String`).
  - `public struct SlotTile: Hashable` (`init(_ slot: Slot, slots:themes:served:appearance:dark:)`, `slot`, `name`, `note`).
  - `ArrowKeys` and `ArrowCatcher`, internal: the gallery's arrows, through an AppKit view that holds the window's keys.
  - `Preferences.choosing: Slot?`.
- Produces, in the app: `--keys` presses reaching Settings while it's open.

- [ ] **Step 1: Write the failing tests**

Create `mac/Tests/JawUITests/GalleryTests.swift`:

```swift
import AppKit
import JawThemes
@testable import JawUI
import SwiftUI
import Testing

/// The Appearance pane's gallery and slots: a slot's half of every theme that has one, in Jaw's,
/// Omarchy's and Redlamp's sections; the Omarchy section while jawd's themes aren't in; where the
/// arrows go, and the arrows pressed in a window, choosing and scrolling with no click first; what
/// each slot's tile says; what VoiceOver reads a card as; and every card for each slot compared with a
/// reference picture, so a change to any mapping shows.

/// Omarchy's 23 as jawd serves them, read from `protocol/themes/`, in the order of their IDs.
func allOmarchy() throws -> [OmarchyTheme] {
    let folder = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent().appending(path: "protocol/themes")
    return try FileManager.default.contentsOfDirectory(atPath: folder.path).filter { $0.hasSuffix(".toml") }
        .map { String($0.dropLast(5)) }.sorted().map { try omarchy($0) }
}

@MainActor @Test func `the gallery shows a slot's half of every theme that has one, in three sections, 41 for dark and 28 for light`() throws {
    let preferences = try scratchPreferences("gallery-sections")
    preferences.served = try allOmarchy()
    let dark = Gallery(preferences.themes, for: .dark, omarchyLine: preferences.omarchyLine)
    #expect(dark.sections.map(\.source) == [.jaw, .omarchy, .redlamp] && dark.sections.map(\.themes.count) == [1, 18, 22])
    #expect(dark.cards.count == 41 && dark.sections.allSatisfy { $0.line == nil })
    #expect(dark.cards.allSatisfy { $0.dark != nil })
    #expect(dark.sections.map(\.header) == ["Jaw's", "Omarchy's", "Redlamp's"])
    #expect(dark.sections.map(\.footer) == ["Jaw's own Tokyo Night, with Tokyo Night Day as its light half.",
                                            "As the terminal draws them; jawd serves them.",
                                            "Redlamp's theme families, each a dark and a light half."], "as the wireframes have them")
    let light = Gallery(preferences.themes, for: .light, omarchyLine: nil)
    #expect(light.sections.map(\.themes.count) == [1, 5, 22] && light.cards.count == 28)
    #expect(light.sections[1].themes.map(\.name) == ["Catppuccin Latte", "Flexoki Light", "Lupine", "Rosé Pine", "White"])
    #expect(light.cards[0].name(.light) == "Tokyo Night Day", "Jaw's light half, by its own name")
    #expect(Array(dark.sections[2].themes.prefix(3).map(\.name)) == ["Neutral", "Redlamp", "Kanagawa"])
}

@MainActor @Test func `while jawd's themes aren't in, the Omarchy section shows the slots' own from their copies and says why`() throws {
    let suite = "jaw.tests.gallery-waiting"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defaults.removePersistentDomain(forName: suite)
    let first = Preferences(defaults: defaults, accent: .blue)
    first.served = try allOmarchy()
    first.slots = Slots(dark: "omarchy:gruvbox", light: "jaw")
    let next = Preferences(defaults: defaults, accent: .blue)
    next.omarchyState = .unreachable
    let gallery = Gallery(next.themes, for: .dark, omarchyLine: next.omarchyLine)
    #expect(gallery.sections[1].themes.map(\.id) == ["omarchy:gruvbox"])
    #expect(gallery.sections[1].line == "Omarchy's themes come from jawd, which isn't answering; they show once it does")
    #expect(gallery.sections[1].header == "Omarchy's, from jawd" && gallery.sections[1].footer == gallery.sections[1].line,
            "the line in its footer's place, as the wireframes have it")
    #expect(gallery.sections[0].themes.count == 1 && gallery.sections[2].themes.count == 22, "Jaw's and Redlamp's never wait")
}

@MainActor @Test func `the arrows go along the gallery's order, and up and down its columns into the sections around, nowhere past either end`() throws {
    let preferences = try scratchPreferences("gallery-arrows")
    preferences.served = try allOmarchy()
    let gallery = Gallery(preferences.themes, for: .dark, omarchyLine: nil)
    #expect(gallery.moving(from: "jaw", .right)?.id == "omarchy:catppuccin", "from Jaw's into Omarchy's")
    #expect(gallery.moving(from: "omarchy:catppuccin", .left)?.id == "jaw")
    #expect(gallery.moving(from: "omarchy:vantablack", .right)?.id == "redlamp:neutral", "from Omarchy's into Redlamp's")
    #expect(gallery.moving(from: "jaw", .down)?.id == "omarchy:catppuccin", "the card under Jaw's lone one")
    #expect(gallery.moving(from: "omarchy:ethereal", .down)?.id == "omarchy:hackerman", "down its column")
    #expect(gallery.moving(from: "omarchy:ethereal", .up)?.id == "jaw", "up into a shorter row, to its last card")
    #expect(gallery.moving(from: "omarchy:tokyo-night", .down)?.id == "redlamp:redlamp", "into the next section's first row, the same column")
    #expect(gallery.moving(from: "redlamp:redlamp", .up)?.id == "omarchy:tokyo-night", "and back into the last row of the one before")
    #expect(gallery.moving(from: "redlamp:mc-burnt-tape", .down)?.id == "redlamp:mc-velvet-static", "into a shorter last row")
    #expect(gallery.moving(from: "jaw", .left) == nil && gallery.moving(from: "jaw", .up) == nil)
    #expect(gallery.moving(from: "redlamp:mc-velvet-static", .right) == nil && gallery.moving(from: "redlamp:mc-velvet-static", .down) == nil)
    #expect(gallery.moving(from: "omarchy:catppuccin-latte", .right) == nil, "a light theme isn't in the dark gallery")
}

@MainActor @Test(.timeLimit(.minutes(1))) func `the arrows choose card after card with no click and no keyboard navigation, and the gallery scrolls to each`() async throws {
    let preferences = try scratchPreferences("gallery-keys")
    preferences.served = try allOmarchy()
    let host = NSHostingView(rootView: AppearancePane(preferences: preferences))
    let window = NSWindow(contentRect: NSRect(origin: CGPoint(x: -30000, y: -30000), size: SettingsPane.appearance.size),
                          styleMask: [.borderless], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.appearance = NSAppearance(named: .darkAqua)
    window.contentView = host
    window.orderFront(nil)
    defer { window.close() }
    try await Task.sleep(for: .milliseconds(300))
    #expect(window.firstResponder is ArrowCatcher, "the gallery holds the window's keys as the pane opens")
    func press(_ keyCode: UInt16, _ modifiers: NSEvent.ModifierFlags = []) async throws {
        let key = try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: modifiers.union([.numericPad, .function]),
                                                timestamp: 0, windowNumber: window.windowNumber, context: nil, characters: "",
                                                charactersIgnoringModifiers: "", isARepeat: false, keyCode: keyCode))
        window.sendEvent(key)
        try await Task.sleep(for: .milliseconds(100))
    }
    let (left, right, down, up): (UInt16, UInt16, UInt16, UInt16) = (123, 124, 125, 126)
    try await press(right)
    try await press(right)
    try await press(down)
    #expect(preferences.slots == Slots(dark: "omarchy:hackerman", light: "jaw"), "→ → ↓ from Jaw's: Catppuccin, Ethereal, then Hackerman under it")
    try await press(right, .command)
    #expect(preferences.slots.dark == "omarchy:hackerman", "⌘→ isn't the gallery's, and goes on up the responder chain")
    func scrolls(_ view: NSView) -> NSScrollView? {
        view as? NSScrollView ?? view.subviews.lazy.compactMap { scrolls($0) }.first
    }
    let scroll = try #require(scrolls(host), "the pane scrolls")
    let top = scroll.documentVisibleRect.minY
    for _ in 0 ..< 7 {
        try await press(down)
    }
    #expect(preferences.slots.dark == "redlamp:nord", "down Hackerman's column, then into Redlamp's")
    #expect(abs(scroll.documentVisibleRect.minY - top) > 400, "scrolled to the card chosen, far below the pane's first rows")
    for _ in 0 ..< 7 {
        try await press(up)
    }
    try await press(left)
    #expect(preferences.slots.dark == "omarchy:gruvbox", "back up the same way, then ← to the card before Hackerman")
}

@Test func `a slot's tile says its theme and its source, that it's in use or when it would be, and why it draws Tokyo Night`() throws {
    let themes = Catalog.themes(omarchy: [try omarchy("gruvbox")], accent: .blue)
    let slots = Slots(dark: "omarchy:gruvbox", light: "redlamp:nord")
    let dark = SlotTile(.dark, slots: slots, themes: themes, served: true, appearance: .system, dark: true)
    #expect(dark.name == "Gruvbox, Omarchy's" && dark.note == "In use")
    let light = SlotTile(.light, slots: slots, themes: themes, served: true, appearance: .system, dark: true)
    #expect(light.name == "Nord, Redlamp's" && light.note == nil, "left to the system, the other slot says nothing")
    #expect(SlotTile(.light, slots: slots, themes: themes, served: true, appearance: .dark, dark: true).note == "Used when Appearance is Light")
    let gone = Slots(dark: "omarchy:nord", light: "omarchy:gruvbox")
    #expect(SlotTile(.dark, slots: gone, themes: themes, served: true, appearance: .system, dark: true).name == "Nord isn't among jawd's themes")
    #expect(SlotTile(.dark, slots: gone, themes: themes, served: false, appearance: .system, dark: true).name == "Nord, once jawd serves it")
    #expect(SlotTile(.light, slots: gone, themes: themes, served: true, appearance: .system, dark: true).name
        == "Tokyo Night Day, as Gruvbox has no light half", "the half of Jaw's it draws in its place, by its name")
    #expect(SlotTile(.dark, slots: Slots(dark: "redlamp:nowhere"), themes: themes, served: true, appearance: .system, dark: true).name
        == "Tokyo Night, as no source has redlamp:nowhere")
    #expect(SlotTile(.light, slots: Slots(), themes: themes, served: true, appearance: .system, dark: true).name == "Tokyo Night Day, Jaw's")
}

@Test func `VoiceOver reads a card as the name of the half it shows, its section and the slots it fills`() throws {
    let themes = Catalog.themes(omarchy: [try omarchy("gruvbox")], accent: .blue)
    func card(_ id: String) throws -> Theme {
        try #require(themes.first { $0.id == id })
    }
    let slots = Slots(dark: "omarchy:gruvbox", light: "jaw")
    #expect(Gallery.label(try card("omarchy:gruvbox"), for: .dark, slots) == "Gruvbox, Omarchy's, the dark theme")
    #expect(Gallery.label(try card("jaw"), for: .light, slots) == "Tokyo Night Day, Jaw's, the light theme")
    #expect(Gallery.label(try card("redlamp:nord"), for: .dark, Slots(dark: "redlamp:nord", light: "redlamp:nord"))
        == "Nord, Redlamp's, the dark and light themes")
    #expect(Gallery.label(try card("redlamp:kanagawa"), for: .light, slots) == "Kanagawa, Redlamp's")
}

@MainActor @Test(.timeLimit(.minutes(2))) func `every theme's card for the dark slot and for the light one, three across at 144 points, as the gallery draws them`() async throws {
    let preferences = try scratchPreferences("gallery-pictures")
    preferences.served = try allOmarchy()
    for slot in Slot.allCases {
        let gallery = Gallery(preferences.themes, for: slot, omarchyLine: nil)
        let grid = GalleryGrid(themes: gallery.cards, slot: slot, slots: Slots(dark: "", light: "")) { _, _ in }
            .frame(width: ThemeChat.size.width * 3 + Space.m * 2)
            .padding(Space.m)
        let host = NSHostingView(rootView: grid)
        host.frame.size = host.fittingSize
        let w = NSWindow(contentRect: NSRect(origin: CGPoint(x: -30000, y: -30000), size: host.frame.size), styleMask: [.borderless],
                         backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        w.appearance = NSAppearance(named: slot.isDark ? .darkAqua : .aqua)
        w.contentView = host
        w.orderFront(nil)
        defer { w.close() }
        try await Task.sleep(for: .milliseconds(200))
        host.layoutSubtreeIfNeeded()
        try compare(try picture(host, on: slot.isDark ? RGBA(0x1E1E1E) : RGBA(0xF2F2F2)), "theme-gallery-\(slot.rawValue)")
    }
}
```

The pictures tick no card, as the system's accent would draw the tick, and Neutral is drawn in macOS 27.0.1's colours (`SystemAccent.blue`), which `scratchPreferences` gives, so they don't depend on the build Mac.

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 1, with `error: cannot find 'Gallery' in scope` from `GalleryTests.swift`.

- [ ] **Step 3: The gallery's sections, its arrows and the tiles' words**

Create `mac/JawUI/Sources/Settings/Gallery.swift`:

```swift
import JawThemes

/// The gallery for a slot: Jaw's, Omarchy's and Redlamp's sections, each the themes with a half for
/// the slot in the catalog's order, three across, with what's said in place of Omarchy's while jawd
/// hasn't served them; where the arrows go; and what VoiceOver reads a card as.
public struct Gallery: Sendable, Hashable {
    /// A section: its source, its themes, and the line said in place of those jawd hasn't served.
    public struct Section: Sendable, Hashable, Identifiable {
        public var source: ThemeSource
        public var themes: [Theme]
        public var line: String?

        public init(source: ThemeSource, themes: [Theme], line: String? = nil) {
            self.source = source
            self.themes = themes
            self.line = line
        }

        public var id: ThemeSource {
            source
        }

        /// Its heading: its source's name, and where they come from while jawd's themes aren't in.
        public var header: String {
            line == nil ? source.title : "\(source.title), from jawd"
        }

        /// What's said under it, as the wireframes have it: the line in jawd's state while jawd's
        /// themes aren't in, or what the section holds.
        public var footer: String {
            if let line { return line }
            switch source {
            case .jaw: return "Jaw's own Tokyo Night, with Tokyo Night Day as its light half."
            case .omarchy: return "As the terminal draws them; jawd serves them."
            case .redlamp: return "Redlamp's theme families, each a dark and a light half."
            }
        }
    }

    /// An arrow key.
    public enum Arrow: Sendable {
        case left, right, up, down
    }

    public var slot: Slot
    public var sections: [Section]

    public init(_ themes: [Theme], for slot: Slot, omarchyLine: String?) {
        self.slot = slot
        sections = ThemeSource.allCases.map { source in
            Section(source: source, themes: themes.filter { $0.source == source && $0.roles(slot) != nil },
                    line: source == .omarchy ? omarchyLine : nil)
        }
    }

    /// Every card, in the gallery's order, across its sections.
    public var cards: [Theme] {
        sections.flatMap(\.themes)
    }

    /// The card an arrow reaches from the theme `id`: ← and → the one before or after in the gallery's
    /// order, across its sections; ↑ and ↓ the one above or below in its column, three across, past a
    /// section's edge the next section's first row or the last row of the one before, the row's last
    /// card where that row is shorter. Nil past the gallery's ends, or from a theme it hasn't.
    public func moving(from id: String, _ arrow: Arrow) -> Theme? {
        let cards = cards
        guard let at = cards.firstIndex(where: { $0.id == id }) else { return nil }
        switch arrow {
        case .left:
            return at > 0 ? cards[at - 1] : nil
        case .right:
            return at + 1 < cards.count ? cards[at + 1] : nil
        case .up, .down:
            let shown = sections.map(\.themes).filter { !$0.isEmpty }
            guard let s = shown.firstIndex(where: { $0.contains { $0.id == id } }),
                  let i = shown[s].firstIndex(where: { $0.id == id })
            else { return nil }
            let (row, column) = (i / 3, i % 3)
            if arrow == .down {
                if (row + 1) * 3 < shown[s].count { return shown[s][min((row + 1) * 3 + column, shown[s].count - 1)] }
                return s + 1 < shown.count ? shown[s + 1][min(column, shown[s + 1].count - 1)] : nil
            }
            if row > 0 { return shown[s][(row - 1) * 3 + column] }
            guard s > 0 else { return nil }
            let above = shown[s - 1]
            return above[min((above.count - 1) / 3 * 3 + column, above.count - 1)]
        }
    }

    /// What VoiceOver reads a card as: the name of the half it shows for `slot`, its section and the
    /// slots it fills, as in "Gruvbox, Omarchy's, the dark theme".
    public static func label(_ theme: Theme, for slot: Slot, _ slots: Slots) -> String {
        let fills = Slot.allCases.filter { slots[$0] == theme.id }.map(\.rawValue)
        let filling = fills.isEmpty ? "" : ", the \(fills.joined(separator: " and ")) theme\(fills.count > 1 ? "s" : "")"
        return "\(theme.name(slot)), \(theme.source.title)\(filling)"
    }
}

/// What a slot's tile says: its theme's name and source, or why it draws Tokyo Night, and that Jaw
/// draws it now, or when it would.
public struct SlotTile: Sendable, Hashable {
    public var slot: Slot
    public var name: String
    /// "In use", "Used when Appearance is Light" while Appearance holds the other, or nil.
    public var note: String?

    /// `served` is whether jawd has served Omarchy's themes, and `dark` whether Jaw draws dark now.
    /// Where the slot draws Jaw's in its theme's place, it names the half of Jaw's it draws.
    public init(_ slot: Slot, slots: Slots, themes: [Theme], served: Bool, appearance: Appearance, dark: Bool) {
        self.slot = slot
        let id = slots[slot], instead = Catalog.jawTheme.name(slot)
        if let theme = themes.first(where: { $0.id == id }) {
            name = theme.roles(slot) == nil ? "\(instead), as \(theme.name) has no \(slot.rawValue) half" : "\(theme.name(slot)), \(theme.source.title)"
        } else if id.hasPrefix("omarchy:") {
            let named = Omarchy.name(String(id.dropFirst("omarchy:".count)))
            name = served ? "\(named) isn't among jawd's themes" : "\(named), once jawd serves it"
        } else {
            name = "\(instead), as no source has \(id)"
        }
        note = slot.isDark == dark ? "In use" : appearance == .system ? nil : "Used when Appearance is \(slot.title)"
    }
}
```

- [ ] **Step 4: The card, the gallery's grid and the slots' tiles**

Create `mac/JawUI/Sources/Settings/ThemeCard.swift`:

```swift
import AppKit
import JawThemes
import SwiftUI

/// A theme's half as a small chat, drawn from its roles as the chat list and a conversation draw them:
/// on the ground, an unread chat's row on the surface, a separator, their bubble on the surface with a
/// mention of you, and yours. 104 points tall and as wide as it's given, 144 in the design; a slot's
/// tile draws it at half size.
public struct ThemeChat: View {
    public static let size = CGSize(width: 144, height: 104)
    let roles: Roles

    public init(_ roles: Roles) {
        self.roles = roles
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: Space.xxs) {
            row
            Rectangle().fill(roles.separator.color).frame(height: Stroke.hairline)
            theirs
            yours.frame(maxWidth: .infinity, alignment: .trailing)
        }
        .padding(Space.s)
        .frame(maxWidth: .infinity, minHeight: Self.size.height, maxHeight: Self.size.height, alignment: .top)
        .background(roles.ground.color)
        .clipShape(RoundedRectangle(cornerRadius: Radius.control))
        .accessibilityHidden(true)
    }

    /// Lisbon Climbing's row, unread, as the chat list draws one: the avatar in a sender's colour with
    /// the online dot, the name, Rui's line, the time in the accent drawn as text and the count on the accent.
    var row: some View {
        HStack(spacing: Space.xs) {
            Circle().fill(roles.sender(2).color).frame(width: 16, height: 16)
                .overlay(alignment: .bottomTrailing) { Circle().fill(roles.online.color).frame(width: 6, height: 6) }
            VStack(alignment: .leading, spacing: 0) {
                Text("Lisbon Climbing").bold().foregroundStyle(roles.text.color)
                Text("Rui: Sunday at 9?").foregroundStyle(roles.secondary.color)
            }
            Spacer(minLength: 0)
            VStack(alignment: .trailing, spacing: 0) {
                Text("13:58").foregroundStyle(roles.accentText.color)
                Text("2").bold().foregroundStyle((roles.isDark ? roles.ground : RGBA(0xFFFFFF)).color)
                    .padding(.horizontal, Space.xs)
                    .background(Capsule().fill(roles.accent.color))
            }
        }
        .textStyle(.micro)
        .lineLimit(1)
        .padding(Space.xs)
        .background(RoundedRectangle(cornerRadius: Radius.small).fill(roles.surface.color))
    }

    /// Their bubble on the surface: Ana's name in another sender's colour over a line with @Pedro, a
    /// mention of you, in the mention colour on a tint of it, as `MessageLayout` draws one.
    var theirs: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Ana").bold().foregroundStyle(roles.sender(0).color)
            HStack(spacing: 0) {
                Text("@Pedro").bold().foregroundStyle(roles.mention.color).background(roles.mentionTint.color)
                Text(" bring the rope").foregroundStyle(roles.text.color)
            }
        }
        .textStyle(.micro)
        .lineLimit(1)
        .padding(Space.xs)
        .background(RoundedRectangle(cornerRadius: Radius.small).fill(roles.surface.color))
    }

    /// Yours: "On it" in your bubble, its time in the secondary text's colour, as the conversation draws your bubble's, and its read ticks in the link colour.
    var yours: some View {
        HStack(spacing: Space.xxs) {
            Text("On it").foregroundStyle(roles.text.color)
            Text("13:59").foregroundStyle(roles.secondary.color)
            Text("✓✓").foregroundStyle(roles.link.color)
        }
        .textStyle(.micro)
        .padding(Space.xs)
        .background(RoundedRectangle(cornerRadius: Radius.small).fill(roles.yours.color))
    }
}

/// A card in the gallery: the half the slot chosen for would take, as a small chat, that half's name
/// under it in the system's label colour, a tick and the system's accent around it when it's that
/// slot's theme, and a ring under the pointer. VoiceOver reads it as its label: the half's name, its
/// section and the slots it fills.
public struct ThemeCard: View {
    let theme: Theme
    let slot: Slot
    let roles: Roles
    let ticked: Bool
    let hovered: Bool
    let label: String
    @State private var pointer = false

    /// `hovered` draws it as under the pointer, for the harness's pictures.
    public init(theme: Theme, slot: Slot, roles: Roles, ticked: Bool, hovered: Bool = false, label: String) {
        self.theme = theme
        self.slot = slot
        self.roles = roles
        self.ticked = ticked
        self.hovered = hovered
        self.label = label
    }

    public var body: some View {
        VStack(spacing: Space.xs) {
            ThemeChat(roles)
                .overlay {
                    RoundedRectangle(cornerRadius: Radius.control)
                        .strokeBorder(ring, lineWidth: ticked || hovered || pointer ? Stroke.focus : Stroke.hairline)
                }
                .overlay(alignment: .topTrailing) {
                    if ticked {
                        Image(systemName: "checkmark.circle.fill").symbolRenderingMode(.palette)
                            .foregroundStyle(.white, Color.accentColor)
                            .padding(Space.xs)
                    }
                }
            Text(theme.name(slot)).font(.callout).lineLimit(1)
        }
        .contentShape(Rectangle())
        .onHover { pointer = $0 }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityAddTraits(ticked ? [.isButton, .isSelected] : .isButton)
    }

    var ring: Color {
        if ticked { return .accentColor }
        return hovered || pointer ? Color(nsColor: .secondaryLabelColor) : Color(nsColor: .separatorColor)
    }
}

/// A gallery section's cards, three across in rows of their own, each the half `slot` would take and
/// ticked when it's the slot's theme: a click chooses it, and ⌥-click or its menu's Use for Dark Only
/// (or Light Only) chooses it for that slot alone; VoiceOver's press and its named action do the same.
/// The rows are laid out whole, not lazily, so the gallery can scroll to any card an arrow reaches.
public struct GalleryGrid: View {
    let themes: [Theme]
    let slot: Slot
    let slots: Slots
    let choose: (Theme, _ only: Bool) -> Void

    public init(themes: [Theme], slot: Slot, slots: Slots, choose: @escaping (Theme, _ only: Bool) -> Void) {
        self.themes = themes
        self.slot = slot
        self.slots = slots
        self.choose = choose
    }

    public var body: some View {
        VStack(spacing: Space.m) {
            ForEach(Array(stride(from: 0, to: themes.count, by: 3)), id: \.self) { start in
                HStack(alignment: .top, spacing: Space.m) {
                    ForEach(themes[start ..< min(start + 3, themes.count)]) { theme in
                        card(theme)
                    }
                    ForEach(0 ..< 3 - min(3, themes.count - start), id: \.self) { _ in
                        Color.clear.frame(maxWidth: .infinity, maxHeight: 0)
                    }
                }
            }
        }
    }

    func card(_ theme: Theme) -> some View {
        ThemeCard(theme: theme, slot: slot, roles: theme.roles(slot) ?? Catalog.jaw.roles(dark: slot.isDark),
                  ticked: slots[slot] == theme.id, label: Gallery.label(theme, for: slot, slots))
            .onTapGesture { choose(theme, NSEvent.modifierFlags.contains(.option)) }
            .contextMenu {
                Button("Use for \(slot.title) Only") { choose(theme, true) }
            }
            .accessibilityAction { choose(theme, false) }
            .accessibilityAction(named: "Use for \(slot.title) Only") { choose(theme, true) }
            .id(theme.id)
    }
}

/// A gallery section as the Appearance pane draws it: its header, its cards three across, and its
/// footer, which says why while jawd's themes aren't in.
public struct GallerySection: View {
    let section: Gallery.Section
    let slot: Slot
    let slots: Slots
    let choose: (Theme, _ only: Bool) -> Void

    public init(section: Gallery.Section, slot: Slot, slots: Slots, choose: @escaping (Theme, _ only: Bool) -> Void) {
        self.section = section
        self.slot = slot
        self.slots = slots
        self.choose = choose
    }

    public var body: some View {
        SettingsSection(section.header, footer: section.footer) {
            if !section.themes.isEmpty {
                GalleryGrid(themes: section.themes, slot: slot, slots: slots, choose: choose)
            }
        }
    }
}

/// A section as the system's grouped forms draw one, but open rather than boxed, as the wireframes
/// draw the slots and the gallery: its header, what it holds, and its footer.
public struct SettingsSection<Content: View>: View {
    let title: String
    let footer: String?
    let content: Content

    public init(_ title: String, footer: String? = nil, @ViewBuilder content: () -> Content) {
        self.title = title
        self.footer = footer
        self.content = content()
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            Text(title).font(.headline)
            content
            if let footer {
                Text(footer).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A slot's tile: what it draws, as a card at half size, with the slot's name, its theme's name and
/// source, and whether it's in use or when it would be; outlined in the system's accent while the
/// gallery chooses for it.
public struct SlotTileView: View {
    let tile: SlotTile
    let roles: Roles
    let outlined: Bool

    public init(tile: SlotTile, roles: Roles, outlined: Bool) {
        self.tile = tile
        self.roles = roles
        self.outlined = outlined
    }

    public var body: some View {
        HStack(spacing: Space.m) {
            ThemeChat(roles)
                .frame(width: ThemeChat.size.width, height: ThemeChat.size.height)
                .scaleEffect(0.5)
                .frame(width: ThemeChat.size.width / 2, height: ThemeChat.size.height / 2)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(tile.slot.title).font(.headline)
                Text(tile.name).font(.callout).lineLimit(2)
                if let note = tile.note {
                    Text(note).font(.caption).foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(Space.xs)
        .frame(maxWidth: .infinity, minHeight: 64)
        .overlay {
            RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(outlined ? Color.accentColor : Color(nsColor: .separatorColor), lineWidth: outlined ? Stroke.focus : Stroke.hairline)
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(outlined ? [.isButton, .isSelected] : .isButton)
    }
}
```

Create `mac/JawUI/Sources/Settings/ArrowKeys.swift`:

```swift
import AppKit
import SwiftUI

/// The gallery's arrow keys, from an AppKit view behind the pane that takes the window's keys as the
/// pane opens and again after a click on a card or a tile, so the arrows work with no click first and
/// whatever the Mac's keyboard navigation is, where a SwiftUI view would need focus it may not be
/// given. `moved` is told each arrow and says whether a card was chosen; every other key goes on.
struct ArrowKeys: NSViewRepresentable {
    /// Counted up by a click on a card or a tile, so the keys come back to the gallery.
    let clicked: Int
    let moved: (Gallery.Arrow) -> Bool

    func makeNSView(context _: Context) -> ArrowCatcher {
        ArrowCatcher()
    }

    func updateNSView(_ view: ArrowCatcher, context _: Context) {
        view.moved = moved
        if view.clicked != clicked {
            view.clicked = clicked
            view.takeKeys()
        }
    }
}

/// An invisible view that holds its window's keys for the gallery's arrows, passing every other key,
/// and every arrow with a modifier, on up the responder chain.
final class ArrowCatcher: NSView {
    var moved: (Gallery.Arrow) -> Bool = { _ in false }
    var clicked = 0

    override var acceptsFirstResponder: Bool {
        true
    }

    /// It's there for the keys alone, so clicks go through it to the pane.
    override func hitTest(_: NSPoint) -> NSView? {
        nil
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        takeKeys()
    }

    /// Becomes its window's first responder once AppKit has placed the pane.
    func takeKeys() {
        DispatchQueue.main.async { [weak self] in
            guard let self, let window = self.window, window.firstResponder !== self else { return }
            window.makeFirstResponder(self)
        }
    }

    override func keyDown(with event: NSEvent) {
        let arrows: [UInt16: Gallery.Arrow] = [123: .left, 124: .right, 125: .down, 126: .up]
        if event.modifierFlags.intersection([.command, .option, .control, .shift]).isEmpty, let arrow = arrows[event.keyCode], moved(arrow) {
            return
        }
        super.keyDown(with: event)
    }
}
```

- [ ] **Step 5: Run the tests, then look at the pictures and keep them**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 1, every test passing but two: the pictures', which record `theme-gallery-dark: no reference yet; wrote theme-gallery-dark.png to look at and commit` and the same for light, and the arrows pressed in a window, whose pane is still Task 5's menus, with no `ArrowCatcher`, until Step 6.

Run: `mac/scripts/build-mac references`, then look at `mac/Tests/JawUITests/References/theme-gallery-dark.png` and `theme-gallery-light.png` with the Read tool.
Expected: 41 cards and 28, three across, each a small chat on its theme's ground, its half's name under it: the row's name, line, time and count, the separator, Ana's bubble with @Pedro on its tint, and yours with its ticks, all readable, nothing cut but the row's name in a narrow card. Tokyo Night first, Tokyo Night Day in the light one, then Omarchy's from Catppuccin, then Neutral in macOS's blue, Redlamp, Kanagawa and on. If a card is cut or unreadable, fix `ThemeChat` and run again, deleting the two pictures first.

- [ ] **Step 6: The pane**

In `mac/JawUI/Sources/Style/Preferences.swift`, after `omarchyState`, add:

```swift
    /// The slot the gallery chooses for, nil for the one in use; Settings sets it back each time it
    /// opens, and it isn't kept.
    public var choosing: Slot? = nil
```

In `mac/JawUI/Sources/Settings/Panes.swift`, `AppearancePane` becomes:

```swift
/// Settings › Appearance: dark, light or the system's; the slots, Dark and Light, each a tile of what it
/// draws; and the gallery that fills the outlined one, Jaw's, Omarchy's and Redlamp's themes each a
/// card. It scrolls as a whole, as a grouped form does, the slots and the gallery open under their
/// headers as the wireframes draw them, so an arrow's card can be scrolled to; the arrows move through
/// the cards from the moment it opens, choosing each as they reach it.
public struct AppearancePane: View {
    @Bindable var preferences: Preferences
    @Environment(\.colorScheme) private var scheme
    /// Counted up by each click on a card or a tile, which gives the gallery the window's keys again.
    @State private var clicked = 0

    public init(preferences: Preferences) {
        self.preferences = preferences
    }

    /// The slot the gallery chooses for: the one picked, or the one in use.
    var slot: Slot {
        preferences.choosing ?? (scheme == .dark ? .dark : .light)
    }

    public var body: some View {
        let gallery = Gallery(preferences.themes, for: slot, omarchyLine: preferences.omarchyLine)
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: Space.xl) {
                    GroupBox {
                        Picker("Appearance", selection: $preferences.appearance) {
                            Text("The system's").tag(Appearance.system)
                            Text("Dark").tag(Appearance.dark)
                            Text("Light").tag(Appearance.light)
                        }
                        .pickerStyle(.radioGroup)
                        .horizontalRadioGroupLayout()
                        .padding(Space.xs)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    SettingsSection("Themes", footer: "Jaw draws the dark theme while the Mac is dark, and the light one while it's light.") {
                        HStack(spacing: Space.m) {
                            ForEach(Slot.allCases, id: \.self) { each in
                                SlotTileView(tile: SlotTile(each, slots: preferences.slots, themes: preferences.themes,
                                                            served: preferences.served != nil, appearance: preferences.appearance,
                                                            dark: scheme == .dark),
                                             roles: preferences.roles(each), outlined: each == slot)
                                    .onTapGesture { pick(each) }
                                    .accessibilityAction { pick(each) }
                            }
                        }
                    }
                    ForEach(gallery.sections) { section in
                        GallerySection(section: section, slot: slot, slots: preferences.slots) { theme, only in
                            choose(theme, only: only)
                        }
                    }
                }
                .padding(Space.xl)
            }
            .background(ArrowKeys(clicked: clicked) { arrow in move(arrow, in: gallery, proxy) })
        }
        .frame(width: SettingsPane.appearance.size.width, height: SettingsPane.appearance.size.height)
    }

    /// Makes `each` the slot the gallery chooses for, as a click on its tile does.
    func pick(_ each: Slot) {
        preferences.choosing = each
        clicked += 1
    }

    /// Fills the slots with `theme` as a click on its card does, and gives the gallery the keys again.
    func choose(_ theme: Theme, only: Bool) {
        preferences.slots = preferences.slots.choosing(theme, for: slot, only: only)
        clicked += 1
    }

    /// Chooses the card an arrow reaches, as the terminal previews a theme as it moves through its
    /// list, and scrolls it into sight; says whether there was one.
    func move(_ arrow: Gallery.Arrow, in gallery: Gallery, _ proxy: ScrollViewProxy) -> Bool {
        guard let theme = gallery.moving(from: preferences.slots[slot], arrow) else { return false }
        preferences.slots = preferences.slots.choosing(theme, for: slot)
        proxy.scrollTo(theme.id)
        return true
    }
}
```

In `mac/JawUI/Sources/Settings/SettingsPane.swift`, Appearance's size becomes `NSSize(width: 500, height: 640)`.

In `mac/JawApp/Sources/SettingsWindowController.swift`, the controller keeps the preferences and opens on the slot in use: add `    let preferences: Preferences` after `let tabs: SettingsTabs`, `        self.preferences = preferences` as `init`'s first line, and `        preferences.choosing = nil` as `show(_:over:)`'s first line.

In `mac/JawApp/Sources/Bench.swift`'s `press`, Settings takes the keys while it's open, as it would as the key window, which on the build Mac it never is. Lines 179-187, from `if !step, let event = NSEvent.keyEvent(` to the closing brace of that `if`, become:

```swift
            // While Settings is open it takes the keys, as the key window would; the build Mac runs the
            // app inactive, so it never is.
            let settings = model.settings?.window.flatMap { $0.isVisible ? $0 : nil }
            let window = settings ?? NSApp.keyWindow ?? model.window?.window
            if !step, let event = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: mods, timestamp: ProcessInfo.processInfo.systemUptime,
                                            windowNumber: window?.windowNumber ?? 0, context: nil,
                                            characters: typed, charactersIgnoringModifiers: typed, isARepeat: false, keyCode: codes[char] ?? 0)
            {
                // AppKit offers a key with ⌘ to the key window's views before the menu bar, as the
                // library's ⌘↩ is taken; the build Mac runs the app inactive, with no key window, so
                // that's done here first.
                if !(mods.contains(.command) && window?.performKeyEquivalent(with: event) == true) {
                    if let settings, !mods.contains(.command) { settings.sendEvent(event) } else { NSApp.sendEvent(event) }
                }
            }
```

A key with ⌘ that Settings doesn't take still goes to the menu bar through `NSApp`, as before.

- [ ] **Step 7: Build, test and look at the pane**

Run: `mac/scripts/build-mac build`, `mac/scripts/build-mac test JawUITests`, `mac/scripts/build-mac test JawHarnessTests` and `make check-parity`.
Expected: each exits 0, JawUITests with seven more tests than Task 5 left, the arrows pressed in a window among them. `check-components` passes: the card takes its type from `TextStyle.micro`, its corners from `Radius` and its lines from `Stroke`, and the pane's own words use the system's semantic fonts, as the Settings window is the system's look.

A snapshot saves no preference it changes (Task 4's `keeps`), so the arrows pressed below leave the build Mac's preferences for Jaw as they were.

Run: `mac/scripts/build-mac snapshot mac/build/settings/gallery.png --keys settings:appearance -appearance dark -themeDark omarchy:gruvbox -themeLight redlamp:nord`
Expected: the Appearance pane, 640 points tall: the appearance's radio buttons in a row in their box, Dark selected; under "Themes", the Dark tile outlined, "Gruvbox, Omarchy's", "In use", its half-size card in Gruvbox, and the Light tile "Nord, Redlamp's", "Used when Appearance is Light", then the footer; "Jaw's" with Tokyo Night's card and its footer; and "Omarchy's" from Catppuccin, Gruvbox's card, the first of its second row, ticked and ringed in the accent if the picture reaches it (it shows the pane's top 640 points). Nothing cut at the right edge.

Run: `mac/scripts/build-mac snapshot mac/build/settings/arrows.png --keys "settings:appearance,right,right,down" -appearance dark -themeDark jaw -themeLight jaw`
Expected: the Dark tile says "Hackerman, Omarchy's" and Hackerman's card, the second of Omarchy's second row, is ticked: → → ↓ reached the gallery with no click, in an app the build Mac never makes active. `--keys`' lines after each key say the main window's panes, as before.

Run: `mac/scripts/build-mac snapshot mac/build/settings/scrolled.png --keys "settings:appearance,right,right,down,down,down,down,down,down,down,down" -appearance dark -themeDark jaw -themeLight jaw`
Expected: the pane scrolled down to "Redlamp's", Nord's card, the second of its third row, ticked and whole in sight, the slots' tiles scrolled away above. If the tick is right but the card is out of sight, the scrolling is what's wrong: say so in the report rather than taking the picture as it is.

- [ ] **Step 8: Commit**

```bash
git add mac/JawUI/Sources/Settings mac/JawUI/Sources/Style/Preferences.swift mac/JawApp/Sources/SettingsWindowController.swift \
    mac/JawApp/Sources/Bench.swift mac/Tests/JawUITests/GalleryTests.swift mac/Tests/JawUITests/References/theme-gallery-dark.png \
    mac/Tests/JawUITests/References/theme-gallery-light.png
git commit -F- --trailer "Co-authored-by: Cursor <cursoragent@cursor.com>" <<'EOF'
Settings › Appearance gets its slots and its gallery, as the Settings and themes design and its wireframes draw them: under Themes, a tile for Dark and one for Light, each its theme's card at half size with its half's name and source and whether it's in use or when it would be, the outlined one the slot the gallery chooses for, and under them Jaw's, Omarchy's and Redlamp's sections of cards three across, open under their headers with their footers, each a small chat in the half that slot would take, an unread row, Ana's mention of you and your reply with its ticks in the link colour, named for its half, so Jaw's light one is Tokyo Night Day, and ticked and ringed in the accent when it's the slot's theme; a click fills the slots as a choice does, ⌥-click or the card's menu fills only the slot chosen for, VoiceOver's press and its named action do the same, and the arrows move through the cards from the moment the pane opens, with no click and whatever the keyboard navigation, through an AppKit view that holds the window's keys, ← and → along the gallery and ↑ and ↓ its columns, choosing each card as they reach it and scrolling it into sight; while jawd's themes aren't in, the Omarchy section shows the slots' own from their copies and says why; a tile says why its slot draws Tokyo Night or Tokyo Night Day when it does; --keys reaches Settings while it's open; and two reference pictures hold every theme's card for each slot, so a change to any mapping shows
EOF
```

---

### Task 7: The harness draws every theme, and Settings

**Files:**
- Create: `mac/JawHarness/Sources/Fixtures/Themes.swift`, `mac/JawHarness/Sources/Scenes/SettingsScenes.swift`
- Modify: `mac/Project.swift` (the `JawHarness` target's `resources`, line 138, and the comment above it), `mac/JawHarness/Sources/HarnessScene.swift` (`HarnessSection` at lines 6-36, `Conditions.roles` at 49-51), `mac/JawHarness/Sources/HarnessLaunch.swift` (its doc comment and `--theme` at lines 39-41), `mac/JawHarness/Sources/ConditionsControls.swift` (the Theme picker at 12-14), `mac/JawHarness/Sources/BuiltInScenes.swift` (after `catalog.register(.contactForm)`), `mac/JawThemes/Sources/Roles.swift` (`Catalog.jaw`'s doc comment, `families` and `family(_:)` at lines 59-67)
- Test: `mac/Tests/JawHarnessTests/ScenesTests.swift` (the list of scenes at 53-59), `mac/Tests/JawHarnessTests/LaunchTests.swift`, `mac/Tests/JawHarnessTests/CatalogTests.swift`

**Interfaces:**
- Consumes: `Theme` with `roles(_:)` and `name(_:)`, `ThemeSource`, `Slot`, `Slots`, `OmarchyTheme(id:file:)`, `Catalog.jaw`, `Catalog.jawTheme`, `Catalog.themes(omarchy:accent:)`, `Catalog.roles(_:of:omarchy:accent:)`, `SystemAccent.blue`, and `Preferences(defaults:accent:)` with `look`, `density`, `textSize`, `font`, `appearance` and `slots` (Task 4); `Preferences.served` and `OmarchyState` with `line` (Task 5); `AppearancePane`, `MessagesPane`, `PanesPane`, `SettingsPane` (Task 1); `Gallery` with `sections` and `label(_:for:_:)`, `Gallery.Section(source:themes:line:)`, `GalleryGrid`, `GallerySection`, `ThemeCard`, `ThemeChat`, `SlotTile`, `SlotTileView` (Task 6); the harness's `Fixtures`, `SpecimenGroup` and `HarnessScene`.
- Produces, in the harness: `Fixtures.omarchy: [OmarchyTheme]` and `Fixtures.themes: [Theme]`; `HarnessSection.settings`; the scenes `settings-panes`, `theme-gallery` and `theme-states`; `--theme jaw|omarchy:<id>|redlamp:<id>`.
- Removes: `Catalog.families` and `Catalog.family(_:)`, which nothing reads once the harness lists every theme.

- [ ] **Step 1: Write the failing tests**

In `mac/Tests/JawHarnessTests/ScenesTests.swift`, the list in `every scene draws as wide as it says, says what to look for, and is named for a script` gains the three scenes after `"contact-form"`:

```swift
            "poll-form", "contact-sheet", "contact-form", "settings-panes", "theme-gallery", "theme-states", "states",
```

In `mac/Tests/JawHarnessTests/LaunchTests.swift`, add:

```swift
@MainActor @Test func `--theme takes any theme's ID, Jaw's, Omarchy's or Redlamp's, and nothing else`() {
    #expect(HarnessLaunch.parse(["JawHarness", "--theme", "omarchy:gruvbox"]).conditions.theme == "omarchy:gruvbox")
    #expect(HarnessLaunch.parse(["JawHarness", "--theme", "redlamp:nord"]).conditions.theme == "redlamp:nord")
    #expect(HarnessLaunch.parse(["JawHarness", "--theme", "nord"]).conditions.theme == "jaw", "an ID names its source")
}
```

In `mac/Tests/JawHarnessTests/CatalogTests.swift`, add:

```swift
@Test func `a scene is drawn in any theme's half for its appearance, and in Tokyo Night's where the theme has none`() throws {
    #expect(Fixtures.omarchy.count == 23 && Fixtures.themes.count == 46, "Omarchy's, read from the harness's bundle, beside Jaw's and Redlamp's")
    var conditions = Conditions()
    conditions.theme = "redlamp:nord"
    #expect(conditions.roles == (try #require(Fixtures.themes.first { $0.id == "redlamp:nord" })).dark)
    conditions.theme = "omarchy:gruvbox"
    conditions.dark = false
    #expect(conditions.roles == Catalog.jaw.light, "Gruvbox is dark only")
    conditions.theme = "redlamp:neutral"
    #expect(conditions.roles.accent.hex == 0x007AFF, "Neutral in macOS's blue, whatever the Mac's accent")
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawHarnessTests`
Expected: exit 1, with `error: type 'Fixtures' has no member 'omarchy'` from `CatalogTests.swift`.

- [ ] **Step 3: Omarchy's themes in the harness's bundle**

In `mac/Project.swift`, the `JawHarness` target's comment and resources become:

```swift
            // The app's icon; the protocol's fixtures, which the scenes decode as jawd sends them; and
            // Omarchy's themes, which they read as jawd serves them.
            resources: ["JawApp/Resources/**", "../protocol/fixtures/*.json", "../protocol/themes/*.toml"],
```

Create `mac/JawHarness/Sources/Fixtures/Themes.swift`:

```swift
import Foundation
import JawThemes

extension Fixtures {
    /// Omarchy's themes as jawd serves them: each `protocol/themes/<id>.toml` in the harness's bundle,
    /// read as jaw-proto reads it, in the order of their IDs. A file that can't be read stops the
    /// harness, saying which.
    static let omarchy: [OmarchyTheme] = (Bundle.main.urls(forResourcesWithExtension: "toml", subdirectory: nil) ?? [])
        .map { url in
            let id = url.deletingPathExtension().lastPathComponent
            guard let text = try? String(contentsOf: url, encoding: .utf8), let theme = OmarchyTheme(id: id, file: text) else {
                fatalError("protocol/themes/\(id).toml doesn't read as jaw-proto reads it")
            }
            return theme
        }
        .sorted { $0.id < $1.id }

    /// Every theme there is to choose, as the gallery lists them, Neutral in macOS 27.0.1's colours
    /// (`SystemAccent.blue`), so the harness's pictures don't depend on the Mac's colours.
    static let themes = Catalog.themes(omarchy: omarchy, accent: .blue)
}
```

- [ ] **Step 4: Every theme in the conditions and on the command line**

In `mac/JawHarness/Sources/HarnessScene.swift`:
- `HarnessSection` gains `    case settings = "Settings"` after `case overlays = "Palette and overlays"`, and its `symbol` gains `        case .settings: "gearshape"` after `.overlays`' line.
- `Conditions.roles` becomes:

```swift
    /// The theme's half for the appearance, or Tokyo Night's where it has none, as the app draws a slot.
    var roles: Roles {
        Fixtures.themes.first { $0.id == theme }?.roles(dark ? .dark : .light) ?? Catalog.jaw.roles(dark: dark)
    }
```

In `mac/JawHarness/Sources/HarnessLaunch.swift`, the doc comment's line ``/// - `--theme <id>`, `--appearance dark|light`, …`` begins ``/// - `--theme jaw|omarchy:<id>|redlamp:<id>`, `--appearance dark|light`,`` and the `--theme` lines become:

```swift
        if let theme = value("--theme"), Fixtures.themes.contains(where: { $0.id == theme }) {
            launch.conditions.theme = theme
        }
```

In `mac/JawHarness/Sources/ConditionsControls.swift`, the Theme picker lists every theme by section:

```swift
            Picker("Theme", selection: $conditions.theme) {
                ForEach(ThemeSource.allCases, id: \.self) { source in
                    Section(source.title) {
                        ForEach(Fixtures.themes.filter { $0.source == source }) { Text($0.name).tag($0.id) }
                    }
                }
            }
```

In `mac/JawThemes/Sources/Roles.swift`, `Catalog.jaw`'s doc comment's last sentence, "Redlamp's families join it in Mac-M5.", becomes "`Catalog.themes` lists it first, as `jawTheme`, before Omarchy's and Redlamp's.", and `public static let families = [jaw]` and `family(_:)` with the blank lines around them go.

- [ ] **Step 5: The Settings scenes**

Create `mac/JawHarness/Sources/Scenes/SettingsScenes.swift`:

```swift
import AppKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var settingsPanes: HarnessScene {
        HarnessScene(
            id: "settings-panes", title: "Settings' panes", symbol: "gearshape",
            synopsis: "Appearance, Messages and Panes at the window's width, each at its own height, as the toolbar's tabs show them",
            section: .settings, covers: [AppearancePane.self, MessagesPane.self, PanesPane.self, SlotTileView.self],
            width: SettingsPane.appearance.size.width,
        ) { SettingsPanesScene(conditions: $0) }
    }

    static var themeGallery: HarnessScene {
        HarnessScene(
            id: "theme-gallery", title: "Theme gallery", symbol: "square.grid.3x3",
            synopsis: "Every theme's card for the slot of the appearance, as the gallery draws them: each should read, and none look like another",
            section: .settings, covers: [GalleryGrid.self, ThemeCard.self, ThemeChat.self],
            width: ThemeChat.size.width * 3 + Space.m * 2,
        ) { ThemeGalleryScene(conditions: $0) }
    }

    static var themeStates: HarnessScene {
        HarnessScene(
            id: "theme-states", title: "Theme states", symbol: "checkmark.circle",
            synopsis: "A card at rest, under the pointer and filling its slot; the slots' tiles; and the Omarchy section while jawd starts, can't be reached and is older",
            section: .settings, covers: [ThemeCard.self, SlotTileView.self, GallerySection.self],
            width: SettingsPane.appearance.size.width,
        ) { ThemeStatesScene(conditions: $0) }
    }
}

/// Settings' preferences for a scene: the harness's own, emptied once a launch so each starts from the
/// app's defaults; the scene's look, density, text size, font and appearance; its theme in the slot of
/// its appearance and Jaw's in the other; Omarchy's themes as jawd serves them; and Neutral in macOS
/// 27.0.1's colours (`SystemAccent.blue`), so the pictures don't depend on the Mac's colours.
@MainActor
func scenePreferences(_ conditions: Conditions) -> Preferences {
    let preferences = Preferences(defaults: SettingsDefaults.shared, accent: .blue)
    preferences.look = conditions.look
    preferences.density = conditions.density
    preferences.textSize = conditions.textSize
    preferences.font = conditions.font
    preferences.appearance = conditions.dark ? .dark : .light
    preferences.served = Fixtures.omarchy
    preferences.slots = conditions.dark ? Slots(dark: conditions.theme) : Slots(light: conditions.theme)
    return preferences
}

/// The harness's own defaults for Settings, emptied as they're made, the first time a scene makes
/// preferences in a launch.
private enum SettingsDefaults {
    static let shared: UserDefaults = {
        let suite = "\(Bundle.main.bundleIdentifier ?? "JawHarness").settings"
        let defaults = UserDefaults(suiteName: suite)!
        defaults.removePersistentDomain(forName: suite)
        return defaults
    }()
}

/// Settings' three panes as the toolbar's tabs show them, each at its size on the window's colour.
private struct SettingsPanesScene: View {
    let roles: Roles
    @State private var preferences: Preferences

    init(conditions: Conditions) {
        roles = conditions.roles
        _preferences = State(initialValue: scenePreferences(conditions))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "Appearance",
                note: "The appearance in a row; the slots, the one in use outlined and saying so, the other saying when it would be, each its theme's card at half size with its name and source; then the gallery's first sections. Nothing should be cut at the right edge.",
                roles: roles,
            ) { pane(AppearancePane(preferences: preferences)) }
            SpecimenGroup(
                title: "Messages",
                note: "Bubbles or lines, then spacing, text size and font, each explanation its section's footer, as the sheet had them.",
                roles: roles,
            ) { pane(MessagesPane(preferences: preferences)) }
            SpecimenGroup(
                title: "Panes",
                note: "How much the other panes are dimmed, and the focused pane's outline, its colour the theme's accent until one's picked.",
                roles: roles,
            ) { pane(PanesPane(preferences: preferences)) }
        }
    }

    /// A pane on the window's colour, as the window holds it.
    func pane(_ view: some View) -> some View {
        view.background(Color(nsColor: .windowBackgroundColor)).clipShape(RoundedRectangle(cornerRadius: Radius.panel))
    }
}

/// Every theme's card for the slot of the scene's appearance, three across at 144 points, as the
/// gallery draws them, the scene's theme ticked.
private struct ThemeGalleryScene: View {
    let conditions: Conditions

    var slot: Slot {
        conditions.dark ? .dark : .light
    }

    var body: some View {
        let slots = conditions.dark ? Slots(dark: conditions.theme) : Slots(light: conditions.theme)
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Gallery(Fixtures.themes, for: slot, omarchyLine: nil).sections) { section in
                SpecimenGroup(title: "\(section.source.title), for \(slot.title)", note: note(section.source), roles: conditions.roles) {
                    GalleryGrid(themes: section.themes, slot: slot, slots: slots) { _, _ in }
                }
            }
        }
    }

    func note(_ source: ThemeSource) -> String {
        switch source {
        case .jaw: "Tokyo Night, or Tokyo Night Day for Light: the app's own, as it was before the slots."
        case .omarchy: "The 18 dark or the 5 light, in jawd's order, each mapped as the terminal maps it and held where it's drawn: no line should be hard to read."
        case .redlamp: "Neutral in macOS 27.0.1's colours, then Redlamp and the twenty from omoi-station, held by the same bands and where they're drawn: no line should be faint, Redlamp's red only its accent, its light half's mention a brown and no sender's name red."
        }
    }
}

/// A card in each of its states, the slots' tiles, and the Omarchy section while jawd's themes aren't
/// in, each as the Appearance pane draws it.
private struct ThemeStatesScene: View {
    let conditions: Conditions

    var slot: Slot {
        conditions.dark ? .dark : .light
    }

    /// The scene's theme, or Jaw's where it has no half for the scene's appearance.
    var theme: Theme {
        Fixtures.themes.first { $0.id == conditions.theme && $0.roles(slot) != nil } ?? Catalog.jawTheme
    }

    var body: some View {
        let roles = theme.roles(slot) ?? conditions.roles
        let slots = conditions.dark ? Slots(dark: theme.id) : Slots(light: theme.id)
        let empty = Slots(dark: "", light: "")
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "A card",
                note: "At rest, under the pointer, and filling the slot: ringed in the separator, then in the secondary label colour, then ticked and ringed in the system's accent.",
                roles: conditions.roles,
            ) {
                HStack(alignment: .top, spacing: Space.m) {
                    ThemeCard(theme: theme, slot: slot, roles: roles, ticked: false, label: Gallery.label(theme, for: slot, empty))
                    ThemeCard(theme: theme, slot: slot, roles: roles, ticked: false, hovered: true, label: Gallery.label(theme, for: slot, empty))
                    ThemeCard(theme: theme, slot: slot, roles: roles, ticked: true, label: Gallery.label(theme, for: slot, slots))
                }
                .frame(width: ThemeChat.size.width * 3 + Space.m * 2)
            }
            SpecimenGroup(
                title: "The slots",
                note: "The slot chosen for outlined in the accent; the one in use says so, and the other when it would be, as Appearance holds one.",
                roles: conditions.roles,
            ) {
                HStack(spacing: Space.m) {
                    ForEach(Slot.allCases, id: \.self) { each in
                        SlotTileView(tile: SlotTile(each, slots: slots, themes: Fixtures.themes, served: true,
                                                    appearance: conditions.dark ? .dark : .light, dark: conditions.dark),
                                     roles: Catalog.roles(each, of: slots, omarchy: Fixtures.omarchy, accent: .blue), outlined: each == slot)
                    }
                }
            }
            SpecimenGroup(
                title: "Omarchy's, while jawd's themes aren't in",
                note: "The slots' own from their copies, here Gruvbox, under \"Omarchy's, from jawd\", and one line in jawd's state where the section's footer goes: starting, not answering, older.",
                roles: conditions.roles,
            ) {
                VStack(spacing: Space.xl) {
                    ForEach([OmarchyState.starting, .unreachable, .older], id: \.self) { state in
                        GallerySection(section: Gallery.Section(source: .omarchy, themes: Fixtures.themes.filter { $0.id == "omarchy:gruvbox" },
                                                                line: state.line),
                                       slot: .dark, slots: Slots(dark: "omarchy:gruvbox")) { _, _ in }
                            .padding(Space.l)
                            .background(Color(nsColor: .windowBackgroundColor))
                            .clipShape(RoundedRectangle(cornerRadius: Radius.panel))
                    }
                }
            }
        }
    }
}
```

In `mac/JawHarness/Sources/BuiltInScenes.swift`, after `catalog.register(.contactForm)`, add:

```swift
        catalog.register(.settingsPanes)
        catalog.register(.themeGallery)
        catalog.register(.themeStates)
```

- [ ] **Step 6: Run the tests and look at the scenes**

Run: `mac/scripts/build-mac test JawHarnessTests`
Expected: exit 0, `** TEST SUCCEEDED **`: the new scenes draw as wide as they say, each with notes and the same picture twice, and the two new tests pass.

Run, one at a time:

```bash
mac/scripts/build-mac shot settings-panes mac/build/settings/settings-panes-dark.png --appearance dark
mac/scripts/build-mac shot settings-panes mac/build/settings/settings-panes-light.png --appearance light
mac/scripts/build-mac shot theme-gallery mac/build/settings/theme-gallery-dark.png --appearance dark --theme omarchy:gruvbox
mac/scripts/build-mac shot theme-gallery mac/build/settings/theme-gallery-light.png --appearance light --theme omarchy:gruvbox
mac/scripts/build-mac shot theme-states mac/build/settings/theme-states-dark.png --appearance dark --theme redlamp:nord
mac/scripts/build-mac shot theme-states mac/build/settings/theme-states-light.png --appearance light --theme redlamp:nord
```

Expected: each picture, looked at with the Read tool, shows what its notes ask for: the three panes whole at 500 points, Appearance's first rows; every card for the slot, Jaw's named Tokyo Night Day in the light gallery, Gruvbox's ticked in the dark one and nothing ticked in the light one, where Gruvbox has no half; Nord's card at rest, under the pointer and ticked, the two tiles, and the three Omarchy sections, each "Omarchy's, from jawd" with Gruvbox and its line under it.

Then `mac/scripts/build-mac build`, `mac/scripts/build-mac test JawThemes`, `mac/scripts/build-mac test JawUITests` and `make check-parity`; each exits 0.

- [ ] **Step 7: Commit**

```bash
git add mac/Project.swift mac/JawHarness/Sources mac/JawThemes/Sources/Roles.swift mac/Tests/JawHarnessTests
git commit -F- --trailer "Co-authored-by: Cursor <cursoragent@cursor.com>" <<'EOF'
The harness draws every theme the app offers, as the Settings and themes design asks: Omarchy's 23 are copied into its bundle from protocol/themes and read as jaw-proto reads them, its Theme condition lists Jaw's, Omarchy's and Redlamp's by section, --theme takes jaw, omarchy:<id> or redlamp:<id>, Neutral is drawn in macOS 27.0.1's colours (SystemAccent.blue) so no picture depends on the Mac's colours, and the Roles scene draws any of them with its contrast; a Settings section draws the three panes at the window's width, every card for the slot of the appearance, and a card at rest, under the pointer and filling its slot, the slots' tiles and the Omarchy section while jawd starts, can't be reached and is older; and Catalog.families and Catalog.family, which nothing reads now, go
EOF
```

---

### Task 8: The docs, and the branch left ready

**Files:**
- Modify: `docs/plans/2026-10-26-jaw-mac-design.md` (Decisions, line 30; "The UI stack", line 181; "Themes", lines 357-390; "Code", line 488; "Testing", line 501; "Milestones", line 515), `docs/plans/2026-10-26-jaw-mac-plan.md` (Decisions, line 35; Mac-M5's opening, line 145, and its steps 1 and 6, lines 149 and 154; "Alongside", line 171), `docs/plans/2026-10-27-jaw-mac-parity-plan.md` (line 27; step 3.5, lines 156-160; Phase 4, line 202; the owner's answers, line 221), `mac/README.md` (lines 25 and 39)
- Outside the repository: the parity canvas, `jaw-feature-parity.canvas.tsx` in the project's canvases (`/Users/pedrogomes/.cursor/projects/Users-pedrogomes-src-jaw/canvases/`), lines 348-349
- Unchanged, on purpose: `mac/parity.toml`, whose rows for step 3.5, `settings.set` and the `settings` push, are its other half, the settings jawd keeps (P5)

**Interfaces:**
- Consumes: what Tasks 1 to 7 built, as their commits say it.
- Produces: docs that say what's built: Settings in a window of its own, the slots and the gallery, Omarchy's themes from jawd, Redlamp's copied under jaw's MIT by its owner, `sync-themes` run by hand rather than in CI.

- [ ] **Step 1: The Mac design**

In `docs/plans/2026-10-26-jaw-mac-design.md`:

The decision on line 30 becomes:

```markdown
- **Redlamp's themes copied into jaw, with a check that keeps them in step** (Owner, 26 Oct), **and Omarchy's from jawd beside them, a theme for dark and one for light** (Owner, 6 Nov). `RedlampDesign` is a Tuist framework that depends on Redlamp's engine API and is mostly a photo editor's controls; jaw needs only its ten-colour theme families, which their owner relicensed for jaw under its MIT licence, and `scripts/sync-themes` keeps the copy diffable, as `scripts/sync-protos` keeps whatsmeow's protobufs. "Themes" has the slots and the gallery.
```

In "The UI stack"'s decision, line 181, `the palette (in a panel of its own), settings, sheets` becomes `the palette (in a panel of its own), settings (in a window of its own, its panes the toolbar's tabs), sheets`.

"Themes", from `## Themes` to the line before `## Animation`, becomes:

```markdown
## Themes

Settings › Appearance holds a theme for dark and one for light, each chosen from a gallery of Jaw's Tokyo Night, Omarchy's 23 from jawd and Redlamp's 22 families, each drawn as a small chat in its colours ([the Settings and themes design](2026-11-06-jaw-mac-settings-and-themes-design.md), settled with the owner on 6 November).

### Redlamp's catalog

- **Redlamp keeps themes as families,** each a matched dark and light pair, so a choice survives an appearance change. A family is either authored directly or ten colours (`background`, `card`, `foreground`, `muted`, `primary`, `accent`, `border`, `success`, `destructive`, `neutral`, and `primaryRoles`, which keeps a brand colour to the places it's allowed), mapped to tokens. Its catalog is Neutral, Redlamp, and twenty ported from omoi-station: Kanagawa, Tokyo Night, Moonlight, Catppuccin, Rosé Pine, Nord, Everforest, Gruvbox, Dracula, Solarized, One Dark, Monokai, Ocean, Sakura, Copper, Vinyl Noir, Neon Dusk, Burnt Tape, Phantom Frequency and Velvet Static. Around them: a tint from 0 (the theme's tones in grey) to 1, native controls in the theme's accent or the system's, the panels' translucency (solid under Reduce Transparency), and the palette in a theme of its own, which come in Mac-M5 (Owner, 6 Nov).
- **Its tokens are a photo editor's** (labels, tracks, thumbs, wells, an edited dot), so jaw maps the ten colours to a chat's roles itself, below.
- **Copied, not linked** (Owner, 26 Oct), **under jaw's licence** (Owner, 6 Nov). `RedlampDesign` has no `Package.swift` (it's a Tuist framework target), depends on `RedlampEngineAPI` through its sliders and `TrackGradients.swift`, and is mostly controls jaw doesn't use. Linking it would mean a package carved out of it in Redlamp's repository and jaw's builds reaching a private one. jaw copies what it uses, about 200 lines in `JawThemes/Sources/RedlampCatalog.swift`: `ThemePalette`, `PrimaryRoles`, the HSL parser, the families' ten colours in Redlamp's slot order, and the brand's nine colours the Redlamp family is made of. Its owner relicensed Redlamp's catalog for jaw under jaw's MIT licence ("I own redlamp, MIT is fine"), so the copy carries jaw's licence with a line saying whose it is, and there's no `docs/licenses/redlamp.txt`. `scripts/sync-themes` copies it from a checkout of Redlamp at a pinned commit with `git show`, never touching the checkout, and with `--check` says what differs and what Redlamp's main has changed since, Neutral's tokens included; CI has no checkout of Redlamp, so there it says so and checks nothing.
- **Held readable** (Owner, 6 Nov): mapped by the table below, then held as Omarchy's are, every colour at 3:1 on the ground and the surface, secondary text too, the dim text at 2.5 to 4.5:1, and the selection and your bubbles moved toward their ground until text reads at 4.5:1 on them, readable over exact: 37 of the 42 halves move a little, a photo editor's faint labels most. Neutral is Redlamp's standard greys in the system's colours, its accent, orange, green and red, read in both appearances and again when they change. The Redlamp family keeps its red to its accent: its light half's mentions take the brand's filament, darkened to read on paper, and its senders' eight hues keep 30° or more from the red. Under a grey accent, as Graphite's is, Neutral's senders take the system's blue's hue, so a group's are still eight colours (Owner, 6 Nov).

### Omarchy's themes, from jawd

- **Asked after each hello** with `themes`, never before the first frame; jawd answers at once, and its answer holds while the app runs. The slots' Omarchy themes are remembered with their colours (`themeDarkColours`, `themeLightColours`), so the first frame is drawn in them before jawd answers, and a theme jawd no longer serves is drawn as Tokyo Night in its slot, the choice kept.
- **Mapped as the terminal maps them:** `theme.rs`'s `Roles::of`, ported to Swift with the same keys, bands and straight line toward the ground or the text, so the Mac lands on the terminal's colours, then held at 3:1 on the surface the Mac draws its list and bubbles on, which moves 9 of the 23 a little (Owner, 6 Nov). The Settings and themes design's table has each role's key.

### jaw's own theme

- **jaw, first in the list and the default, following the system's light and dark** (Owner, 26 Oct): ooze's Tokyo Night, the terminal's palette, authored directly, since it has more than ten colours: the ground `#1a1b26`, the surface `#24283b`, the selection `#283457`, text `#c0caf5` and `#a9b1d6`, the quieter `#737aa2` and `#565f89`, borders `#3b4261`, and its eight accents, which are also the eight group senders' colours. Held as every theme is (below), it draws its secondary and dim text a step lighter, `#7e87af` and `#66709a`, and Tokyo Night Day's blue as text and links, its red, its mention and its dim text a shade deeper. Its light half is Tokyo Night Day, Tokyo Night's own light palette. Omarchy's Tokyo Night and Redlamp's, a lighter take, are in their own sections of the gallery.

### The roles

Every colour the app draws is asked for by what it's for, as `theme.rs` asks, never by hue:

| Role | jaw's (ooze's Tokyo Night) | A Redlamp family |
| --- | --- | --- |
| The window's ground | bg `#1a1b26` | `background` |
| The sidebar, the list, their bubbles | surface `#24283b` | `card` |
| Text | fg `#c0caf5` | `foreground` |
| Secondary text: previews, times | muted `#737aa2` | `muted` |
| Quieter text, ticks not yet read (in the secondary text's colour everywhere but on the window's ground and the surface) | comment `#565f89` | `muted` at 62%, Redlamp's tertiary |
| Separators | gutter `#3b4261` | `border` |
| Focus, selection, unread counts | blue `#7aa2f7`, selection `#283457` | `primary`, or `neutral` where `primaryRoles` keeps it out |
| The accent drawn as text: an unread chat's time, Draft:, a mention of someone else, Show more, You in a reply | blue `#7aa2f7` | as the accent |
| Links, read ticks | blue `#7aa2f7` | as the accent |
| Your bubbles | blue at 18% over the ground | `primary` at 18% over `background` (12% in light), `neutral` where `primaryRoles` keeps it out |
| Mentions, the "new" marker | orange `#ff9e64` | `accent`, the brand's filament in the Redlamp family's light half |
| Online, typing | green `#9ece6a` | `success` |
| Failed, destructive | red `#f7768e` | `destructive` |
| Group senders | red, yellow, blue, purple, cyan, green, orange, teal | eight hues 45° apart from `primary`'s, 37.5° apart away from it in the Redlamp family, each lightened or darkened until it reads at 4.5:1 on their bubble |

- **Held where they're drawn** (Owner, 6 Nov): every theme's roles, of every source, then go through one hold, which moves each colour drawn as text or as a mark toward the text until it reads at 3:1 on every ground it's drawn on, the window's, the surface, a selected row, your bubbles and a selected message's band, and the tints laid over them, a card's and a quote's well, the accent's tint behind its symbols, the box's chip and a mention of you on its own tint; a selected row's online at 4.5:1; while the accent's fills, the grounds, the text and the borders keep the theme's colours. The accent drawn as text is a role of its own, and the dim text is held on the window's ground and the surface and drawn in the secondary text's colour everywhere else, so it stays the quietest (the Settings and themes design's "Every theme, held where it's drawn").
- **The slots** (Owner, 6 Nov): a theme for dark and one for light, kept in the app's preferences as `themeDark` and `themeLight` (`jaw`, `omarchy:<id>` or `redlamp:<id>`), both Jaw's Tokyo Night at first, the single `theme` key read into both once. A theme with both halves fills both; one of Omarchy's, which have one mode each, fills the slot of its mode; ⌥-click fills only the slot chosen for. Jaw draws the dark slot's theme while the Mac is dark, and the light one's while it's light.
- **Tint, appearance and translucency** as Redlamp's, in Mac-M5: the tint pulls every role toward the grey of its own lightness; appearance is dark, light or the system's, as built; the sidebar's glass shows through by the translucency setting, solid under Reduce Transparency. Increase Contrast draws borders and secondary text a step stronger.
- **Switching a theme is live:** the roles are read as views draw, and every window redraws on the change, with no relaunch.
- **The Settings' Appearance pane** has the appearance, the two slots as tiles, each its theme's card at half size, and the gallery: Jaw's, Omarchy's and Redlamp's sections of cards three across, each a small chat in the half the slot would take (an unread row, their bubble with a mention of you, yours with its ticks in the link colour), as Redlamp's gallery shows each theme as a small panel. The arrows move through the cards, choosing each as they reach it, and while jawd's themes aren't in, Omarchy's section says why.
```

In "Code", line 488, `  JawThemes/                Redlamp's families (copied), jaw's own, the roles` becomes `  JawThemes/                the roles, jaw's own, Omarchy's mapping, Redlamp's families (copied), the slots`.

In "Testing", line 501, `in both looks, in jaw's theme light and dark and two of Redlamp's, compared with references` becomes `in both looks, in any of the themes, Jaw's, Omarchy's or Redlamp's, light and dark, compared with references`.

In "Milestones", line 515, Mac-M5's `**Polish, and it's jaw:** Redlamp's themes, the animations,` becomes `**Polish, and it's jaw:** the themes' tint and translucency, the animations,`.

- [ ] **Step 2: The Mac plan**

In `docs/plans/2026-10-26-jaw-mac-plan.md`:

- Line 35 becomes `- **Redlamp's themes are copied, with a check that keeps them in step:** jaw needs the families, not Redlamp's controls. Brought forward with Omarchy's by [the Settings and themes plan](2026-11-06-jaw-mac-settings-and-themes-plan.md) (6 November); Mac-M5 step 1 keeps what's left.`
- Mac-M5's opening sentence, line 145, `Redlamp's themes, the animations,` becomes `The themes' tint and translucency, the animations,`, as the Mac design's Milestones row is worded in Step 1.
- Mac-M5's step 1 becomes:

```markdown
1. **Themes:** what [the Settings and themes design](2026-11-06-jaw-mac-settings-and-themes-design.md) left for Mac-M5 (Owner, 6 Nov): the tint, the sidebar's translucency, native controls in the theme's accent, and the palette in a theme of its own. Redlamp's families copied with `scripts/sync-themes` and held readable, Omarchy's from jawd mapped as the terminal maps them, the senders' colours checked at 4.5:1, a slot for dark and one for light, and the Appearance pane's gallery of small chats came before, on `mac-settings-themes`.
```

- In Mac-M5's step 6, `Appearance (now with Redlamp's families),` becomes `Appearance (its slots and gallery since 6 November),`, and the step's first words, `**Settings, whole:**`, become `**Settings, whole,** each pane a tab in the Settings window's toolbar beside Appearance, Messages and Panes:`.
- In "Alongside", line 171 becomes:

```markdown
- **`scripts/sync-themes`** came with the Settings and themes plan. It reads a checkout of Redlamp, whose repository is private, so CI, which has none, doesn't run it; it's run by hand when Redlamp's catalog moves, and `--check` says what Redlamp's main has changed since the pinned commit.
```

- [ ] **Step 3: The parity plan**

In `docs/plans/2026-10-27-jaw-mac-parity-plan.md`:

- Line 27: `The viewer, Redlamp's themes, the animations,` becomes `The viewer, the animations,`.
- Step 3.5's heading, its first paragraph and its first bullet stay; its second and third bullets, ``- **Both sets of themes** (Owner, 27 Oct): …`` and ``- **The check:** `settings.set` and the `settings` push.``, become:

```markdown
- **Both sets of themes** (Owner, 27 Oct), **together** (Owner, 6 Nov): Settings is a window of its own whose Appearance pane has a slot for dark and one for light, filled from a gallery of Jaw's Tokyo Night, Omarchy's 23, read from jawd's `themes` and mapped onto the Mac's roles as the terminal maps them onto its own, and Redlamp's 22 families, brought forward from Mac-M5 ([the Settings and themes design](2026-11-06-jaw-mac-settings-and-themes-design.md) and [its plan](2026-11-06-jaw-mac-settings-and-themes-plan.md)). What's left of 3.5 is jawd's own settings, with P5.
- **The check:** `settings.set` and the `settings` push, which stay in `mac/parity.toml` until the Mac sets jawd's settings. The Mac asks `themes`, which the terminal never needs to, as it reads Omarchy's themes from jaw-proto, so that's no gap.
```

- Phase 4, line 202: `and Mac-M5 (Redlamp's themes, the animations,` becomes `and Mac-M5 (the themes' tint, translucency and native controls, the animations,`.
- The owner's second answer, line 221, becomes:

```markdown
2. **Themes:** both, Omarchy's from jawd's `themes` first and Redlamp's in Mac-M5 (3.5, Phase 4); on 6 November he brought Redlamp's forward, so both come in 3.5.
```

- [ ] **Step 4: The Mac README**

In `mac/README.md`, line 25:
- `` `revert`, `settings`, `terminal-panes` `` becomes `` `revert`, `settings`, which opens Settings, and `settings:appearance`, `settings:messages` or `settings:panes`, which open it on that pane, after which the keys go to Settings, as `right` and `down` move through its gallery, `terminal-panes` ``.
- `(or the sheet over it)` becomes `(or the sheet over it, or Settings with its toolbar once a step has opened it)`.
- ``such as `-appearance light`, which is read before what's saved and isn't saved`` becomes ``such as `-appearance light`, or `-themeDark omarchy:gruvbox` for the theme in the dark slot (`jaw`, `omarchy:<id>` or `redlamp:<id>`), which is read before what's saved and isn't saved; and a snapshot saves no preference it changes, as a theme chosen by its `--keys`, since every checkout's snapshots on the build Mac share them``.

Line 39: `` `--theme <id>` `` becomes `` `--theme jaw|omarchy:<id>|redlamp:<id>` ``.

- [ ] **Step 5: The parity canvas's Mac cells**

In `/Users/pedrogomes/.cursor/projects/Users-pedrogomes-src-jaw/canvases/jaw-feature-parity.canvas.tsx`, the parity workstream's, change only these two strings, each the `m:` cell of its row (line 348, "A settings panel"; line 349, "Themes"):

- `m: ["works", "a sheet: Appearance, Messages and Panes"]` becomes `m: ["works", "a sheet: Appearance, Messages and Panes; a window with the toolbar's tabs on mac-settings-themes"]`.
- `m: ["partial", "jaw's Tokyo Night, dark and light; Omarchy's from jawd's themes next, Redlamp's in Mac-M5"]` becomes `m: ["partial", "jaw's Tokyo Night, dark and light; a slot for each, from a gallery of Jaw's, Omarchy's 23 and Redlamp's 22, on mac-settings-themes"]`.

If either isn't there as quoted, the workstream has changed it since: leave the canvas as it is and say so in the report. The board and the cells' states are the workstream's to move when this lands.

- [ ] **Step 6: Check the docs**

Run: `make check-parity` and `make check-audit`.
Expected: both exit 0. `scripts/check-data-audit.py`'s `DATA_PATHS` name nothing under `mac/`, `scripts/`, `protocol/` or `docs/`, so there's nothing for the audit to find, as the Global Constraints say.

Run: `rg -n "MPL|Redlamp's themes, the animations|now with Redlamp's families|runs in CI after it|join them in Mac-M5" docs/plans/2026-10-26-jaw-mac-design.md docs/plans/2026-10-26-jaw-mac-plan.md docs/plans/2026-10-27-jaw-mac-parity-plan.md`
Expected: no match (rg exits 1): none of the old words for Redlamp's licence, Mac-M5's themes or `sync-themes` in CI is left.

- [ ] **Step 7: Commit**

```bash
git add docs/plans/2026-10-26-jaw-mac-design.md docs/plans/2026-10-26-jaw-mac-plan.md docs/plans/2026-10-27-jaw-mac-parity-plan.md mac/README.md
git commit -F- --trailer "Co-authored-by: Cursor <cursoragent@cursor.com>" <<'EOF'
The Mac's docs say what the Settings and themes plan built: the Mac design's Settings is a window of its own with the toolbar's tabs, its Themes a slot for dark and one for light filled from a gallery of Jaw's Tokyo Night, Omarchy's 23 from jawd mapped as the terminal maps them and held on the surface, and Redlamp's 22 families copied under jaw's MIT by their owner and held readable, every colour held at 3:1 wherever it's drawn, the tints laid over a ground too, with links and read ticks, and the accent drawn as text, in roles of their own, Neutral in the system's colours, the tint and translucency left for Mac-M5; the Mac plan's Mac-M5 keeps what's left of its themes, and sync-themes is run by hand, since CI has no checkout of Redlamp; the parity plan's step 3.5 has both sets of themes together, its check keeping settings.set and the settings push for jawd's own settings; and the README gives --keys settings:<pane>, a theme in a slot by argument and the harness's --theme for any theme
EOF
```

- [ ] **Step 8: Leave the branch ready**

Run: `git fetch origin && git log --oneline HEAD..origin/main`
Expected: nothing; if main has moved, `git rebase origin/main`, then every test again on the build Mac (`test JawKit`, `test JawThemes`, `test JawUITests`, `test JawHarnessTests`, `build`) and `make check-parity`, each exiting 0.

Run: `scripts/sync-themes --check`
Expected: `sync-themes: jaw's copy is Redlamp's catalog at 9fa9d9155`.

Run: `rm -rf mac/build/settings && make prune-builds`
Expected: the scratch pictures gone, and `prune-builds` saying what it freed from this worktree's `target/` and the others'.

Report: the branch's commits (`git log --oneline origin/main..HEAD`: the design, the owner's answers, this plan and its fixes, Task 4b's section and the design's amendments, then one for each of Tasks 1 to 4, Task 1's fix, 4b and 5 to 8), each test run's count, what the pictures showed, any decision a task took that this plan didn't, whether the parity canvas's two cells changed, and what's left for the controller: the budgets, `--bench launch` and `--bench idle` with Settings open, to measure with a Release build, and the owner's look by hand that the design's Testing ends with, the window on a real screen with its toolbar and the gallery, the Mac's appearance switched to Auto while both slots hold themes of their own, and VoiceOver over the gallery, its cards' press and Use for Dark Only among it. Don't push: the controller lands it.
