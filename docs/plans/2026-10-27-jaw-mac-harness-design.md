# Jaw for the Mac: the harness

27 October 2026. A design; nothing here is built. It grows [the Mac design](2026-10-26-jaw-mac-design.md)'s `JawHarness` and its "Screens" test into a harness in the way of Redlamp's and omoi-station's, and moves the window's content into `JawUI`, so every screen is drawn from values. The owner's answers are marked "Owner, 27 Oct"; the build order is under "Building it".

## What it is

Jaw Harness (the `JawHarness` target) is a Mac app for building and reviewing Jaw's interface in isolation. It links the app's own frameworks (`JawKit`, `JawThemes`, `JawUI`), draws every component, every state and every screen from made-up rows and the protocol's fixtures, and never talks to jawd. It serves the two ends the owner gave it:

- **The interface is self-contained and data-driven.** Everything the app draws is in `JawUI` and is made from values; nothing drawn reaches for jawd. Something that can't be put in the harness from fixtures isn't data-driven yet, and a check says so.
- **Everything can be tested and audited, and refined as we go.** A sweep draws every scene in each look and appearance; a review canvas shows them and takes the owner's marks; agents read the marks, fix and sweep again; and a pinned set of pictures fails the tests when a screen changes by accident.

It follows Redlamp's harness (`apps/RedlampHarness` in `~/src/darkroom`) and omoi-station's (`apps/Harness` in `~/src/omoi-station`), which share one shape: scenes by section in a sidebar, one file each, each with a line on what to look at; specimens with a note on what would be wrong with them; the stage's settings kept from scene to scene; an inspector of knobs; launch flags for scripted pictures; and, in Redlamp's, Live scenes with a checklist that ticks each interaction as it's tried.

What's there today, on `mac-m1`, is a first step: one AppKit file with six scenes (the conversation, the chat list, the states, the Mimic, the palette and the theme's roles), bubbles or lines, dark or light, compact, and `--scene`, `--light` and `--snapshot`; beside it, `LayoutTests` compares the conversation's layout with five reference pictures. This design replaces the file and keeps the tests.

## Decisions

- **The harness's own branch and worktree** (Owner, 27 Oct): `mac-harness` in `~/src/jaw-harness`, from `mac-m1`, rebased onto it as it moves and fast-forwarded into it in small steps whenever its worktree has nothing uncommitted. The Mac app's chat commits to `mac-m1` every few minutes, so neither works in the other's checkout.
- **Screens drawn from values** (Owner, 27 Oct). The window's content, the bottom bar, the glance's view and the status window move into `JawUI`, made from values and reporting what's done through hosts and closures; what the screen says becomes pure functions of jawd's state; `AppModel` stays the only code that talks to jawd. Not chosen: the real `AppModel` run against a scripted jawd, which reaches a screen by scripting updates and leaves the views as they are; and components alone, which leaves whole screens to `Jaw.app --demo --snapshot`.
- **A review canvas** (Owner, 27 Oct): the sweep's pictures by section in an Atelier canvas, where the owner marks each scene right, or needs work with a note, and agents read the marks back. Not chosen: a folder of pictures with an index page; the app alone.
- **Every component lands with its scene** (Owner, 27 Oct): `scripts/check-harness` fails the build when a public `JawUI` view or controller isn't covered by a scene, or the app target defines a view of its own.
- **Pinned references, not the whole sweep, in git** (Owner, 27 Oct): about 30 pictures committed and checked in `make mac-test`. The sweep's pictures are drawn again each time and never committed; the five references there today take 3.4 MB on their own.
- **Screens from values come second** (Owner, 27 Oct), after the shell and the loop: the move is the one part that touches the Mac app's chat's files, and they grow every hour.

## The boundary

Everything drawn lives in `JawUI` and is made from values: rows, window updates, state kinds and models. What the user does goes out through a host protocol or closures, as `ChatListHost` and `ConversationHost` already have it. `AppModel` is the only code that talks to jawd: it turns jawd's windows into those values, and what the views report into requests.

### What moves into JawUI

| Today, in `JawApp` | In `JawUI` | Made from |
| --- | --- | --- |
| The split view and the bottom bar, built in `MainWindowController.init` | `WindowContent`, an `NSViewController` | the sidebar, the chat list and the `DetailController` it's given; it folds by its own width, the sidebar below 960 points and the chat list below 740 while a chat is open, so a screen drawn at 740 folds as the window does |
| `BottomBar`, which takes `AppModel` | `BottomBar` | the status line's values, the layout's name and whether it's edited, and its buttons' commands and menus as `Command`s |
| `DetailController` | `DetailController`, as it is | the panes, a state or linking, as today |
| `GlanceView` and `GlanceState` | the same | the chats that need you and their rows; `Glance`, which watches their windows, stays in the app |
| The status window's text, made in `showStatus` | `StatusView` | jawd's `Status` |
| The layout prompts' alerts, made in `askName` and `askLayoutDelete` | `Prompts` | their words; the window shows them |

