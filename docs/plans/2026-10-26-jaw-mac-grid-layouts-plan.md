# Jaw for the Mac, layouts on a grid: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build [the grid layouts design](2026-10-26-jaw-mac-grid-layouts-design.md) on `mac-m1`: a component library and one list of commands under every menu, the grid under the panes, one box for the window, named layouts, the editor and dragging chats onto panes.

**Architecture:** `JawUI/Components` holds the type scale, spacing, radii, motion, and the few controls every surface uses, so nothing sets a font or draws a button by hand. `JawUI/Commands` holds `Command` and `Shortcut`: the main menu, the toolbar's menus, context menus, the palette and the keys sheet are all built from one list, so every key has a menu item and every menu item shows its key. `PaneGrid`, a value tested on its own, replaces `PaneLayout`; `GridView` lays slots out by hand in `layout()`, so they follow the window in the same frame.

**Tech stack:** Swift 6, macOS 26, AppKit for the grid, the box and the lists, SwiftUI for the editor sheet and the palette, Swift Testing, Tuist 4 through mise, builds and tests on `build-mac` (`mac/scripts/build-mac`).

## Global constraints

- Copy says **pane**, as the menus and the terminal do; "slot" is the code's and the design's word.
- At most **12 panes**; a pane is at least **240 points** across and down; the editor's grid is **6 by 4** for a new layout and at most **12 by 8**; at most **30 layouts**.
- **Every command reachable three ways:** its key, a menu (the main menu, or a context menu where it acts on a thing under the pointer), and the palette. A toolbar button where it's frequent. Tested in `CommandTests`.
- **Shift shortcuts are written with their capital** (`"D"` with `[.command, .shift]`); `Shortcut` does it, so no menu item is made by hand.
- **No font, radius, spacing or colour literal outside `JawUI/Components` and `JawThemes`**, except the message layout's own metrics in `MessageLayout`. `scripts/check-components` greps for them.
- **Responsiveness:** the grid follows a live resize in the same frame (frames set in `layout()`, no animation on resize); switching layouts draws the focused pane within 30 ms p99; animations 120–180 ms, none under Reduce Motion.
- **The owner's folder and agent are never touched:** tests and snapshots run on `jawd demo` in folders of their own; stop only PIDs you started.
- Warnings are errors; Swift 6 language mode.

## Files

```
mac/JawUI/Sources/Components/Type.swift        TextStyle: the type scale, as NSFont and SwiftUI Font
mac/JawUI/Sources/Components/Metrics.swift     Space, Radius, Stroke, Motion (Reduce Motion aware)
mac/JawUI/Sources/Components/Tone.swift        Tone: primary, secondary, tertiary, accent, warn, as Roles' colours
mac/JawUI/Sources/Components/Controls.swift    IconButton, Labels, Surface (AppKit)
mac/JawUI/Sources/Components/SwiftUI.swift     .textStyle, .tone, KeyCap, Card (SwiftUI)
mac/JawUI/Sources/Commands/Command.swift       Shortcut, Command, MenuPlace, menus and palette actions built from commands
mac/JawUI/Sources/Panes/PaneGrid.swift       the model (replaces PaneLayout's rules; PaneLayout stays as the old preferences' reader)
mac/JawUI/Sources/Panes/GridView.swift         slots laid out by hand, gutters, dragging lines, the focus outline, the empty pane
mac/JawUI/Sources/Panes/LayoutBook.swift       named layouts, the edited state, version 1 JSON
mac/JawUI/Sources/Panes/LayoutEditor.swift     the editor sheet (SwiftUI) and its model
mac/JawApp/Sources/Commands.swift              the app's commands, grouped by menu
mac/JawApp/Sources/AppModel.swift              panes on PaneGrid, the book, the box's focus
mac/JawApp/Sources/MainWindowController.swift  the box under the grid, the Layout toolbar menu
mac/JawApp/Sources/AppDelegate.swift           the main menu from the commands
mac/scripts/check-components                   the literal check
mac/Tests/JawUITests/PaneGridTests.swift, LayoutBookTests.swift, CommandTests.swift, ComponentTests.swift
```

---

### Task 1: The component library

