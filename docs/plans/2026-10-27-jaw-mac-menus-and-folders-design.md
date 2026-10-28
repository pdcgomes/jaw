# Jaw for the Mac: right-click menus, folders and the roster

27 October 2026. A design; nothing here is built. It adds the right-click menus [the Mac design](2026-10-26-jaw-mac-design.md) didn't draw, brings its "Folders and the roster" forward from Mac-M4, and adds a jawd step, chat actions from jaw, to [jaw's design](2026-10-16-jaw-design.md), which the Mac design lists as its proposal P8. The screens are drawn in the menus and folders canvas (`mac-menus-folders-wireframes.canvas.tsx` in the project's canvases). The owner's answers are marked "Owner, 27 Oct".

## What it is

The owner asked for three things on 27 October: splitting and closing panes from a right click on the chat, pinning, archiving and filing chats from a right click on the chat list, and the terminal's folders on the Mac. Two of them rest on jawd:

- **Pin, archive, mute, mark read or unread.** The Mac's Chat menu and the terminal's palette already send `chat.pin`, `chat.archive`, `chat.mute` and `chat.mark_unread`, but jawd serves none of them on any branch, and no step of the plan built them; both interfaces answer "pinning comes with the next step". jawd reads what the phone sets (`convert/app_state.rs`), and its outbox already sends an app-state change for deleting a message for you through the engine's `AppStatePatch`, so setting them from jaw is a contained jawd step: "Chat actions from jaw", below.
- **Folders and the roster.** jawd serves them on the `folders` branch (M6), in its second audit now. The Mac's sidebar has All and Archived.

## Decisions

- **Built beside the Mac builder** (Owner, 27 Oct): the Mac's part on `mac-menus`, a branch on top of `mac-m1` that the Mac builder merges as each part is ready; jawd's part on `chat-actions`, from main, audited by the database expert before it lands on main.
- **Folders and the roster now, as the terminal has them, folders first** (Owner, 27 Oct): the tree, then the picked folder's people below it. This is Mac-M4's step 4 brought forward; the rest of Mac-M4 stays where it is.
- **Add to Folder ▸ everywhere; Move to ▸ and Remove from in your folders** (Owner, 27 Oct). A chat can sit in several folders, as jaw's design has them and the terminal files them, with ticks; while the list shows one of your folders, Move to files the chat in another and takes it out of this one.
- **Each action written once, for a chat** (Owner, 27 Oct). Pin, mute, archive, read, the folders, favourites and opening in a new pane are one command each, acting on the focused chat from the menu bar and the palette, and on the chat right-clicked from its menu, as the Mac's commands are already written once for the menu bar, the palette and the keys sheet.
- **A message's menu ends with the pane's splits; the header and the space around the messages open the pane's menu** (Owner, 27 Oct), as iTerm2's sessions do.
- **One outline for the folder tree, the roster below it** (Owner, 27 Oct), under a divider that can be dragged, as the terminal stacks them.
- **jawd changes the chat first and tells WhatsApp through the outbox** (Owner, 27 Oct), as deleting for you does: one synced commit with the change and its outbox row, the answer, then the patch.
- **Starring comes in jawd's step** (Owner, 27 Oct), and a message's menu gets Star.
- **Later** (Owner, 27 Oct): Delete Chat and Clear Chat, as a jawd step of their own that settles what jaw keeps; acting on several chats at once.

## The chat's menus

- **A message's menu** keeps its actions as `mac-m1` has them (Reply, React ▸, Copy, Forward…, Edit, Send Again, Delete…, Info, with the keys the conversation takes while a message is selected), gains Star or Unstar once jawd serves `star`, and ends, after a separator, with Split Across ⌘D, Split Down ⇧⌘D and Close Pane ⌘W.
- **The pane's menu,** on the header, on the space under and between the messages, and on system and call lines, which have no message menu: Split Across, Split Down, Close Pane and Swap ▸ (Left, Right, Up and Down, each with ⌃⌘ and its arrow), then the chat's actions as the chat list's menu has them, then Search This Chat ⌘F. The header's menu on `mac-m1`, which lists the splits and the swaps flat, becomes this one.
- **An empty pane's menu:** Split Across, Split Down and Close Pane.
- **The pane right-clicked takes the focus first,** as a click does, so the menu acts on it and its chat. Close Pane is greyed in the only pane, and Swap ▸ in a layout of one.

## The chat list's menu

A right click on a chat rings its row while the menu is open, without opening it; the menu acts on that chat, whichever chat the focused pane shows.

- **Open in New Pane** splits the focused pane across for it, or puts it in the focused pane when that's empty, as ⌘↩ does in the palette; a chat another pane shows is gone to instead.
- **Pin or Unpin, Mute ▸ (For 8 Hours, For a Week, Always) or Unmute, Archive or Unarchive, Mark as Read or Mark as Unread,** each saying what it will do, from the chat's row as the menu opens. Until jawd's step lands, each says in the status line that jawd can't do that yet. The times for 8 hours and a week are counted from when the item is chosen.
- **Add to Folder ▸:** your folders as a tree, indented, with a tick on each the chat sits in directly (the row's `folders`); a click files it there (`folder.add`) or takes it out (`folder.remove`). New Folder… at the end makes a folder at the top, its name typed in place in the sidebar, and files the chat in it.
- **Move to ▸ and Remove from ‹folder›,** only while the list shows one of your folders. Move to lists your other folders; it files the chat in the one picked, then takes it out of this one, so a failure between the two leaves it in both, never in neither. Remove from is greyed when the chat is in the folder only through a subfolder.
- **Favourite ▸:** the nine slots, each saying what's on it ("⌘1 Family"); picking one puts the chat there, which takes it off its old slot and puts out what the slot held, as `favourite.set` does; Remove from Favourites empties its slot.
- **A community's row** has Add to Folder, Move to, Remove from and Favourite only; its groups, shown under it once it's open, have the whole menu.
- **In All, Unread, Groups, Communities, Archived or a smart folder,** the menu is the same without Move to and Remove from.

## One command per action

`Command` stays as `mac-m1` has it. The chat's actions are written once, as a table of actions on a `ChatRow`, each with its menu title from the row's state (Pin or Unpin), its palette phrase, its key, whether it applies to the chat and what it does. The table is bound twice: to the focused pane's chat for the menu bar's Chat menu and the palette, and to one chat for a context menu. The submenus made from jawd's state (Add to Folder, Move to, Favourite) are made as the menu opens, from the folder tree and the chat's row, as the Layouts menu is made from the layout book. The palette lists each folder action by name ("Add this chat to Family", "Move this chat to Work", "Put this chat on ⌘3"), so it reaches them without a mode.

## The sidebar

As jawd serves M6 (`folders.watch`, `roster.watch`, `chats.watch`'s `folder`, `folder.*` and `favourite.set`), and as the terminal's sidebar draws it.

- **The tree** is an `NSOutlineView` on the sidebar's glass, as the Mac design has it: the built-in folders (All, Unread, Groups, Communities, Archived) without a heading; Folders, yours, nested, each opened and closed with its triangle; then Smart Folders (Waiting on me, Mentions me, Muted, unread, and any made later). Each row has its unread total (none for Archived, as in the terminal), an @ when it has mentions, and its ⌘ digit when it's a favourite. Its rows come from `folders.watch` and its pushes, laid out by `parent` and `position`.
- **Picking a folder** makes the chat list that folder's window (`chats.watch` with `folder`) and the roster its people. A folder that goes, deleted from another interface, leaves the list on All.
- **A folder's menu:** for one of yours, New Folder Inside…, Rename, Move to ▸ (the top, or a folder of yours that isn't it or under it), Delete… (asking first, saying its chats stay wherever else they're filed and its subfolders take its place) and Favourite ▸; for a built-in or smart folder, New Folder… and Favourite ▸; on the sidebar's empty space, New Folder….
- **Naming in place:** New Folder (⇧⌘N, or the menus) adds a row with its name field open where the folder will go; Return keeps it (`folder.create`), Esc drops it. Rename (Return on a selected folder of yours, or the menu) opens the same field (`folder.rename`). jawd's refusals (a name a sibling has, error 4; one that's empty, longer than 256 characters or on more than one line, -32602) are said under the field, in jawd's words, and the field stays open.
- **Dragging:** a chat from the chat list, or a person from the roster, onto one of your folders files it there (`folder.add`); a folder of yours onto another nests it, and between two rows puts it there (`folder.move` with `parent` and `position`). Built-in and smart folders take no drops, and a folder can't be dropped into itself or a folder under it. Dragging a chat onto a pane works as it does now.
- **Favourites:** ⌘1 to ⌘9 go to the slot's folder, picked in the sidebar, or its chat, opened in the focused pane; an empty slot says so in the status line. The Find menu lists them with what's on each.
- **The roster,** under the divider, kept at the height it's left: the picked folder's people (`roster.watch` with the folder, from the top, 200 at a time, growing as it scrolls), online, typing and recording first, then by when last seen, as jawd sorts them, the rest folded under "N offline". Under All they're grouped by your top folders, then the others, as in the terminal. Each row has the avatar, the name, and how they are (typing… or recording… in the online colour, online, last seen as a time ago, nothing when they hide it), faint where jawd isn't following them now (`known` false). A double click opens their chat, or starts one (`chat.start` with `person`); its menu has Open Chat or Start Chat, Open in New Pane, and the folder items. Watching it is what tells jawd whose presence to follow, within its limits.
- **m,** with a message selected, opens its chat's Add to Folder ▸ at the pane's header, as the terminal's m files the open chat.

## Keys

| Key | Does |
| --- | --- |
| ⇧⌘N | a new folder, inside the selected folder of yours or at the top, named in place |
| Return | rename the selected folder of yours |
| ⌘⌫ | delete the selected folder of yours, after asking |
| ⌘1 to ⌘9 | the favourite on that slot |
| m | with a message selected, its chat's Add to Folder ▸ |

⇧⌘N and ⌘1 to ⌘9 are free on `mac-m1`; the Mac design gave ⌘1 to ⌘9 to favourites already.

## Chat actions from jaw

jawd's step, P8 in the Mac design, on `chat-actions`.

### Requests

| Request | What jawd does | What WhatsApp is told |
| --- | --- | --- |
| `chat.pin` (`chat`, `pinned`) | Pins the chat now, or unpins it. At most three chats are pinned, as WhatsApp's apps allow; a fourth is error 5, with `data` `{"limit": "pinned", "most": 3}`. | `pin_v1`, version 5, in `regular_low` |
| `chat.archive` (`chat`, `archived`) | Archives the chat or brings it back; archiving unpins it, as WhatsApp's apps do. An archived chat leaves All and every built-in folder but Archived, and stays in your folders. | `archive`, version 3, with the newest message's range, and `pin_v1` off when it was pinned, in `regular_low` |
| `chat.mute` (`chat`, `until`) | `until` null unmutes; a time in milliseconds mutes until then; -1, or any time from the last millisecond of 9999 on, mutes for ever (the terminal sends the largest integer, the Mac -1). A time already past is -32602 with `param` `until`. | `mute`, version 2, its end in milliseconds, or -1 for ever, in `regular_high` |
| `chat.mark_unread` (`chat`, `unread`) | true marks the chat unread; false clears the mark and reads the chat up to its newest message, sending read receipts as `read` does. | `markChatAsRead`, version 3, with the newest message's range, in `regular_low` |
| `star` (`message`, `starred`, and optionally `chat`) | Stars the message or takes its star off, as the phone's stars already show in jaw. | `star`, version 2, naming the chat, the message, whether it's yours and its sender in a group, in `regular_high` |

Each answers `{}` once its change is on disk, and the row it changes comes to every interface. A chat or a message jawd doesn't have is error 3; a community's row, whose groups carry these, is error 4 for the four chat requests; an account that isn't linked is error 1.

### How jawd does it

- **One synced commit, then the answer.** The chat's new state (`pinned` as the time it was pinned, `archived`, `muted_until`, `marked_unread`) or the message's `starred`, and an outbox row of kind `app_state` holding the patch, are written in one transaction, synced, before jawd answers, with a new version for the row it changes and its feed event, so a window watched again from its `seq` is sent it.
- **Through the outbox, as deleting for you is.** The patch goes to the engine's `AppStatePatch` once it's connected, after the chat's earlier sends, retried as the outbox retries, and counted in `jawd status` with the outbox; the outbox's bound refuses a request past it with error 5, as it refuses a send.
- **Asked twice, done once.** A request for what's already so changes nothing, pushes nothing and queues nothing, so an interface asking again after a lost answer, or two interfaces asking at once, end the same.
- **The echo changes nothing.** WhatsApp sends each change back through app state, and whatsmeow passes it on; the patch carries the time jawd wrote (its `timestamp`, and the pin's time), so the converter's write finds the row as it is and leaves it alone.
- **The phone and jaw at once.** The one WhatsApp takes last wins on every device, as WhatsApp orders its own apps' changes. WhatsApp sends app state back in the order it took it, so a change from the phone to a setting that arrives before jaw's own change to it has come back came before jaw's: it's kept but not shown, since jaw's goes after it and the row already shows how every device ends. The echo of an earlier change of jaw's is held back the same way, so nothing sets the row back while jaw's latest is on its way; a change made after jaw's reached WhatsApp shows as it comes. If the phone changes the same setting at the same moment as jaw, jaw can show the other value until that setting next changes or the engine next starts, since whatsmeow doesn't say in which order WhatsApp took two changes it fetched separately.
- **Refused for good, put back as the phone has it.** A patch WhatsApp won't take, or that runs out of tries, is kept in quarantine with its error, never dropped; jawd asks the engine to fetch that app-state collection again (`AppStateResync`), so the chat ends as the phone has it, and every interface is told why.
- **The data layer.** It changes `send/`, `store/` and the converter's merge, so it follows "Data integrity" in jaw's design and is audited by the database expert before it lands. It needs one new table, `app_state_awaiting`, with an index, holding for each setting jaw has changed its latest change still on its way, and nothing else new: the chats' and messages' columns, the outbox's `app_state` kind and quarantine exist.

### Tests

Each rule above has a test that fails without it: each request's row and patch, on the demo engine and through the socket; the fourth pin; archiving unpinning; each form of mute's `until`; marking read sending receipts; the echo leaving the row's version alone; asking twice; a refusal quarantined and the collection fetched again; a crash between the commit and the answer, and one before the commit; a phone change arriving while jaw's waits. The demo engine answers `AppStatePatch` and echoes it as app state, so `jawd demo` shows the round trip. The terminal's palette actions, which send the four chat requests already, work once it lands, and on the made-up account as before.

## Until things land

- **Before jawd's step:** Pin, Mute, Archive and Mark as Read or Unread are in the menus and say in the status line that jawd can't do that yet, as the Chat menu does now. Star isn't shown.
- **Before `folders` lands:** the sidebar keeps All and Archived and the folder items aren't in the menus. The Mac's folders and roster are built and tested against the protocol's examples from the `folders` branch (its fixtures, copied into JawKit's tests until they're on main) and against `FakeJawd`, and tried on `jawd demo` once `folders` is on main and `mac-m1` has it.

## Testing

- **JawKit:** folder and roster rows, the folders and roster windows and their pushes, and each request (`folder.*`, `favourite.set`, `chat.*`, `star`) against the protocol's examples, as the fixtures are tested now; `FakeJawd` answering them.
- **JawUI:** the chat actions' table (titles from a row's state, what applies to a community, Move to's order of requests, Remove from greyed through a subfolder, the mute times counted when chosen); the tree's layout from rows (sections, nesting, counts, favourite digits); the roster's lines (grouped under All, offline folded, faint); the commands' check with the new keys.
- **The app:** `--snapshot` with `--keys` opening each menu on the demo account and printing its items; reference pictures of the sidebar with folders and the roster, in both looks, light and dark.
- **Budgets:** a menu opens in the frame after the click, made from rows the app holds, asking jawd for nothing; picking a folder draws its chats within the 30 ms of opening a chat; the sidebar adds nothing to the keypress budget during a flood.

## Build order

1. **The chat's menus** (`mac-menus`): a message's menu with the pane's three, the pane's menu on the header and around the messages, an empty pane's.
2. **The chat list's menu** (`mac-menus`): the chat actions' table bound to the focused chat and to a row, Open in New Pane, and pin, mute, archive and read saying jawd can't do them yet.
3. **Chat actions from jaw** (`chat-actions`, jawd): the five requests, the protocol's text and fixtures, the demo engine's echo, the tests and the audit; then the Mac's Star, and the menus' items do what they say.
4. **Folders** (`mac-menus`, once `folders` is on main and `mac-m1` has it): JawKit's folder rows, window and requests; the tree; picking a folder; the folder items in the chat menus; a folder's menu, naming in place and dragging; favourites on ⌘1 to ⌘9; the palette's folder actions.
5. **The roster** (`mac-menus`): JawKit's roster rows and window; the list under the tree, with its menu and dragging.

## What this changes elsewhere

- **jaw's design** gets the step, "Chat actions from jaw", after "Presence and typing in chats", and its line in the milestones.
- **The Mac design** gets P8 in its proposals, and its "Folders and the roster" points here.
- **The Mac plan:** Mac-M4's step 4, folders and the roster, comes now, with this design; Mac-M3's chat actions need P8 rather than "as jawd serves app state".
- **The protocol** gets the five requests' text when jawd's step lands, on `chat-actions`; until then its Methods table lists them as it does.
- **The board** (`ws-jaw.canvas.tsx`) is the orchestrator's: it gains the step and this work's rows when the owner tells it.

## Later

- **Delete Chat and Clear Chat** (Owner, 27 Oct): a jawd step of their own that settles what jaw keeps of a chat deleted or cleared (search, the library, the files), with its own design and audit.
- **Acting on several chats at once** (Owner, 27 Oct): ⌘-click and ⇧-click in the chat list, and one menu for all of them.
- **Making smart folders and changing their rules** (`folder.smart`), which jawd serves and neither interface draws yet.
- **Dropping a chat onto Archived** to archive it.