The sheets are in `JawUI` already (`InfoView`, `KeysView`, `SettingsView`, `LinkingView`, `LayoutEditorView`), as are the palette, the chat list, the conversation, the box, the sidebar and the panes.

### What becomes pure

`Situation`, a value in `JawUI`: how jawd is reached (`waiting`, `starting(failed:)`, `up` or `unreachable(why:retry:)`, which is `AppModel.Reach` today), whether the account is linked, the connection, the link step and the notice. On it, each a function tested over every case:

- `stateLine`: the status line's left half, its words and whether it warns, which is `AppModel.stateLine` today;
- `chatLine(focused:now:)`: how the focused chat is, or the notice while it lasts (`chatLine`);
- `stateView(hasChats:hasPanes:)`: what fills the panes' place when it isn't chats (`stateView`);
- and beside it, `needs(_:shown:)`: the chats that need you that no pane shows, mentions first, then people, then groups (`needs`).

The app fills a `Situation` from jawd's pushes and asks it; the harness makes one for each state.

### What stays in JawApp

`AppModel`, `Agent`, `Launch`, `Bench`, the commands and the menus, and `MainWindowController`, which keeps the window around `WindowContent`: its frame and its autosave, the title bar's sidebar button, the sheets, the palette's panel and the display link. The app target defines no `NSView`, `NSViewController` or SwiftUI `View` of its own.

### The system's settings as values

Reduce Motion is `Motion.reduced` already, a closure the tests set; the harness sets it from its own control. Increase Contrast and Reduce Transparency become values on `Style` when Mac-M5 draws them, read from the system by the app and set by the harness. The harness never changes a system setting, and `JawUI` never reads one behind the value's back.

## The harness app

A SwiftUI app hosting AppKit, as Redlamp's and omoi's are, replacing today's `main.swift`:

```text
mac/JawHarness/Sources/
  HarnessApp.swift        the app, its window, and the menus' keys its Live scenes need (⌘K)
  HarnessScene.swift      HarnessScene, HarnessSection and the catalog
  BuiltInScenes.swift     the table of contents: one line a scene
  HarnessRootView.swift   the sidebar of sections and scenes, the controls under it, the stage
  Stage.swift             the scene on its surface, at its size, with its inspector
  Specimen.swift          SpecimenGroup, Specimen, AppKitSpecimen, ScreenSpecimen
  Knob.swift              a labelled slider for inspectors
  HarnessLaunch.swift     the launch flags
  Pictures.swift          --snapshot, --sweep and --check
  Fixtures/               the made-up account
  Scenes/                 a file a scene, or a few of one section's
  Live/                   the scripted feed, the hosts that log, the checklists
```

### Scenes

- **`HarnessScene`:** `id`; `title`; `symbol`; `synopsis`, one line on what to look at, shown under the title; `section`; `content`, built only when shown, so a scene that isn't costs nothing; an optional `inspector`; `fillsStage`, for screens and Live scenes; `covers`, the `JawUI` types it shows, as `[Any.Type]` so the compiler checks the names `check-harness` reads; `variants`, the axes the sweep draws it in; and `pinned`, its variants checked against references.
- **Variants:** dark and light for every scene; bubbles and lines for scenes that draw messages; compact where density changes what's drawn; jaw's theme only until Mac-M5 brings Redlamp's families, then any family with `--themes`.
- **`HarnessSection`, in sidebar order:** Foundations, Controls, Chat list, Conversation, Panes, Palette and overlays, States, Screens, Live, Performance.
- **`HarnessCatalog`:** built once and kept, since a scene's knobs keep their models in it, as both harnesses learned; it refuses a duplicate id. Adding a scene is a file under `Scenes/` and a line in `BuiltInScenes.swift`.

### The window