**Files:** create `Components/Type.swift`, `Metrics.swift`, `Tone.swift`, `Controls.swift`, `SwiftUI.swift`, `scripts/check-components`, `Tests/JawUITests/ComponentTests.swift`; modify every file the check names.

**Produces:**
- `public enum TextStyle { case title, heading, body, callout, caption, micro, mono, key }` with `nsFont: NSFont` and `font: Font`: title 17 semibold, heading 15 semibold, body 13, callout 12, caption 11, micro 10, mono 11 monospaced, key 11 semibold monospaced.
- `public enum Space { static let xxs = 2.0, xs = 4.0, s = 6.0, m = 8.0, l = 12.0, xl = 16.0, xxl = 24.0 }`, `Radius { small = 4, control = 8, field = 10, panel = 14 }`, `Stroke { hairline = 1, focus = 1.5 }`, `Motion { quick = 0.12, standard = 0.18; static func run(_ duration:, _ changes:) }` running `changes` without animation under Reduce Motion.
- `public enum Tone { case primary, secondary, tertiary, accent, warn }` with `func color(_ roles: Roles) -> RGBA`.
- `IconButton(symbol:label:shortcut:size:)` (borderless, tinted by tone, hover fill, tooltip "label (⌘W)", accessibility label), `Labels.make(_:style:tone:roles:)`, `Surface(radius:fill:border:)` with `roles` to restyle.
- SwiftUI: `.textStyle(_:)`, `.tone(_:_:)`, `KeyCap(shortcut)`, `.card(roles:selected:)`.

- [ ] Write `ComponentTests`: every `TextStyle`'s size and weight as listed; `Tone.color` gives `roles.secondary` for `.secondary`; `IconButton`'s tooltip is "Close Pane (⌘W)" for that shortcut; `Motion.run` applies the change at once when told motion is reduced.
- [ ] Run `mac/scripts/build-mac test JawUITests`: fails, the types don't exist.
- [ ] Write the five files.
- [ ] Move every surface onto them: `PaneFrame`'s close button, the composer's strip close, field and completions, the message hover bar, the header's labels, the chat list's text attributes, the sidebar's rows, the palette's, glance's, linking's, keys' and state views' fonts.
- [ ] Write `scripts/check-components`: fails on `systemFont(ofSize`, `.font(.system(`, `cornerRadius =`, `borderWidth =` outside `Components/`, `Layout/MessageLayout.swift` and `Layout/Fonts.swift`; add it to `scripts/check`.
- [ ] Run the tests and the check; commit.

### Task 2: One list of commands

**Files:** create `JawUI/Sources/Commands/Command.swift`, `JawApp/Sources/Commands.swift`, `Tests/JawUITests/CommandTests.swift`; modify `AppDelegate.swift` (menus), `AppModel.swift` (`actions()` from commands), `MainWindowController.swift` (toolbar menus), `Views.swift` (`KeysView` from commands).

**Produces:**

```swift
public struct Shortcut: Hashable, Sendable {
    public let key: String            // "d", "↩", an arrow's function-key character
    public let modifiers: NSEvent.ModifierFlags
    public var display: String        // "⌃⌥⇧⌘D", in Apple's order
    public var menuKey: String        // the capital when Shift is held and the key is a letter
}
public enum MenuPlace: String, CaseIterable, Sendable { case app, file, edit, find, panes, layouts, chat, view, window, help }
@MainActor public final class Command {
    public let id: String, title: String, detail: String, symbol: String?, shortcut: Shortcut?, place: MenuPlace?, group: Int
    public var enabled: () -> Bool
    public let run: () -> Void
    public func menuItem() -> NSMenuItem      // target and validation by a CommandTarget it keeps
    public var paletteAction: PaletteAction
}
public enum Menus { @MainActor public static func build(_ commands: [Command]) -> NSMenu }
```

- [ ] Write `CommandTests`: `Shortcut("d", [.command, .shift]).menuKey == "D"` and `.display == "⇧⌘D"`; `menuItem()` carries `menuKey` and modifiers; a disabled command's item validates false; `Menus.build` puts commands in their places, in groups split by separators; no two commands share a shortcut (on the app's list, through a `Commands.all(for:)` seam fed a stub model); every command with a shortcut has a place.
- [ ] Run: fails.
- [ ] Write `Command.swift`; write `JawApp/Commands.swift` with every command the menus, palette and toolbar have today, each once; build the main menu, the Split toolbar menu, the palette's actions and the keys sheet from it; delete the hand-made lists.
- [ ] Run the tests; press ⌘D, ⌘⇧D, ⌘W, ⌘K through `--keys` on the build Mac; commit.

