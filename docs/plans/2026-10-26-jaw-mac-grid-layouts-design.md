# Jaw for the Mac: layouts on a grid

26 October 2026. A design; nothing here is built. It extends [the Mac design](2026-10-26-jaw-mac-design.md)'s "Split panels" and replaces one of its decisions, the box under each pane. The screens are drawn in the grid layouts canvas (`mac-grid-layouts-wireframes.canvas.tsx` in the project's canvases), with a playground of the splitting rules. The owner's answers are marked "Owner, 26 Oct".

## What it is

Panes today are a tree of splits: ⌘D and ⌘⇧D cut the focused pane in two. This keeps that and adds what a tree can't give: **layouts you draw on a grid**, as Moom draws windows on the screen, each named, each remembering which chat is in which slot, and switched like tabs. A layout is a place you go back to (Work, Family), not a template you refill.

There's one model under both: a layout is slots on grid lines. The ad hoc splits are edits to that grid, so a saved layout and the one you're making on the fly follow the same rules, drawn by one view.

## Decisions

- **Named layouts of a grid's slots, each keeping its shape and the chat in each slot** (Owner, 26 Oct). Switching to Work brings back Work's chats. Putting another chat in a slot is saved at once.
- **Several layouts, switched with ⌃⌘1 to ⌃⌘9, the toolbar's Layout menu and the palette** (Owner, 26 Oct).
- **One model: slots on grid lines, the splits included** (Owner, 26 Oct). The tree of splits built on mac-m1 is replaced. A tree reads as a grid, so a saved tree, and the terminal's layout when the Mac opens it, still open.
- **One message box for the window, under the slots and as wide as them** (Owner, 26 Oct). It writes to the focused slot's chat and says which. This replaces the Mac design's "one message box per conversation" (Owner, 26 Oct, earlier the same day).
- **A change to a saved layout's shape marks it edited until you save or revert it** (Owner, 26 Oct). A split, a close or a dragged line shows Work • in the toolbar. Save Layout keeps the change, and Revert Layout goes back to the saved shape. The edit survives switching layouts and quitting. Which chat a slot holds isn't part of this: it's saved at once, as the first decision says.
- **A slot drawn in the editor, or emptied, waits for a chat you choose; ⌘D fills its new slot with the next chat no slot shows** (Owner, 26 Oct), as splitting does today.
- **A grid of 6 columns by 4 rows for a new layout, set in the editor up to 12 by 8** (Owner, 26 Oct). There are 12 slots at most, whatever the grid.

## The model

`PaneGrid` replaces `PaneLayout` in `JawUI/Panes`. It's a value with its rules tested on their own, as `PaneLayout`'s are:

- `cols` and `rows`: the grid's lines as fractions of the width and the height, from 0 to 1, in order, the first 0 and the last 1.
- `slots`: each one a rectangle given as line indices, `c0 < c1` and `r0 < r1`, with an id that stays the same through edits, and the chat it holds or none.
- **What always holds:** slots don't overlap, and together they cover the grid, so there are no holes. Every interior line is the edge of some slot; a line nothing uses any more is dropped. There are at most 12 slots, and a chat shows in one slot at most.

Editing it:

- **Split across (⌘D) or down (⌘⇧D)** the focused slot. A slot already spanning several columns (or rows) splits at the line inside it nearest its middle, adding no line. Otherwise a new line goes through its middle; the other slots that line crosses span one more column and keep their size. The new slot comes after the focused one and takes the focus. The split is refused, with the terminal's words, when either half would be under 240 points ("no room to split this slot; close one, or a bigger window"), and when there are 12 slots already.
- **Close (⌘W, or the slot's ×).** A neighbour sharing the closed slot's whole edge grows into its place, the one before it in reading order first, as the terminal's does. Then the lines no slot uses are dropped. When no neighbour shares a whole edge (a pinwheel), the slot stays and goes empty. The last slot stays, and ⌘W then closes the window, as now.
- **Swap (⌃⌘ and an arrow)** exchanges the chats of the focused slot and its neighbour that way; the shape doesn't change.
- **Move (⌘⌥ and an arrow, ⌥⌘1 to 9)** goes to the slot that way, from the focused slot's middle, or to a slot by its place in reading order (top to bottom, then left to right).
- **Resize:** dragging a gutter moves a whole line, so every slot along it resizes together. No slot goes under 240 points, and the line stops there.
- **Fill or replace:** clicking a chat in the list puts it in the focused slot, or goes to the slot that already shows it. ⌘↩ in the palette puts the chat in the focused slot when that's empty, or splits across for it otherwise. Dragging a chat onto a slot's middle replaces its chat, and onto an edge splits there.

A tree reads as a grid by laying it out in the unit square: each split's position becomes a line, and each leaf becomes a slot spanning the lines around it.

## Layouts

- **The unnamed layout** is what you have before saving any: the splits, as today. Save Layout As… names it.
- **Saved layouts** are listed in the toolbar's Layout menu and in the palette, in the order you made them, the first nine with ⌃⌘1 to ⌃⌘9. The menu has Save Layout, Revert Layout, Save Layout As…, Edit Layout…, Rename and Delete.
- **Edited:** after a split, a close or a dragged line, the menu and the toolbar show Work •. The edited shape is kept apart from the saved one until Save Layout or Revert Layout.
- **Switching** lets the leaving layout's message windows go (`unwatch`) and watches the arriving one's chats, each at its first unread or the bottom, as opening a chat does. The focus goes where it was in that layout. A chat that's gone (left, deleted, or not found by jawd) leaves its slot empty, saying so in the slot.
- **Bounded:** at most 12 slots in a layout, and only the layout on screen holds windows, so it's at most 12 of jawd's 32 windows a connection, as now. There are at most 30 layouts, all of them together a few KB.

## The editor

A sheet over the window, opened by Edit Layout… or Save Layout As…:

- **Name**, with the layout's ⌃⌘ key shown.
- **Presets** to start from: Two, Three, One and two, Four, One and three. Picking one replaces the grid, keeping the chats of the slots that stay.
- **The grid:** steppers for the columns and rows, 6 by 4 for a new layout and up to 12 by 8. Drag across cells to merge them into one slot; when the rectangle dragged cuts through a slot, it grows to take all of that slot in, until it cuts through none. Each merged slot has ⊟ to split it back into cells. Taking a column or a row away removes the last one, its cells joining the slots beside them.
- **The slots,** each showing its chat or "empty", filled by dragging a chat from the list onto it. 12 at most; past 12 the editor won't merge or split further and says why.
- **Save and Cancel.**

## The box

One box for the window, under the slots and as wide as them, above the status line:

- It says "to ⟨chat⟩" for the focused slot. Enter sends, ⇧↩ makes a line, `@` and `:` complete, and files and pictures can be pasted or dropped, as the pane's box does now.
- **Drafts:** moving the focus saves the box's text as that chat's draft (`draft.set`, as now) and shows the next chat's, so nothing typed is lost.
- **Replying and editing:** r or e on a selected message in any slot moves the focus there and puts the strip over the box.
- **↑ from an empty box** selects the focused slot's newest message, as now.
- **An empty slot focused:** the box can't send and says "this slot has no chat: ⌘K to put one here".
- **Typing** (`typing`) and reading go with the focused slot, as only the focused pane reads today.

The conversation view loses its own box. `ComposerController` moves to the window, under the slots, and its chat is set by the focus.

## Storage

Layouts are kept in the app's preferences until jawd keeps an interface's state (P1), then under the key `layouts` with `ui.set`, a JSON object well under P1's 64 KB:

```json
{"v": 1, "current": "Work", "layouts": [
  {"name": "Work", "cols": [0, 0.5, 0.75, 1], "rows": [0, 0.5, 1], "focus": 1,
   "slots": [{"id": 1, "c": [0, 1], "r": [0, 2], "chat": "120363012345678901@g.us"}],
   "edited": null}
]}
```

The panes kept by mac-m1 (`panes:` and a jaw folder's path) are read once as the unnamed layout and then left alone. This is the app's own state, not jaw's data, so it doesn't touch the data layer. P1 is the data-layer change, audited where it's built.

## When things go wrong

- **A window smaller than the layout's least size:** the slots shrink below 240 points rather than hiding. Splitting is refused until there's room.
- **A chat that's gone:** its slot stays, empty, saying the chat isn't there any more, and the layout isn't marked edited.
- **A layout that doesn't decode** (a newer version's, or a damaged preference): it's skipped and named in the status line, and the others still load. The saved bytes are kept, so a newer app can read them.
- **jawd refuses a window** (the 32 limit, or an error): the slot says so, the others still work, and the status line has jawd's words.

## Testing

- **The model:** split, close, swap, move, resize and prune, including what always holds, a pinwheel, the 240-point and 12-slot limits, a tree read as a grid, and decoding version 1. These are unit tests in `JawUITests`, as `PaneLayout`'s are now.
- **The box:** moving the focus saves and swaps drafts; replying from another slot moves the focus; an empty slot can't send. These run against `FakeJawd` in `JawKitTests` and `JawUITests`.
- **The app:** `--snapshot` with `--keys` presses ⌘D, ⌘⇧D, ⌘W, ⌃⌘1 and ⌃⌘2 on the demo account and prints the layout after each. Reference pictures cover a layout of four, a pinwheel and the editor.
- **Budgets:** after ⌃⌘ and a digit, the focused slot's messages are drawn within 30 ms p99, the budget for opening a chat, and the other slots in the frames after. Measured with `--bench`.

## Build order

1. **G1, the grid under the panes:** `PaneGrid` and its view replace the tree, with today's keys and behaviour, the 240-point room check, and dragging lines. Panes saved by mac-m1 are read.
2. **G2, the box:** one box under the slots, drafts swapped as the focus moves, the conversation without its box.
3. **G3, named layouts:** the Layout menu, ⌃⌘1 to ⌃⌘9, the palette's layouts, Save, Revert, Save As, Rename, Delete, the edited state, switching with its windows.
4. **G4, the editor:** the sheet, the presets, merging and splitting cells, the grid's size.
5. **G5, dragging chats:** onto a slot's middle to replace its chat, onto an edge to split, as the Mac design already plans.
6. **Later, with P1:** layouts kept by jawd, and opening the terminal's layout.

## What this changes elsewhere

- The Mac design's decision "One message box per conversation, naming its chat" and its "Split panels" (a box under each pane; the layout as a tree of splits) are replaced by this. That document belongs to the agent that wrote it, so the change is noted here, for it to take in.
- The Mac wireframes canvas's conversation and panes drawings show a box under each pane. They're redrawn when G2 lands.