- **The sidebar:** the sections and their scenes; under them, the controls that hold as you move from scene to scene: the theme family (from `JawThemes`' catalog), dark or light, bubbles or lines, comfortable or compact, the text size (11 to 17 points), the system font or SF Mono, and Reduce Motion.
- **The stage:** the scene on the surface it ships on (the theme's ground, its surface, or the window's), its title and synopsis in the window's title, and its inspector when it has one. A screen has a size: 1180×780, the window's own; 940×700, with the sidebar folded, and 720×600, with the chat list folded too; 520×420, the least the window takes. A change of control makes the scene's AppKit views again, since they take their colours when they're made.

### Building blocks

- **`SpecimenGroup`:** a titled band of specimens with a note on what would be wrong with them, which is what turns a gallery into a review.
- **`Specimen`:** one component with a monospaced caption.
- **`AppKitSpecimen`:** an `NSView` or `NSViewController` at a width, at the height it asks for.
- **`ScreenSpecimen`:** a `WindowContent` at a fixed size, as the window would hold it, and an overlay when one is open: the palette, the glance or a sheet, drawn as views where the window puts them (the palette along the top, 620 points wide; the glance in the middle; a sheet from the top edge) rather than in panels of their own, so a picture holds them.
- **`Knob`:** a labelled slider with its value, for inspectors.

### Launch flags

| Flag | Does |
| --- | --- |
| `--scene <id>` | opens a scene |
| `--theme <id>`, `--appearance dark\|light`, `--look bubbles\|lines`, `--density comfortable\|compact`, `--text-size <11…17>`, `--font system\|mono`, `--reduce-motion` | sets the controls |
| `--window <w>x<h>` | sizes the window, in points |
| `--stage-only` | hides the sidebar and the inspector |
| `--snapshot <png>` | draws the stage into a picture at 2× by its own views, so no Screen Recording is needed, and quits |
| `--sweep <folder>` | draws every still scene in each of its variants at 2×, with `manifest.json`, and quits |
| `--check <folder>` | draws each scene's pinned variants at 1× and compares each with its reference in the folder, failing past 0.1% of pixels differing, as `LayoutTests` compares, and draws every other scene once, in its first variant, failing when one draws nothing; `--record` writes the references instead |

### Never jawd

The harness asks jawd for nothing and opens no folder of jaw's. It makes no `Jawd`, which `check-harness` checks, and its Live scenes' hosts log what the views report rather than sending it anywhere.

## Fixtures

- **Built on `Samples`** (in `JawUI`, which the snapshot tests use too) and on `protocol/fixtures`, copied into the harness's bundle and decoded as jawd sends them, so a scene shows jawd's own shapes.
- **Written longhand, with what real accounts have:** a group called "Família Gomes — Natal 2026 🎄 (não sair!)", a name in Arabic and one in Japanese, a message that's one URL with no spaces, 1,204 unread, a draft, someone typing, every tick, a reply to a deleted message, a message edited twice, an album of eight.
- **Stable ids and a fixed now** (`Samples.now`), so a picture taken today compares with last week's and nothing moves between sweeps.
- **Avatars and previews drawn for the purpose,** shapes and colours in small images in the harness's resources, with the chat list's `root` pointing at them: plainly not photos, so a review judges the frame and not the picture.

## The scenes

| Section | Scenes | What they show |
| --- | --- | --- |
| **Foundations** | Roles, Type, Metrics, Glyphs, The Mimic, App icon | Every role with its contrast ratio, and the eight senders' colours checked at 4.5:1; the type scale in SwiftUI and AppKit side by side, and messages at 11 to 17 points; spacing, radii, lines and motion; the ticks for each status, avatars and the chat chip; the Mimic awake, mid-chomp and asleep at 1× to 8×; the icon at each size |
| **Controls** | Buttons, Surfaces, Scrolling and panels | `IconButton` at each size and tone, with a title, with a menu, disabled and focused; surfaces and key caps; the thin scroller showing and fading; the floating panel at its least |
| **Chat list** | Rows, The list, Sidebar | Every row's state: unread, a mention, muted, pinned, a draft, typing, each tick of yours, a community and its groups, archived, a long name, no message yet, selected, under the pointer; the list at 260, 300 and 420 points; the sidebar |
| **Conversation** | Messages, Cards, Header, The conversation, The box | In both looks: each span, quotes, reactions, each tick, failed with Retry, edited, deleted, forwarded, starred, folded, emoji alone, system and call lines, day separators and "2 new"; photo, video, GIF, sticker, video message, voice, audio, document, location, contact, poll, link preview and albums of 2 to 5 and more; the header for a person, a group with its members and someone typing, with the pane's menu; the conversation from 360 to 1,200 points wide, empty, loading, asking the phone for older messages, with the "new ↓" pill; the box empty with its chip, typed, grown to eight lines, replying, editing, completing `@` and `:`, and refused with jawd's reason |
| **Panes** | Grids, Layout editor | 1, 2 across, 2 down, 3, 4, 6 and 12 panes, focused and dimmed in each pane look, an empty pane, the drop zones; the editor with each preset, merging, splitting, and the line saying why Save can't go |
| **Palette and overlays** | Palette, Glance, Sheets | The palette with nothing typed, with results (the counts strip, marked letters, each kind), nothing found, failed, actions, and each mode; the glance with nothing waiting, one chat, four, and twelve scrolling; message info, the keys, Settings in each section at the sheet's own size, jawd's status, and the layout prompts |
| **States** | The panes' place, Linking, Status line | Each state that fills the panes' place; the QR code, a new code crossfading in, the phone-number code, done; the status line in each connection state, with a notice, and with what needs you elsewhere |
| **Screens** | Each state, With chats, With overlays | The whole window from one value: starting, unreachable, connecting, catching up, phone offline, offline, update needed, banned, logged out and linking, no chats, no chat open; one chat in each look, three panes, focus mode, the sidebar folded at 940 points and the chat list too at 720; with the palette, the glance or Settings open over it |
| **Live** | Conversation, Palette, Panes, The box | Interactive, each with a checklist that ticks every interaction as it's tried and a log of what the views report. A scripted feed: messages arriving, ticks advancing, reactions, edits and deletes, history arriving above while nothing may jump, and a flood at a knob's rate; the palette against canned answers with a latency knob, in each mode; split, move, swap, close, and a chat dragged onto a pane; the box typed into, replying, editing, completing, sending, and a refusal putting its text back |
| **Performance** | Scrolling, A flood | 1,000 messages flung through, and 2,000 a second applied to an open conversation and the chat list, in each look: the main thread's turns at p50, p95, p99 and worst, how many pass 8.3 ms, and how busy it was, timed by a run-loop observer as Redlamp's `MainThreadMonitor` times them |

What isn't built has no scene yet: media's download states, the viewer, the library, folders and the roster, sending media and recording each get theirs as they're built, which `check-harness` makes sure of.

## The review loop

### The sweep

`mac/scripts/build-mac sweep` builds the harness on the build Mac and runs `JawHarness --sweep` there, in one run of the app, then brings the pictures back to `mac/build/review/` in the checkout it was run from. Each picture is `<scene>/<variant>.png`, and `manifest.json` gives, for each, its scene's id, section, title, synopsis and notes, its variant, its size in points and pixels, and a hash of its pixels. Each sweep replaces the last, in a folder git ignores: about 250 pictures and under 100 MB, to be measured in the first phase. Live and Performance scenes are interactive and aren't swept. When the build Mac doesn't answer, the sweep runs here, as `build-mac`'s builds do.

### The canvas

- **Where:** `.atelier/mac-harness.canvas.tsx` at the repository's root, committed with the harness. The sweep writes its data, `.atelier/mac-harness.data.ts`, from the manifest, and Atelier keeps the owner's marks beside it; git ignores both. The canvas shows the sweep of the checkout it's in: the harness's worktree until the first phase lands, `jaw-mac-m1` after, and the main checkout once `mac-m1` lands on main. Marks stay with the checkout they were made in.
- **What it shows:** a tab for each section, and in it a panel for each scene, with its title, its synopsis, its notes and its pictures by variant side by side, each opening full size. Along the top, how many scenes are right, need work, have changed since they were marked, or haven't been looked at yet, each a filter.
- **The marks:** on each scene, Right, or Needs work with a note, kept under `mark:<scene>` with the hashes of the pictures it was made on. When a sweep changes a scene's pictures, its mark shows as "changed since your mark" and the scene asks again.

### The loop

The owner marks. An agent, the Mac app's chat or any other, reads the marks with Atelier's `canvas_state`, fixes each "needs work", and sweeps again; the canvas updates in place, and what it fixed shows as changed for the owner to look at again.

### Pinned references

A small set is committed and checked in every `make mac-test`: the five conversation pictures, which stay in `Tests/JawUITests/References` and `LayoutTests` draws as now; and in `mac/Tests/References`, each state, each screen at 1180×780 and the chat list's rows, dark and light, at 1×, about 30 pictures and around 5 MB. `JawHarness --check` draws them on the build Mac, where they were recorded, and fails on a picture more than 0.1% of whose pixels differ from its reference by more than 24 levels, so an accidental change to a screen fails the tests. An intended one is recorded again with `--record` and committed with its change, as references are now.

### The checks

`scripts/check-harness` runs on every build, beside `check-components`, and fails when:

- the harness names `Jawd`, from the first phase;
- the app target defines a view or a view controller of its own, from the second;
- a public `JawUI` type that draws (declared as an `NSView` or one of its kinds, an `NSViewController`, an `NSWindow` or `NSPanel`, or a SwiftUI `View`) isn't in any scene's `covers`, from the third.

### One scene by hand

`build-mac harness [scene] [flags]` builds the harness on the build Mac, brings it back to `mac/build/harness/` and opens it here, at the scene if one is named; `build-mac shot <scene> <png> [flags]` draws one scene into a picture, for a chat or the README.

## Building it

- **Where:** the worktree `~/src/jaw-harness`, branch `mac-harness`. Before each landing it's rebased onto `mac-m1`, then fast-forwarded into it with `git merge --ff-only mac-harness` in `jaw-mac-m1`, only while `git status` there shows nothing uncommitted. A rebase that conflicts with the Mac app's chat's work keeps that work, and the harness's change is made again on top of it. When a landing moves files that chat works on, the owner is told, to tell it.
- **Builds and tests** run on the build Mac through `build-mac`, as the Mac README has them. The only scratch is the build Mac's copy of the checkout, which `build-mac` keeps under `~/src/build-mac/jaw/jaw-harness`, and the sweep's folder; both go, with the worktree, when the branch is done.
- **Nothing here touches jawd or the data layer** (`scripts/check-data-audit.py`'s paths), so no database audit is needed.

### Phases

Each is planned on its own, the next in detail once the one before has landed, and each lands into `mac-m1` when it's done.

1. **The shell and the loop.** The SwiftUI app: the catalog, the stage, the specimens, the controls, the launch flags, `--snapshot` and `--sweep`; the fixtures; today's six scenes ported; `build-mac harness`, `shot` and `sweep`; the canvas; and `check-harness` with its first rule. Done when the owner reviews the six scenes on the canvas.
2. **Screens from values.** The move into `JawUI`; `Situation` and its tests; the Screens section; `check-harness`'s second rule. Done when every screen in the table draws from a value, and the app draws as it did before the move.
3. **Every component.** The rest of the scenes; `check-harness`'s third rule; the pinned references and `--check` in `make mac-test`; the README's section. Done when `check-harness` passes with nothing left out.
4. **Live.** The scripted feed, the hosts that log, the four Live scenes and their checklists.
5. **Performance.** The run-loop observer and its two scenes.

## Testing

- **`Situation`:** every jawd state and connection state, linked or not, with and without chats and panes, against the words and the state the app shows today.
- **The move changes nothing drawn:** the app's `--snapshot --keys` pictures, taken against `jawd demo` before the move and after it, side by side and as a difference, differ only where the clock does.
- **Every scene draws:** `--check` draws every scene, so one that breaks fails `make mac-test` before a sweep finds it.
- **The checks catch what they're for:** `check-harness` is run on a copy with an uncovered view, a view in the app target and a `Jawd` in the harness, and fails on each.
- **The references:** `--check` in `make mac-test`, on the build Mac.
- **Nothing else breaks:** `JawKit`, `JawThemes` and `JawUITests` pass on the build Mac before each landing, and `check-components` with them.

## Docs

- **The Mac README** gets a "Component harness" section, as Redlamp's README has: what it's for, the sections and their scenes, the launch flags, the sweep and the canvas, and how to add a component: write a scene under `JawHarness/Sources/Scenes/` and register it in `BuiltInScenes.swift`.
- **The Mac design's** "Code" and "Testing" say what the harness is and point here, and the Mac plan's Mac-M1 step 12 does too.

## Later

- Redlamp's families in the theme control, and the sweep's `--themes`, when Mac-M5 copies them.
- A scene for each part as it's built: media's download states, the viewer, the library, folders and the roster, sending media, recording.
- The terminal beside the lines look: `jaw snapshot --demo`'s frame of a chat next to the Mac's, as Redlamp's parity scenes put a SwiftUI original next to its AppKit port.
- `--check` on a macOS runner in CI, when jaw's CI runs macOS on main.

## Risks

- **Two chats in one codebase.** The move touches files the Mac app's chat edits every hour. It comes second, while they're small; it lands in small commits, fast-forwarded only into a clean worktree; that chat's work wins a conflict; and the owner hears when a landing moves files.
- **Pictures drawn by views aren't the screen.** Views drawn by `cacheDisplay` draw materials and glass flat and Metal layers blank, as Redlamp's snapshots note. Glass, translucency and motion are judged in the app, not on the canvas.
- **References are the build Mac's.** Text is drawn differently on another Mac or macOS, so references are recorded and checked on the build Mac, as `LayoutTests`' are now. The sweep's pictures are drawn by the build Mac's macOS too, 27.0.1, where the owner's Mac runs 26.6.2; something that looks different in the app here is judged in the app.
- **The rule costs a scene a component.** A new public view needs a scene before the build passes; a scene can be one specimen, and `check-harness` names what's missing.