### Task 3: The grid model

**Files:** create `Panes/PaneGrid.swift`, `Tests/JawUITests/PaneGridTests.swift`; trim `PaneLayout.swift` to the enum, `Codable` and `frames(in:)`; drop its rule tests.

**Produces:** `PaneGrid` (`cols`, `rows`, `slots: [Slot]` with `id`, `c0`, `c1`, `r0`, `r1`, `chat`), `Axis`, `Direction`, `Refusal { most, room; said }`, `single(_:)`, `reading`, `slot(_:)`, `slot(showing:)`, `frames(in:)`, `splitting(_:_:chat:size:) -> Result<(PaneGrid, Int), Refusal>`, `closing(_:) -> (PaneGrid, Int?)`, `with(chat:in:)`, `swapping(_:_:)`, `neighbour(of:_:)`, `lines`, `moving(_:to:size:)`, `from(_ tree: PaneLayout)`, `shape`, `holds`, and for the editor `onCells(columns:rows:)`, `merging(_:)`, `intoCells(_:)`, `pruned()`.

- [ ] Write `PaneGridTests`: splitting a single pane across and down; splitting a slot that spans columns uses the line inside it and adds none; the slots a new line crosses span one more column and keep their frames; refusals for 12 panes and under 240 points; closing gives the space to the neighbour sharing the whole edge, the one before first, and drops unused lines; a pinwheel's slot goes empty instead; `with(chat:in:)` takes the chat out of any other slot; swapping exchanges chats only; neighbours by direction; moving a line clamps at 240 points; a tree reads as the same frames; `onCells` snaps lines to the cells and keeps them in order; merging grows to the slots it cuts and keeps the first chat; `intoCells` refuses past 12; `holds` after every operation; `Codable` round trip.
- [ ] Run: fails.
- [ ] Write `PaneGrid.swift`.
- [ ] Run; commit.

### Task 4: The grid under the panes

**Files:** create `Panes/GridView.swift`; delete `PaneView.swift`; modify `AppModel.swift` (`grid: PaneGrid?`, `focusedSlot: Int?`, `focusedChat` from it; open, split, close, move, swap, resize, step, restore and save on the grid, the old `panes:` preference read once), `MainWindowController.swift` (`DetailController` on `GridView`), `Bench.swift` (`--keys` prints the grid).

**Produces:** `GridView` with `show(_ grid:, views: [Int: NSView], focused: Int?, roles:)`, `moved: ((PaneGrid.Line, Double) -> Void)?`, `close: ((Int) -> Void)?`, `pick: ((Int) -> Void)?`; slots placed in `layout()` from `grid.frames(in: bounds)`, a 1-point gutter in `roles.separator`, a resize cursor over each line's segments, dragging a line live; the focus outline as the slot's own border; an empty pane drawn with `Surface` and "⌘K or drag a chat here".

- [ ] Run `--keys "cmd+d,cmd+shift+d,cmd+w"` on the build Mac and read the grid printed after each; resize the window by `setContentSize` in the snapshot and check frames.
- [ ] Commit.

### Task 5: One box for the window

**Files:** modify `ConversationController.swift` (no box in its view; `composer` stays its own, placed by the window), `MainWindowController.swift` (a box place under the grid, as wide as it; the focused chat's composer shown there; an empty pane's box saying "this pane has no chat: ⌘K to put one here"), `AppModel.swift` (on focus, the leaving composer's draft sent and the arriving one shown and given the keys).

- [ ] Write a `JawUITests` test: a conversation's view holds no composer; `ComposerController.leave()` sends its draft through the delegate.
- [ ] Run, implement, run.
- [ ] On the build Mac: `--keys "cmd+d,cmd+opt+left"` then the snapshot shows one box naming the focused chat.
- [ ] Commit.

### Task 6: Named layouts

**Files:** create `Panes/LayoutBook.swift`, `Tests/JawUITests/LayoutBookTests.swift`; modify `AppModel.swift`, `Commands.swift` (the Layouts menu: each layout with ⌃⌘1 to ⌃⌘9, Save Layout ⌥⌘S, Revert Layout, Save Layout As… ⇧⌥⌘S, Edit Layout… ⌥⌘E, Rename…, Delete), `MainWindowController.swift` (the toolbar's Layout menu, titled with the layout's name and • when edited).

**Produces:**

```swift
public struct LayoutBook: Codable, Hashable, Sendable {
    public struct Saved: Codable, Hashable, Sendable { public var name: String; public var grid: PaneGrid; public var edited: PaneGrid?; public var focus: Int? }
    public var v = 1
    public var current: String?            // nil: the unnamed layout
    public var unnamed: PaneGrid?
    public var layouts: [Saved]
    public var grid: PaneGrid?           // what's on screen: the edited shape, or the saved one, or the unnamed
    public var isEdited: Bool
    public mutating func reshape(_ grid: PaneGrid)   // a split, a close, a moved line: edited when the shape differs
    public mutating func rechat(_ grid: PaneGrid)    // a chat put in or taken out: saved at once
    public mutating func save(); revert(); saveAs(_ name: String) throws; rename(_ old: String, to: String) throws; delete(_ name: String); switchTo(_ name: String?)
    public static let most = 30
}
```

- [ ] Write `LayoutBookTests`: a shape change marks edited and a return to the saved shape clears it; a chat change is saved at once to the saved grid and the edited one; save, revert, save-as (names unique, 30 at most), rename, delete of the current goes to the unnamed; switching keeps each layout's edit; version 1 JSON decodes, and an unknown version is refused.
- [ ] Run, implement, run.
- [ ] Wire switching in `AppModel`: unwatch the leaving panes, watch the arriving ones, focus where it was; a chat jawd doesn't have leaves its pane empty saying so.
- [ ] `--keys "cmd+d,cmd+opt+shift+s"` can't type a name, so the snapshot takes `--layout <name>` to save as and switch; check ⌃⌘1 and ⌃⌘2.
- [ ] Commit.

### Task 7: The editor

**Files:** create `Panes/LayoutEditor.swift`; modify `MainWindowController.swift` (shown as a sheet), `Commands.swift`.

**Produces:** `LayoutEditorModel` (`@Observable`: `grid`, `columns`, `rows`, `cursor`, `anchor`, `name`, `said`) and `LayoutEditorView`. **Keys:** arrows move the cell cursor, ⇧ and arrows grow the selection, M merges it, S splits the pane under the cursor into cells, 1 to 5 pick a preset, ⌘+ and ⌘− (⇧ for rows) change the grid's size, ⏎ saves, Esc cancels. **Mouse:** drag across cells to merge, ⊟ on a merged pane, presets clicked, steppers, Save and Cancel buttons.

- [ ] Write tests on `LayoutEditorModel`: the cursor and selection move within the grid; merging and splitting change the grid as `PaneGrid`'s do; presets; the grid's size can't go under what the layout needs; Save is refused with a reason when the name is empty or taken.
- [ ] Run, implement, run; snapshot the sheet on the build Mac; commit.

### Task 8: Dragging chats onto panes

**Files:** modify `ChatListController.swift` (rows as drag sources of the chat's id), `GridView.swift` (drop target: the middle replaces the pane's chat, an edge splits there, shown while dragging), `AppModel.swift`.

- [ ] Write a `GridView` test: the drop zone for a point (middle, each edge, outside).
- [ ] Run, implement, run; commit.

### Task 9: Land it

- [ ] All suites on the build Mac; `make mac`; copy `Jaw.app` to the MacBook; update the README's pane section and the grid layouts canvas's statuses; commit.

## Self-review

- Every design section maps to a task: the model (3), the grid on screen and its keys (4), the box (5), layouts, the edited state, switching and storage (6), the editor (7), dragging (8), what goes wrong (4: room and small windows; 6: missing chats and bad JSON), testing (each task), build order (tasks 3 to 8, after the two the owner asked for today, 1 and 2).
- P1 storage stays later, as the design says.
