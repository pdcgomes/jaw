# The layout book kept by jawd (parity step 1.1, second half)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep jaw's Mac app's whole book of layouts in jawd, as P10's format 2, the second half of [the parity plan](2026-10-27-jaw-mac-parity-plan.md)'s step 1.1, with jawd the one store, as the terminal keeps its panes (the owner, 29 October): asked for with `layout.get` at every hello, kept with `layout.set` once a change has settled and at once on quitting, and followed when another of the app's connections keeps a newer one (the `layout` push). The preferences' book moves into jawd once, the first nine named layouts of a larger one moving and an alert naming the rest, and moves again only when jawd keeps none after the move. It's never written again but for a launch in which jawd refuses the move in its own words, which leaves the preferences keeping the layouts as before P10, harmlessly, since the whole book opens and what's written back is that book. What jawd didn't confirm keeping, when the app quit or while jawd was away, is held in one small value and moved in at the next launch. With a jawd that keeps no books, as one from before P10 doesn't, the app keeps its layouts in its preferences as before P10, nine named at most for new ones.

**Architecture:** JawKit asks for the app's own book in the format it was kept in, keeps one at a version, reads both answers and the `layout` push into Part A's `KeptLayout`, a book kept whole as JSON, reads what stops one as `BookRefusal`, and at quitting waits a second at most and gives back what jawd said of the book. JawUI's `LayoutBook` writes and reads format 2, holds nine named layouts besides the unnamed one, and says which of an older, larger book move to jawd. A new value in JawUI, `LayoutKeeper`, decides where the layouts are kept and when they go: it takes what jawd says and the app's changes and answers with steps (open this book, keep this one at this version, hold this one on this Mac, name the layouts that didn't move, say this), which `AppModel` carries out, so the policy is tested on its own and the app only wires it. Another, `LayoutPreferences`, reads the preferences' book and writes the one value this Mac holds (`LayoutHeld`, in format 1: the book jawd didn't confirm, its version, and whether the alert has named the layouts left behind), never over one it can't read.

**Tech Stack:** Swift 6 (AppKit, Swift Testing), Tuist, xcodebuild on the build Mac through `mac/scripts/build-mac`, POSIX `sh`; Python 3 for `make check-parity` and for asking jawd what it keeps in the live check.

## Global Constraints

- The spec is the second half ("With P10") of the parity plan's step 1.1 and its "Done when", with its "P10, a layout book" section (`docs/plans/2026-10-27-jaw-mac-parity-plan.md`); the "Layouts" section of `protocol/README.md` and its five book fixtures, on main since P10 landed; the grid layouts design's "Layouts", "Storage" and "When things go wrong" (`docs/plans/2026-10-26-jaw-mac-grid-layouts-design.md`); the owner's choice of ten layouts (28 October); and his choice of jawd as the one store (29 October), quoted in the Decisions. Where this plan and they differ, they win: stop and say so.
- This is Part B, built on `mac-panes` after the six tasks of Part A, [the first half's plan](2026-10-28-jaw-mac-panes-kept-by-jawd-plan.md), in its worktree, `/Users/pedrogomes/src/jaw-panes`; work only there. Before Task 1, check that Part A, the harness's second phase and P10 are in: `grep -n "func lookUp" mac/JawKit/Sources/Client/Jawd.swift`, `grep -n "struct KeptLayout" mac/JawKit/Sources/Protocol/Layouts.swift`, `grep -n "var unopened" mac/JawApp/Sources/AppModel.swift` and `grep -n "enum Prompts" mac/JawUI/Sources/Window/Prompts.swift` must each find it, `grep -c '= "1.1"' mac/parity.toml` must print `2` (`layout.set`'s row and the `layout` push's), and `git merge-base --is-ancestor 817bc02 HEAD && echo P10` must print `P10`. If the last doesn't, rebase onto origin/main as Part A's constraints do (`git fetch origin && git rebase origin/main`); if any other doesn't, stop and say so.
- P10, jawd's half, is on main since 817bc02, its audit approved (`docs/audits/2026-10-28-layout-book.md`): its "Layouts" text is d8fe2b6 ("The protocol's Layouts in both formats"), with 1d378f5's change to a round trip between formats ("close, not exact"), and its book fixtures 6f65045 ("Books' fixtures and exact lines"). Before Task 1, and again before the final report, read `git log --oneline 817bc02..origin/main -- protocol/README.md protocol/fixtures/`; if a commit there changes the "Layouts" section or a `layout.*` fixture, stop and say what changed.
- `mac-folders` may land on main before this branch does or after it. Its first task adds an `Ahead` folder and fixture helpers to `FixtureTests.swift`, and its Task 14 removes them; this plan copies no fixtures, since P10's are on main, and adds and removes nothing of `Ahead`'s. Both branches add cases in the same places: the fixture test's push list and its `want` switch, `IncomingLine`'s switch of pushes, `JawUpdate`, and the last case of `AppModel.apply(_:)`. Whichever lands second has conflicts there, resolved by keeping both sides' cases, after which its tests and `make check-parity` must pass again.
- Never push, never pass `--no-verify`, never rewrite commits that aren't yours. Never edit, build in, check out in, or run a git command that changes anything in another agent's worktree or in the owner's checkout, `/Users/pedrogomes/src/jaw`; only read them.
- Build and test on the build Mac with `mac/scripts/build-mac`: `test JawKit`, `test JawUITests`, `test JawHarnessTests`, `build` (the `jaw` app) and `snapshot`. `mac/scripts/check` exits 1 when a build or the tests fail, so both a run's exit status and its printed `** BUILD SUCCEEDED **` / `** TEST SUCCEEDED **` and `Test run with …` lines count. For the whole log: `ssh -o BatchMode=yes build-mac 'tail -150 ~/src/build-mac/jaw/jaw-panes/mac/build/check.log'`. Before each run, check that the build Mac answers: `ssh -o BatchMode=yes -o ConnectTimeout=5 build-mac true`. If it doesn't, or `build-mac` prints `doesn't answer; running here` and falls back to building on this Mac, stop (interrupt the run you started) and ask the controller. Never build or test on this Mac.
- JawKit's live tests, `--demo` and the live check need `target/debug/jawd` in this worktree, which `build-mac` copies to the build Mac. Copy the Mac builder's, as it is: `mkdir -p target/debug && cp /Users/pedrogomes/src/jaw-mac-m1/target/debug/jawd target/debug/jawd` (about 48 MB, ignored by git). Never build Rust, check anything out or change anything in that checkout. If its jawd is missing or older than that checkout's HEAD commit, or that HEAD doesn't contain P10 (817bc02) or isn't on origin/main, stop and ask the controller, who keeps it current; main moving on past that HEAD is fine. This prints `current` when none of those is so:

  ```sh
  m1=/Users/pedrogomes/src/jaw-mac-m1
  test -x "$m1/target/debug/jawd" && git -C "$m1" merge-base --is-ancestor 817bc02 HEAD \
      && git -C "$m1" merge-base --is-ancestor HEAD origin/main \
      && [ "$(stat -f %m "$m1/target/debug/jawd")" -ge "$(git -C "$m1" log -1 --format=%ct HEAD)" ] && echo current
  ```

  That jawd is main's, which keeps books since P10, so the live test and the live check expect jawd to keep the book. If either finds it refused, the jawd copied is from before P10: stop and ask the controller. Never run the Go engine: use only `jawd --engine demo` and `jawd demo`, each on a folder of its own. Never stop a process by name; stop only one whose PID you started.
- Swift 6 language mode, macOS 26, warnings are errors. `mac/scripts/check-components` refuses fonts, radii and line widths set by hand outside `JawUI/Sources/Components`. `mac/scripts/check-harness` refuses a view or a view controller in the app target, and `Jawd` or `Client` named in the harness.
- The code goes in JawKit's `Protocol/Layouts.swift`, `Client/Incoming.swift`, `Client/Client.swift` and `Client/Jawd.swift`; JawUI's `Panes` (`LayoutBook`, `PaneGrid`, `LayoutEditor` and the new `LayoutKeeper` and `LayoutPreferences`) and the harness's `Window/Prompts.swift`; and the app's `AppModel` (`start()`, `stop()`, `apply(_:)`, `moved(_:to:done:)` and the "Keeping the panes" section), `MainWindowController`'s Layouts extension, `Bench.swift`'s snapshot and its steps, `Launch.swift` and `main.swift`. Line numbers are main's, the same since fa9265c, only in files neither Part A nor mac-harness changes (`LayoutBook.swift`, `PaneGrid.swift`, `LayoutEditor.swift`, `Launch.swift`, `main.swift`); elsewhere find each place by the symbol named.
- Ten layouts (the owner, 28 October): the unnamed one and nine named, the ones ⌃⌘0 to ⌃⌘9 reach. Names are 64 bytes at most, as jawd keeps them.
- The app's preferences are never deleted. The book (`layouts:` and the jaw folder's path as it was typed) and the older `grid:` and `panes:` keys are read, and the book is written only while the preferences keep the layouts for a launch (`LayoutKeeper.Place.preferences`): with a jawd that keeps no books, before jawd has kept the layouts on that folder, or one that refused in its own words the book moved to it. Once jawd has kept the layouts, that last happens only to the move made again when jawd keeps none, and is harmless: the whole old book opens, and what's written back is it, with the launch's changes. This plan adds one key, `layouts.held:` and the folder's standardized path, one value written whole in one write (`LayoutHeld`, in format 1). A value there that this Jaw can't read, in another format, with a key it doesn't know, or damaged, is never written over. A snapshot (`--snapshot`) reads and writes none of them, so its pictures don't depend on what an earlier run left, but for preferences of the check's own that `--layouts-in` names (Task 5); `--bench` neither asks jawd for the book nor keeps one.
- No audit: nothing here is on jaw's data paths (`scripts/check-data-audit.py` names none under `mac/`), and the app's preferences are its own state. jawd's half, P10, landed with its own audit.
- Scratch goes in three places: the worktree's ignored `mac/build/book/`, the build Mac's copy of the checkout, and on the build Mac the live check's one `mktemp` folder under its `TMPDIR`, with preferences of their own named after that folder, which its script removes, folder and preferences, on its way out. The tests' folders (`/tmp/jawk-…`, `/tmp/jawl-…`) and preferences (`jaw-mac-tests.…`) are their own, and the tests remove them. Before the final report, run `rm -rf mac/build/book` and `make prune-builds`. Once the branch has landed and its worktree is removed, its copy on the build Mac goes too: `ssh -o BatchMode=yes build-mac 'rm -rf ~/src/build-mac/jaw/jaw-panes'`.
- Every commit passes `make check-parity`: Task 1 takes `layout.set`'s row and the `layout` push's out of `mac/parity.toml` in the commit where JawKit's sources first send one and read the other.
- Commit messages are one long sentence in the project's voice (read `git log -5`), with no prefixes. Code matches its surroundings: doc comments in plain sentences, British spelling, no comments that narrate a change, 4-space indentation, and trailing commas in multi-line argument lists.

## Decisions

- **jawd is the one store** (the owner, 29 October): "jawd is the one store, as the terminal does it: the preferences are moved in once and never written again; a change goes to jawd once it settles (a crash within its two seconds loses it, as in the terminal); a keep jawd didn't confirm at quit waits in one small key and is moved in at the next launch; past nine, the first nine move and an alert names the rest, which stay in the old preferences an earlier Jaw opens, with no Bring Back." Why: three reviews of the plan that kept the layouts in jawd and in the preferences as a fallback, the layouts past nine merged back and brought back by hand, each found new ways to lose a layout, all from keeping them in two places.
- **A change is kept nowhere for its first two seconds**, so a crash or a force quit within them loses it, as in the terminal, where before P10 every change went to the preferences at once; the owner accepted this. A change waits until the book has stayed as it is for two seconds (the terminal's `KEEP_AFTER`, `crates/jaw/src/view/panes/kept.rs`), only the newest is sent, one `layout.set` unanswered at a time, and its version is the time in milliseconds, or one past the newest jawd has told of. Every change reaches the keeper, a line still being dragged included (`moved(_:to:done:)` hands it on whatever `done` says), so the keep comes two seconds after the last move and jawd's answer to an earlier keep can't put a line back under the pointer; with a jawd that keeps no books, the preferences are written once the drag ends, as main writes them. Nothing is kept before the app has opened the book it asked for (`LayoutKeeper.opened()`): a keep then would put an empty or half-opened book over jawd's.
- **A change made while jawd is away is held on this Mac** (decided on 29 October, within the owner's one small key): a keep the connection lost before jawd answered it is held at once, and each change made while jawd is away once it has stayed as it is for two seconds, in the one held value, at a version past any jawd told of. So a crash or a force quit then loses only the last two seconds, as when jawd is there, rather than every change since jawd went. A keep lost so may still have been kept, since jawd can commit a book and go before it answers, so from then on the app doesn't count on the book jawd last confirmed: going back to it is held like any other change, quitting holds what's on screen, and once jawd is back the book on screen is kept, whatever jawd answers. What's held goes once jawd keeps it or a newer book, or once what's on screen is what jawd keeps again, as jawd last said.
- **Moving the preferences' book.** The first time jawd keeps nothing for the app (`v` 0) and this Mac holds no book, the preferences' book, or what mac-m1 kept as the unnamed layout, is asked to keep: the unnamed layout and the first nine named layouts whose names jawd keeps (64 bytes at most), the ones ⌃⌘1 to ⌃⌘9 reached unless a name was too long (`LayoutBook.movable`). That's the owner's "the first nine move" made exact: jawd refuses a whole book holding a name longer than 64 bytes, so a layout under one can't move, and the nine that move are the first nine it can take, one with a longer name staying with those past the nine, the alert saying why. It opens once jawd has kept it; if the layout on screen is one of those left, the unsaved panes come on screen in its place (question 1). The rest stay in the preferences, which this Jaw doesn't write, so an earlier Jaw still opens them, and an alert over the window, not the status line's four seconds, names them once, with why. That the alert has said them is kept in the held value; which they are isn't, since the preferences' book doesn't change after the move: the keeper works them out from it at whichever launch first finds jawd keeping a book with the alert unsaid. They're worked out from the preferences' book, not from what jawd took, so a first launch that finds jawd already keeping a book another Jaw kept, one with preferences of its own or one given the folder's path spelt another way (the older keys are as typed), names only those past the nine, or none, though the first nine stayed in the preferences too. The held value's key has the folder's path standardized, so what's held, and that the alert has said, are found however the path is spelt. So a move whose answer is lost, to a dropped connection, a quit or a crash, or that another connection's newer book beats, deletes nothing and loses no alert: jawd's book is asked for again at the next hello, or after a second, the move is made again only if jawd still keeps nothing, and the alert comes once jawd keeps a book. A move refused by a jawd that keeps no books, or in jawd's words, leaves the preferences keeping the layouts for that launch, the whole book open, as before P10; jawd's words are said in an alert at each launch they're true, since each finds the layouts not in jawd, and the next launch moves them again.
- **The move made again** (the owner, 29 October). When jawd keeps no layouts at all for the app once the alert has said (`said` in the held value, which only a launch that found jawd keeping a book sets), as when jawd's jaw.db is new, or restored from before the move, and nothing is held, the launch moves the preferences' book again, the same nine, and says in an alert that they came back: any deleted since are back too, and nothing is lost. With nothing in the preferences to bring back, the status line says that jawd keeps none. Refused in jawd's words, it leaves the preferences keeping the layouts for that launch, as the first move does, the whole old book open, so their book is written after jawd had kept the layouts, the one way it ever is; that's harmless, since what's written back is that book, with the launch's changes.
- **An earlier Jaw's edits after the move.** An earlier Jaw still opens the preferences' book and may change it. This Jaw never writes over those changes, and never takes them either: the move happens once, and after it jawd's book is the layouts. That's the owner's trade-off, and nothing here is built for it.
- **A jawd that keeps no books.** A jawd from before P10 (main before 817bc02) reads no `format`, so it answers `layout.get` for the app with nothing kept (`v` 0), as P10's does for a new folder; only `layout.set`'s answer tells them apart. It reads the layout `layout.set` is given as a tree before it looks at its format, so it refuses a book with -32602, ``layout.set's params: missing field `focused` ``, and no `data` (`crates/jawd/src/layout.rs` at 8591529, `params_of` before `checked`), where its protocol text promised `data` `{"param": "layout.format", "allowed": [1]}`. Both, and -32601 from a jawd before P1, read as `BookRefusal.unknown`. Until jawd has kept the layouts on a folder, that has the app keep them in its preferences for the rest of the launch, as before P10, saying nothing, but for the nine-layout limit: Save Layout As… stops at nine, though a book from before with more still opens whole. The next launch asks again. Such a refusal in the second the app waits as it quits has the preferences keep what's on screen rather than this Mac holding it; and a held book it refuses, which a launch held before it found out whether jawd keeps books (its first keep lost with the connection, or unanswered at quitting), opens, and the preferences keep it from their first write, when the held value goes. When the preferences hold a book this Jaw can't read, a newer Jaw's or a damaged one, which is never written over, they can't keep the layouts either: the layouts are kept nowhere for the rest of the launch, which an alert says, as it says a book jawd keeps that this Jaw can't read. That's the fallback for an older jawd, not a second store beside jawd's: after the move, a jawd from before P10 can't open the jaw.db P10 migrated, since migration 17 is P10's and `migrate` in `crates/jawd/src/store/schema.rs` refuses a jaw.db from a newer jawd, so on one folder no jawd that keeps no books has the preferences' book written again. A jawd that keeps no books after this Jaw has kept the layouts in jawd, swapped while the app runs or refusing the held book once the alert has said, can't then happen on one folder; the keeper keeps a branch for it only as a defence, reading it as a refusal in jawd's words, which never writes the preferences' book, said once a launch rather than at every change.
- **Quitting, and the held value.** When the app quits, what's on screen is kept at once if it isn't what jawd keeps, and the app waits a second at most for jawd's answer before it closes the connection: jawd's lane for a connection stops carrying out its requests once the connection has ended, which a reply that can't be sent ends (`lane` and `one` in `crates/jawd/src/sock.rs`). What jawd didn't confirm keeping in that second, unanswered, refused, or not sent with jawd away, is held on this Mac in one value per jaw folder, `layouts.held:` (`LayoutHeld`: the book; the version it was asked to keep at, or one past any jawd told of when it wasn't sent, so a clock gone back doesn't matter; and whether the alert has said), written whole in one write, the same value a change made while jawd is away is held in. It has a format, 1, as the grid layouts design keeps bytes it can't decode for a newer app: a value in another format, or with a key this Jaw doesn't know, is a newer Jaw's, read as one this Jaw can't read and never written over, so a newer Jaw can add to it. The next launch moves it into jawd when jawd's book is older, and lets it go once jawd keeps it or a newer book. Nothing is held when jawd told of a newer book meanwhile, which wins, nor when that book is what's on screen (another connection's, taken while a keep of the app's own was on its way), since a copy here would be newer than that connection's next keep might be; nor while jawd keeps a book this Jaw hasn't read as it was kept or can't read, since a newer Jaw reading the value might move it over that book. What's on screen otherwise is held, the keep on its way at its own version only while jawd has told of nothing newer since, and anything else at a version past any jawd told of: the screen back on the book on its way after a newer one was told of, or back on the app's own older book that a newer one, told of while a change waited, never replaced.
- **The held book refused in jawd's words.** When the next launch moves the held book and jawd refuses it in its words, the book opens, stays held, and jawd's words are said; the next change, quitting and the next launch try jawd again, and it goes only once jawd keeps it or a newer book. Why: letting it go would lose what was on screen at the last quit, and nothing may go without the app saying so, which here would mean saying it's gone; held, nothing is lost, and the cost is a refusal said again while jawd keeps refusing.
- **Reading.** The app asks for its book at every hello, since a dropped connection misses pushes, and asks with no `format`, so jawd answers in the format the book was kept in (`answered` in `crates/jawd/src/layout.rs`): format 2 for a book a Jaw kept, and anything else as it is, which this Jaw reads as a book it can't read. Asked for format 2, a jawd converts what it keeps ("each format read as the other"), so a later jawd could hand over a newer Jaw's book converted, which this Jaw would open and write over; a jawd from before P10 answers the same either way. For the same reason a `layout.set` answer with `kept` false, whose book is "in the format of the layout asked for", isn't taken: the app asks `layout.get` again, and keeps and holds nothing until it has jawd's book as it was kept. That needs two Jaws on one folder, and while the connection stands the other's push or jawd's answer comes within milliseconds; if the connection drops first, it lasts until jawd is back, every change meanwhile kept nowhere and a quit holding nothing, the app saying only that it can't reach jawd, since the book it hasn't read may be a newer Jaw's. Of the books jawd tells of, the answers and the `layout` push, whichever order they come in, the app takes one only if its version is newer than any it knows, and opens it as switching does; a change of the app's own still waiting wins, and goes after it with a newer version, and an answer jawd read back before another connection's newer one was kept, and pushed first, changes nothing. Newest is by when a book was sent, not when its change was made: between two Jaws on one folder, a change that waited through a disconnect goes when jawd is back and writes over a book the other kept meanwhile, though that one was made later. A book taken from jawd becomes the one the app knows jawd keeps, so taking it sends nothing back but what opening it changes: with the focus on no slot, which jawd keeps as it was given, `reconcile` gives the keys to the first slot, and that book is kept two seconds later, once. An answer to `layout.get` older than a book jawd confirmed keeping says jawd's book went back, as when its jaw.db is restored or linked again: the book jawd keeps now becomes the one the app knows it keeps, so what's on screen is kept again at once, though nothing changed, unless a keep is on its way or a change waits, which goes anyway; one this Jaw can't read is left as it is. A `layout.get` jawd refuses, in its words or by losing it, while the app waits for it, at launch, with a change waiting for jawd to be back, or while jawd keeps a book this Jaw hasn't read, is asked again after a second, doubling to thirty, and at the next hello, jawd's words said once; nothing opens or is kept meanwhile.
- **A book jawd keeps that this Jaw can't read** is a newer Jaw's, as the grid layouts design's "When things go wrong" has a layout that doesn't decode: it's never written over, the unsaved panes open empty, and nothing is kept or held for the rest of the launch, which an alert says, at launch before anything is made; the same goes for one another connection keeps, pushed, and one jawd's book goes back to. Why: keeping in jawd would replace it, holding on this Mac could have a newer Jaw move the held book over it, and the preferences, moved once, aren't written.
- **Snapshots.** A snapshot keeps the layouts it makes in jawd, which the live check needs, so it runs only on a folder of its own: one on jaw's own folder (`Launch.platformHome`, which a release build's snapshot opens when given no folder, only a debug build falling back to the demo's) or on a folder with an engine's session in it, as `jawd demo` refuses for its folder, is refused before it connects (`Launch.refusal`), since a book a snapshot kept in a jawd that kept none for the app would stop the owner's preferences ever moving. A snapshot reads and holds nothing in the preferences, so it moves nothing, but with `--layouts-in`, taken only with `--snapshot` and only for preferences whose name starts `dev.badcoffee.jaw.check.`, as the live check seeds.
- **Gone chats.** jawd empties a slot whose chat is gone, or that shows a chat a slot before it in the same shape shows (a person's phone JID and LID, say), and lists its JID in `dropped` as the app named it, saying neither which slot held it nor which of the two it was. After its own `layout.set` the app can tell which slots jawd emptied, since jawd keeps each slot's id, but not why: a person shown by phone in one slot and by LID in a later one, their chat kept under the phone, comes back as a chat that went does, the LID in `dropped` and its slot empty, and nothing else the app has tells the two apart, since the chat list's rows (`ChatRow`) carry no other JIDs and Part A's lookup finds the LID gone. So it doesn't guess: it takes jawd's book as jawd keeps it (`LayoutBook.following`), each chat as jawd knows it now, the keys and the lines staying where they are, and a slot jawd emptied waits like any empty slot ("⌘K or drag a chat here"), the status line saying how many panes jawd emptied and that each one's chat went or a pane before it shows that chat, the layout neither changed nor marked edited. A chat that goes while it's on screen still says "this chat isn't here any more" (`GridView.gone`, from Part A's lookup) until the next keep, after which its slot waits too. At launch jawd's book opens as it is, the status line says the same, and the book is kept once more as it opened, so jawd forgets those chats and the next launch doesn't say so again. What jawd would need to say is which slot it emptied and why, gone or shown by a slot before it, and which: the proposal beside question 2.
- **Open the Terminal's Panes** keeps asking with no format, which gets the terminal's tree as it was kept, and `TerminalPanes` lays a tree out as jawd's format 2 does (P10's text: "A tree as a grid, as the Mac app's `PaneGrid.from` reads one"). Only a round trip between formats is close rather than exact (1d378f5), and the Mac makes none.

## What P10's text says that the first half's closing section didn't

The closing section had P10 on `layout-book-2`; it's on main since 817bc02, with its "Layouts" text d8fe2b6, its book fixtures 6f65045 and 1d378f5's change. Against that text:

- The two rows leave `mac/parity.toml` in Task 1, not in the last task: `make check-parity` reads the Mac's sources, and Task 1 is where JawKit first writes `ask("layout.set"` and `case "layout"`.
- A book has limits besides its ten layouts and its 12 slots a shape: each name 1 to 64 bytes and the book's only one by it, a `current` the book names, every shape a grid as `PaneGrid.holds` has one, and 64 KiB. The Mac's names had no length limit, so Task 2 gives them jawd's, and a book from before with a longer name leaves that layout behind rather than have jawd refuse the whole book.
- `layout.get`'s `v` 0 doesn't say jawd keeps books: a jawd from before P10 answers it too, reading no `format`. Only `layout.set`'s answer tells.
- `layout.get` answers in the format a layout was kept in unless it names one, when jawd converts; `layout.set` answers, kept or not, in the format of the layout it asked to keep; the push comes in the format kept. So only a `layout.get` with no format and the push give the app a book as it was kept, which is what says whether this Jaw can read it.
- `KeptLayout` can't stay format 1's alone: `layout.get`'s and `layout.set`'s answers and the push share one envelope whatever the format, so Task 1 gives it the book, whole, beside the tree.
- jawd empties the slot that shows a chat a slot before it shows "top to bottom and then left to right", the grid's reading order, and a slot whose chat is gone, and lists the JIDs of both in `dropped` as the interface named them, "each once", saying neither which slot each was in nor which of the two it was. It answers each chat as it knows it now, "never a chat being merged away" included since `merge-chains` landed on main (22dc4c7), which fixed the audit's X1, a slot on the first chat of a chain of merges reading as the chat in the middle.
- A tree read as a grid leaves out a pane whose share rounds to nothing, its JID in `dropped`, and a round trip between formats brings panes back in their places to within a few thousandths of the window rather than exactly (1d378f5).
- A terminal tree of 13 to 16 panes reads as a grid jawd won't keep as a book (the audit's O3). Part A's `TerminalPanes` refuses more than 12, so the Mac never asks it to.
- **Part A's plan still puts P10 on `layout-book-2` in two places of its Task 6**, which the controller amends before either half is built (`docs/plans/2026-10-28-jaw-mac-panes-kept-by-jawd-plan.md` on `mac-panes`). This plan's Task 6 replaces the parity plan's whole "With P10" bullet, so the parity plan ends right either way, but Part A's commit would otherwise carry a sentence and a message that are no longer true.
  - Task 6 Step 2's last paragraph, which begins ``At the end of the "With P10" bullet, add: `P10's protocol text and fixtures are on layout-book-2;``, becomes:

    ```markdown
    At the end of the "With P10" bullet, add: `P10 is on main since 817bc02, its protocol text and fixtures with it; this half has a plan of its own, [the layout book plan](2026-10-28-jaw-mac-layout-book-kept-by-jawd-plan.md), built after the first half once the harness's second phase is on main.` Leave "Where it starts" as it is, since it says where the plan started.
    ```

  - Task 6 Step 4's commit message becomes:

    ```text
    Parity step 1.1's first half is done: the parity plan says what Open the Terminal's Panes does and how a pane whose chat the chat list doesn't hold gets its row or says why it can't, and that the second half, with P10 on main since 817bc02, has a plan of its own, the layout book plan, built after the first once the harness's second phase is on main; the Mac README says how the terminal's tree is read, how a pane's chat is looked up and what a slot whose chat won't have a pane says; make check-parity passes, as it has since layout.get's row left mac/parity.toml
    ```

  - Its closing section, which this section answers, can say so too: its heading `## Part B, with P10: its own plan, written next` becomes `## Part B, with P10: its own plan`, and its first paragraph, up to the list, becomes:

    ```markdown
    The step's second half has no code here. P10 is on main since 817bc02, its audit approved (`docs/audits/2026-10-28-layout-book.md`): its "Layouts" section in both formats (d8fe2b6, with 1d378f5's change to a round trip between formats) and the books' fixtures, `protocol/fixtures/layout.*.book.json` (6f65045). Part B's own plan, [the layout book plan](2026-10-28-jaw-mac-layout-book-kept-by-jawd-plan.md), is built against those fixtures and `FakeJawd` after Part A, once the harness's second phase is on main, and tries it end to end on `jawd demo`; the owner tries it on his account. This is what that text settles, as this plan read it before P10 landed; where the layout book plan says otherwise, it's right:
    ```

## Questions for the owner

Neither blocks a task; the plan builds each lean, and each is a small change in the one place named.

1. **The layout on screen, when it's past the nine that move.** Nothing brings a layout left behind back now, so the question is what this Jaw shows of it. A: the unsaved panes come on screen, as they were, and the alert names it among those left, which an earlier Jaw still opens whole (lean: nothing that moves is replaced). B: its panes come on screen as the unsaved panes, replacing what they held, as Delete Layout leaves a deleted layout's panes, so what was on screen still is, though not as a named layout, and the unsaved panes it replaced stay only in the preferences. `LayoutBook.movable` builds A.
2. **Saying which slot jawd emptied, and why.** jawd's answers can't tell the app a chat that went from a person a slot before shows under their other JID, so every slot jawd empties waits for a chat, at launch and after a keep, and the status line says how many (the Decisions' "Gone chats"). A: leave it so (lean: P10 has just landed, and both are rare). B: the proposal below, after which a slot whose chat went says "this chat isn't here any more", at launch too, and only a slot that repeated a chat waits. `LayoutBook.following` and `LayoutKeeper.took` build A, and Task 6 marks the grid layouts design's two bullets on gone chats as waiting on this answer.

**For jawd's builders, with question 2: say which slots jawd emptied, and why.** An optional `emptied` beside `dropped`, in `layout.get`'s and `layout.set`'s answers and in the `layout` push: for each slot jawd empties, the layout's name (none for the unsaved panes), the shape (`grid` or `edited`), the slot's id, and why, `gone`, or `shown` with the id of the slot before it that shows the chat as jawd knows it now. An interface that doesn't read it reads `dropped` as now, so it needs no new protocol version. With it the Mac puts a gone chat's JID back in its slot, which then says the chat isn't here any more, and leaves a slot emptied for repeating a chat waiting, as now.

## File structure

New:

- `mac/Tests/JawKitTests/BookTests.swift`: the app's book against a fake jawd, answering as P10's and as one from before it.
- `mac/JawUI/Sources/Panes/LayoutKeeper.swift`: where the layouts are kept and when they go to jawd (`LayoutKeeper`), what this Mac holds of them (`LayoutHeld`), and the alert's words.
- `mac/Tests/JawUITests/LayoutKeeperTests.swift`: the keeper's steps, and the alert's words.
- `mac/JawUI/Sources/Panes/LayoutPreferences.swift`: what this Mac keeps of the layouts in its preferences.
- `mac/Tests/JawUITests/LayoutPreferencesTests.swift`: those preferences, each test in preferences of its own.

Changed:

- `mac/JawKit/Sources/Protocol/Layouts.swift` (Part A's): `LayoutSet`, and `KeptLayout`'s `book` and `kept`.
- `mac/JawKit/Sources/Client/Incoming.swift` and `Client.swift`: the `layout` push, typed.
- `mac/JawKit/Sources/Client/Jawd.swift`: `book()`, `keep(_:v:)`, `waitForKeeping(seconds:)`, `BookRefusal`, and `JawUpdate`'s `book`, `bookPushed` and `bookRefused`.
- `mac/JawUI/Sources/Panes/LayoutBook.swift`: nine named layouts, names of 64 bytes, format 2 (`forJawd`, `init(fromJawd:)`), `isEmpty`, `movable` and `following(_:)`.
- `mac/JawUI/Sources/Panes/PaneGrid.swift`: `following(_:)`.
- `mac/JawUI/Sources/Panes/LayoutEditor.swift`: `notReady` says both limits.
- `mac/JawUI/Sources/Window/Prompts.swift` (the harness's second phase's): `layouts(_:_:)`.
- `mac/JawApp/Sources/AppModel.swift`: the keeper and the preferences, wired (Task 4), and `kept(_:)` with `--layouts-in` (Task 5); `MainWindowController.swift`: `tellLayouts(_:_:)`; `Launch.swift` and `main.swift`: a snapshot refused on an account's folder (Task 4) and `--layouts-in` (Task 5); `Bench.swift`: a snapshot that waits for the layouts and says what it held as it quits (Task 4), and the `wait:` step and the panes lines' `alert` and `layouts` (Task 5).
- `mac/Tests/JawKitTests/FixtureTests.swift`, `LiveTests.swift`; `mac/Tests/JawUITests/LayoutBookTests.swift`, `LayoutEditorTests.swift`.
- `mac/parity.toml` (Task 1); `mac/README.md`, `docs/plans/2026-10-27-jaw-mac-parity-plan.md` and `docs/plans/2026-10-26-jaw-mac-grid-layouts-design.md` (Task 6).

The tests this adds: 7 in JawKit (Task 1) and 25 in JawUITests (4 in Task 2, 18 in Task 3 and 3 in Task 4).

---

### Task 1: The app's book in JawKit: asked for, kept, pushed, and what stops it

**Files:**
- Create: `mac/Tests/JawKitTests/BookTests.swift`
- Modify: `mac/JawKit/Sources/Protocol/Layouts.swift` (Part A's `KeptLayout`); `mac/JawKit/Sources/Client/Incoming.swift` (`Incoming`, the pushes in `IncomingLine.init(from:)`); `mac/JawKit/Sources/Client/Client.swift` (`take(_:_:)`'s pushes, `Event`); `mac/JawKit/Sources/Client/Jawd.swift` (after `layout(of:what:)`, `take(_:_:)`, `answered(_:_:_:)`, `Asked`, `refused(_:_:)`, before and in `JawUpdate`); `mac/JawApp/Sources/AppModel.swift` (`apply(_:)`'s last case); `mac/parity.toml` (two rows)
- Test: `mac/Tests/JawKitTests/FixtureTests.swift`, `mac/Tests/JawKitTests/LiveTests.swift`

**Interfaces:**
- Consumes, from Part A: `LayoutGet(interface:)`, `KeptLayout(interface:v:layout:dropped:)`, `SplitLayout`, `Expect.layout`, `Reply.layout(KeptLayout)`, `Jawd.Asked.layout(String)`, `JawUpdate.layout(KeptLayout)`.
- Produces, in JawKit:
  - `struct LayoutSet: Encodable, Sendable { let v: UInt64; let layout: JSONValue }`, internal.
  - `KeptLayout.book: JSONValue?` (a layout in format 2, whole) and `KeptLayout.kept: Bool?` (`layout.set`'s answer), and `public init(interface: String, v: UInt64, layout: SplitLayout? = nil, book: JSONValue? = nil, kept: Bool? = nil, dropped: [String] = [])`.
  - `Jawd.book()`, which asks `layout.get` with no params; `Jawd.keep(_ book: JSONValue, v: UInt64)`; and `Jawd.waitForKeeping(seconds: Double) -> [JawUpdate]`, which returns once no book asked to keep waits for its answer, or the time is up, with what came of the app's book meanwhile: `.book`, `.bookPushed` and `.bookRefused`.
  - `public enum BookRefusal: Sendable, Hashable { case unknown, unanswered, refused(String) }`, with `public init(_ failure: Failure)`.
  - `JawUpdate.book(KeptLayout)`, `JawUpdate.bookPushed(KeptLayout)` and `JawUpdate.bookRefused(BookRefusal, keeping: Bool)`.
- The app lets the three new updates go until Task 4.

- [ ] **Step 1: Check the book fixtures are there**

P10's book fixtures are on main since 817bc02, so this plan copies none. Run, from the worktree's root:

```bash
git merge-base --is-ancestor 817bc02 HEAD && ls protocol/fixtures/layout.*.book.json | wc -l
```

Expected: `5`. If the first command fails, the branch is on a main from before P10: rebase onto origin/main as the Global Constraints say, then run it again.

- [ ] **Step 2: Write the failing tests**

In `mac/Tests/JawKitTests/FixtureTests.swift`, make four changes.

First, in `every result and push fixture decodes`, read every fixture, a book's among them, by the method its name begins with. Its line

```swift
        let method = name.components(separatedBy: ".").dropLast(2).joined(separator: ".")
```

becomes

```swift
        let parts = name.components(separatedBy: ".")
        let method = parts.firstIndex { $0 == "result" || $0 == "push" }.map { parts[..<$0].joined(separator: ".") } ?? ""
```

so `layout.set.result.book.json` is read as `layout.set`'s answer. In its `want` switch, Part A's `case "layout.get": .reply(.layout)` becomes:

```swift
        case "layout.get", "layout.set": .reply(.layout)
```

and the pushes that must not decode untyped gain the `layout` push:

```swift
        case let .push(method, _) where ["chats", "messages", "resync", "connection", "notice", "link", "layout"].contains(method):
```

Second, in Part A's `the panes jawd keeps decode as a tree, with the chats of the panes it left out`, these two lines

```swift
    let book = try JSONDecoder().decode(KeptLayout.self, from: Data(#"{"interface":"jaw-mac","v":5,"layout":{"format":2,"layouts":[]}}"#.utf8))
    #expect(book.v == 5 && book.layout == nil, "panes in a format other than 1's tree are none this Jaw reads")
```

become

```swift
    let book = try JSONDecoder().decode(KeptLayout.self, from: Data(#"{"interface":"jaw-mac","v":5,"layout":{"format":2,"layouts":[]}}"#.utf8))
    #expect(book.v == 5 && book.layout == nil && book.book == .object(["format": .number(2), "layouts": .array([])]),
            "a book is kept whole, for JawUI to read")
    let newer = try JSONDecoder().decode(KeptLayout.self, from: Data(#"{"interface":"jaw-mac","v":6,"layout":{"format":3}}"#.utf8))
    #expect(newer.v == 6 && newer.layout == nil && newer.book == nil, "a format other than 1's and 2's is none this Jaw reads")
```

Third, after that test, add:

```swift
@Test func `a book comes whole as jawd answers and pushes it, with whether the one asked to keep is kept`() throws {
    guard case let .reply(_, .layout(terminal)) = try incoming("layout.get.result.book.json", .reply(.layout)) else {
        Issue.record("not layout.get's answer")
        return
    }
    #expect(terminal.interface == "jaw" && terminal.v == 1_792_000_000_123 && terminal.layout == nil)
    #expect(terminal.book?["unnamedFocus"] == .number(2) && terminal.dropped == ["351919999999@s.whatsapp.net"],
            "the terminal's tree, read as a book")
    guard case let .reply(_, .layout(kept)) = try incoming("layout.set.result.book.json", .reply(.layout)) else {
        Issue.record("not layout.set's answer")
        return
    }
    let answered = try JSONValue.parse(fixture("layout.set.result.book.json"))["result"]?["layout"]
    #expect(kept.kept == true && kept.v == 1_792_000_000_789 && kept.dropped == ["351919999999@s.whatsapp.net"])
    #expect(kept.book == answered && kept.book?["current"] == .string("Work"))
    guard case let .layout(pushed) = try incoming("layout.push.book.json", .nothing) else {
        Issue.record("not a layout push")
        return
    }
    #expect(pushed.interface == "jaw-mac" && pushed.v == 1_792_000_000_999 && pushed.kept == nil && pushed.book?["format"] == .number(2))
    guard case let .layout(tree) = try incoming("layout.push.json", .nothing) else {
        Issue.record("not a layout push")
        return
    }
    #expect(tree.layout?.tree == .chat("351912345678@s.whatsapp.net") && tree.book == nil, "the terminal's push, a tree")
}
```

Fourth, in `each request JawKit writes equals its fixture`, add these after Part A's `layout.get.request.json` line. JawKit never asks `layout.get` for a format, so `layout.get.request.book.json` has no request of its to equal:

```swift
    let book = try #require(try JSONValue.parse(fixture("layout.set.request.book.json"))["params"]?["layout"])
    try same("layout.set.request.book.json", written(id: 32, "layout.set", LayoutSet(v: 1_792_000_000_789, layout: book)))
```

Create `mac/Tests/JawKitTests/BookTests.swift`:

```swift
import Foundation
@testable import JawKit
import Testing

/// The app's book of layouts against a fake jawd: asked for in the format it was kept in and kept
/// at a version, as P10's jawd answers; what a jawd from before P10 answers, which keeps no books;
/// what stops a book, as the app reads it; another connection's book, pushed; and quitting, which
/// waits a while for jawd's answer to the last book and gives back what came.

/// A script answering the methods given with their lines, `{id}` the request's, and anything else
/// as `ordinary` does.
func answering(_ lines: [String: String]) -> FakeJawd.Script {
    { method, params, id in
        lines[method].map { [$0.replacingOccurrences(of: "{id}", with: "\(id)")] } ?? ordinary(method, params, id)
    }
}

/// A fixture's line, its id `{id}`, for `answering`.
func fixtureLine(_ name: String, id: Int) throws -> String {
    try String(decoding: fixture(name), as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        .replacingOccurrences(of: "\"id\":\(id)", with: "\"id\":{id}")
}

/// The book the protocol's fixture asks jawd to keep.
func askedBook() throws -> JSONValue {
    try JSONValue.parse(fixture("layout.set.request.book.json"))["params"]?["layout"] ?? .null
}

func books(_ all: [JawUpdate]) -> [KeptLayout] {
    all.compactMap { if case let .book(kept) = $0 { kept } else { nil } }
}

func refusals(_ all: [JawUpdate]) -> [(BookRefusal, Bool)] {
    all.compactMap { if case let .bookRefused(why, keeping) = $0 { (why, keeping) } else { nil } }
}

/// What a jawd answers `layout.get` for an interface it keeps nothing for, whichever jawd it is.
let nothingKept = #"{"id":{id},"result":{"interface":"jaw-mac-test","v":0}}"#

@MainActor
@Suite(.serialized) struct BookTests {
    @Test func `the book is asked for as it was kept, and kept at its version, as P10's jawd answers`() async throws {
        let keeps = try fixtureLine("layout.set.result.book.json", id: 32)
        let fake = try FakeJawd(script: answering(["layout.get": nothingKept, "layout.set": keeps]))
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.book()
        let asked = await drain(jawd) { !books($0).isEmpty }
        #expect(books(asked) == [KeptLayout(interface: "jaw-mac-test", v: 0)], "nothing kept yet")
        #expect(requests(fake, "layout.get") == [.object([:])], "the connection's own, in the format it was kept in")

        let book = try askedBook()
        jawd.keep(book, v: 1_792_000_000_789)
        let answered = await drain(jawd) { !books($0).isEmpty }
        let kept = try #require(books(answered).first)
        #expect(requests(fake, "layout.set") == [.object(["v": .number(1_792_000_000_789), "layout": book])])
        #expect(kept.kept == true && kept.v == 1_792_000_000_789 && kept.dropped == ["351919999999@s.whatsapp.net"])
        let expected = try JSONValue.parse(fixture("layout.set.result.book.json"))["result"]?["layout"]
        #expect(kept.book == expected, "the book as jawd keeps it, each chat as it knows it now")
    }

    @Test func `a jawd from before P10 keeps no books: it answers that it keeps none, and refuses one as it reads a tree`() async throws {
        let fake = try FakeJawd(script: answering([
            "layout.get": nothingKept,
            "layout.set": #"{"id":{id},"error":{"code":-32602,"message":"layout.set's params: missing field `focused`"}}"#,
        ]))
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.book()
        let asked = await drain(jawd) { !books($0).isEmpty }
        #expect(books(asked) == [KeptLayout(interface: "jaw-mac-test", v: 0)], "it reads no format, so it answers as for a tree")
        let book = try askedBook()
        jawd.keep(book, v: 5)
        let refused = refusals(await drain(jawd) { !refusals($0).isEmpty })
        #expect(refused.count == 1 && refused.first?.0 == .unknown && refused.first?.1 == true)
    }

    @Test func `what stops a book reads as no books kept, no answer, or jawd's own words`() {
        func reading(_ code: Int, _ message: String, _ data: JSONValue? = nil) -> BookRefusal {
            BookRefusal(.jawd(JawError(code: code, message: message, data: data)))
        }
        #expect(reading(ErrorCode.invalidParams, "layout.set's params: missing field `focused`") == .unknown, "a jawd from before P10")
        #expect(reading(ErrorCode.invalidParams, "jawd keeps layouts in format 1, and this one is in format 2",
                        .object(["param": .string("layout.format"), "allowed": .array([.number(1)])])) == .unknown,
                "the protocol text from before P10")
        #expect(reading(ErrorCode.unknownMethod, "jawd doesn't know layout.set") == .unknown, "a jawd from before P1")
        let limit = "a book holds 10 layouts at most, its unnamed one and 9 named, and this one names 10"
        #expect(reading(ErrorCode.limit, limit) == .refused(limit))
        let name = #"a layout's name is 1 to 64 bytes, and "x" isn't"#
        #expect(reading(ErrorCode.invalidParams, name, .object(["param": .string("layout.layouts")])) == .refused(name))
        #expect(BookRefusal(.lost) == .unanswered && BookRefusal(.unreachable) == .unanswered && BookRefusal(.full(256)) == .unanswered)
        let jawd = Jawd(socket: "/tmp/jawk-unused/jaw.sock", client: "jaw-mac-test", version: "0", focus: false) { _ in }
        #expect(refusals(jawd.refused(.keep, .lost)).first.map { $0.0 == .unanswered && $0.1 } == true)
        let unknown = jawd.refused(.book, .jawd(JawError(code: ErrorCode.unknownMethod, message: "?")))
        #expect(refusals(unknown).first.map { $0.0 == .unknown && !$0.1 } == true)
    }

    @Test func `another connection's book comes as the layout push says it`() async throws {
        let fake = try FakeJawd(script: ordinary)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        let push = try String(decoding: fixture("layout.push.book.json"), as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        fake.send(push)
        let all = await drain(jawd) { all in all.contains { if case .bookPushed = $0 { true } else { false } } }
        let pushed = all.compactMap { if case let .bookPushed(kept) = $0 { kept } else { nil } }.first
        #expect(pushed?.interface == "jaw-mac" && pushed?.v == 1_792_000_000_999 && pushed?.kept == nil)
        #expect(pushed?.book?["unnamedFocus"] == .number(1) && pushed?.book?["layouts"] == .array([]))
    }

    @Test func `quitting waits for jawd's answer to the last book, but not past its time, and gives back what came`() async throws {
        let words = "jawd couldn't keep the layout; what failed is kept in quarantine"
        let fake = try FakeJawd { method, params, id in
            guard method == "layout.set" else { return ordinary(method, params, id) }
            switch params["v"]?.int {
            case 5?: return ["{\"id\":\(id),\"result\":{\"interface\":\"jaw-mac-test\",\"v\":5,\"kept\":true}}"]
            case 6?: return ["{\"id\":\(id),\"error\":{\"code\":-32603,\"message\":\"\(words)\"}}"]
            default: return []
            }
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        let book = try askedBook()
        jawd.keep(book, v: 5)
        let start = Date()
        let kept = books(jawd.waitForKeeping(seconds: 2))
        #expect(kept.map(\.v) == [5] && kept.first?.kept == true && Date().timeIntervalSince(start) < 1, "jawd answered, keeping it")
        jawd.keep(book, v: 6)
        let refused = refusals(jawd.waitForKeeping(seconds: 2))
        #expect(refused.count == 1 && refused.first?.0 == .refused(words) && refused.first?.1 == true, "jawd refused it")
        jawd.keep(book, v: 7)
        let again = Date()
        let none = jawd.waitForKeeping(seconds: 0.3)
        let waited = Date().timeIntervalSince(again)
        #expect(none.isEmpty && waited >= 0.3 && waited < 1, "jawd didn't answer, so it gave up at its time")
        let away = Jawd(socket: "/tmp/jawk-unused/jaw.sock", client: "jaw-mac-test", version: "0", focus: false) { _ in }
        away.keep(book, v: 5)
        let unsent = refusals(away.waitForKeeping(seconds: 0.3))
        #expect(unsent.count == 1 && unsent.first?.0 == .unanswered, "no connection, so the book never went")
    }
}
```

In `mac/Tests/JawKitTests/LiveTests.swift`, add this at the end of the `LiveTests` suite. It runs against the jawd copied from main, which keeps books since P10:

```swift
    @Test func `a real jawd keeps the app's book as it was given, and gives it back`() async throws {
        let demo = try DemoJawd()
        defer { demo.stop() }
        let held = Held(demo.socket)
        defer { held.jawd.close() }
        let ana = "351912345678@s.whatsapp.net"
        #expect(await held.until { h in h.chats.list.contains { $0.id == ana } }, "the demo's chats come")
        held.jawd.book()
        #expect(await held.until { h in h.updates.contains { if case .book = $0 { true } else { false } } })
        let first = held.updates.compactMap { if case let .book(kept) = $0 { kept } else { nil } }.first
        #expect(first?.v == 0 && first?.book == nil, "a new folder keeps nothing for the app")
        let book = try JSONValue.parse(Data(#"""
        {"format":2,"unnamed":{"cols":[0,1],"rows":[0,1],"slots":[{"id":1,"c0":0,"c1":1,"r0":0,"r1":1,"chat":"\#(ana)"}]},"unnamedFocus":1,"layouts":[]}
        """#.utf8))
        held.updates.removeAll()
        held.jawd.keep(book, v: 5)
        #expect(await held.until { h in h.updates.contains { if case .book = $0 { true } else if case .bookRefused = $0 { true } else { false } } })
        let kept = held.updates.compactMap { if case let .book(kept) = $0 { kept } else { nil } }.first
        #expect(kept?.kept == true && kept?.v == 5 && kept?.book == book,
                "kept as it was given; a jawd from before P10 refuses it, which means the jawd copied is stale")
        held.updates.removeAll()
        held.jawd.book()
        #expect(await held.until { h in h.updates.contains { if case .book = $0 { true } else { false } } })
        let again = held.updates.compactMap { if case let .book(kept) = $0 { kept } else { nil } }.first
        #expect(again?.v == 5 && again?.book == book && again?.kept == nil, "and given back")
    }
```

- [ ] **Step 3: Run them to see them fail**

Run: `mac/scripts/build-mac test JawKit`
Expected: exit 1, with the build failing on `cannot find 'LayoutSet' in scope`, `value of type 'Jawd' has no member 'book'` and `cannot find 'BookRefusal' in scope`.

- [ ] **Step 4: The protocol's types**

In `mac/JawKit/Sources/Protocol/Layouts.swift`, after Part A's `LayoutGet`, add:

```swift
/// `layout.set`'s params: the layout to keep, as jawd keeps it, and its version, which the
/// interface gives: newer than any jawd has told it of.
struct LayoutSet: Encodable, Sendable {
    let v: UInt64
    let layout: JSONValue
}
```

and `KeptLayout`, from its doc comment to the end of its `Keys`, becomes:

```swift
/// What jawd keeps of an interface's layout, as `layout.get` and `layout.set` answer and the
/// `layout` push says: the interface, the version kept (0 for none), and the layout, unless none is
/// kept, none is left or it's in a format this Jaw doesn't read: a tree (format 1) as `layout`, or
/// a book (format 2) as `book`, kept whole for JawUI's `LayoutBook` to read; for `layout.set`,
/// whether the layout asked for is the one kept; and the chats of the panes left out or the slots
/// emptied, which are gone or shown by one before them.
public struct KeptLayout: Decodable, Sendable, Hashable {
    public var interface: String
    public var v: UInt64
    public var layout: SplitLayout?
    public var book: JSONValue?
    public var kept: Bool?
    public var dropped: [String]

    public init(interface: String, v: UInt64, layout: SplitLayout? = nil, book: JSONValue? = nil, kept: Bool? = nil,
                dropped: [String] = [])
    {
        self.interface = interface
        self.v = v
        self.layout = layout
        self.book = book
        self.kept = kept
        self.dropped = dropped
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        interface = try c.decode(String.self, forKey: .interface)
        v = try c.decode(UInt64.self, forKey: .v)
        kept = try c.decodeIfPresent(Bool.self, forKey: .kept)
        dropped = try c.decodeIfPresent([String].self, forKey: .dropped) ?? []
        switch try c.decodeIfPresent(Format.self, forKey: .layout)?.format {
        case 1?:
            layout = try c.decode(SplitLayout.self, forKey: .layout)
            book = nil
        case 2?:
            layout = nil
            book = try c.decode(JSONValue.self, forKey: .layout)
        default:
            layout = nil
            book = nil
        }
    }

    /// A layout's format alone, read before the rest: format 1's tree is read whole, failing the
    /// answer if it's malformed; format 2's book is kept whole for JawUI to read; any other format
    /// is one this Jaw doesn't read.
    struct Format: Decodable {
        let format: Int
    }

    enum Keys: String, CodingKey {
        case interface, v, layout, kept, dropped
    }
}
```

- [ ] **Step 5: The push, typed**

In `mac/JawKit/Sources/Client/Incoming.swift`, add this case to `Incoming`, after `case push(String, JSONValue)`:

```swift
    /// A layout another of the interface's connections kept: the `layout` push.
    case layout(KeptLayout)
```

In `IncomingLine.init(from:)`, the pushes' switch gains this case, after `case "notice"`:

```swift
            case "layout": value = try .layout(c.decode(KeptLayout.self, forKey: .params))
```

In `mac/JawKit/Sources/Client/Client.swift`, add this case to `Event`, before `/// Any other push.`:

```swift
    /// A layout another of the interface's connections kept: the `layout` push.
    case layout(KeptLayout)
```

and in `take(_:_:)`, after the `.push` case:

```swift
        case let .layout(kept):
            return event(.layout(kept), generation, urgent: false)
```

- [ ] **Step 6: Ask for the book, keep it, and read what stops it**

In `mac/JawKit/Sources/Client/Jawd.swift`, make seven changes.

After Part A's `layout(of:what:)`, add:

```swift
    /// Asks for the book of layouts jawd keeps for this interface, in the format it was kept in, so
    /// a newer Jaw's book comes as it is rather than converted to one this Jaw would keep over it; a
    /// jawd from before P10 answers that it keeps nothing. It comes as `.book`, and what stops it as
    /// `.bookRefused` with `keeping` false.
    public func book() {
        ask("layout.get", LayoutGet(), .book, expect: .layout)
    }

    /// Asks jawd to keep `book`, a book of layouts in format 2, at version `v`, newer than any it
    /// has told of. The answer comes as `.book`, its `kept` saying whether this is the book kept,
    /// and what stops it as `.bookRefused` with `keeping` true.
    public func keep(_ book: JSONValue, v: UInt64) {
        ask("layout.set", LayoutSet(v: v, layout: book), .keep, expect: .layout)
    }

    /// Waits up to `seconds` for jawd to answer the books asked to keep, as the app does when it
    /// quits, so its last change isn't lost with the connection, and gives back what came of the
    /// app's book meanwhile: answers, pushes and refusals. What else comes is dropped, since nothing
    /// will take it.
    public func waitForKeeping(seconds: Double) -> [JawUpdate] {
        let end = Date().addingTimeInterval(seconds)
        var came: [JawUpdate] = []
        while true {
            for update in drain() {
                switch update {
                case .book, .bookPushed, .bookRefused: came.append(update)
                default: break
                }
            }
            let waiting = asked.values.contains { if case .keep = $0 { true } else { false } }
            guard waiting, Date() < end else { return came }
            usleep(2000)
        }
    }
```

In `take(_:_:)`, after the `.push` case:

```swift
        case let .layout(kept):
            out.append(.bookPushed(kept))
```

In `answered(_:_:_:)`, after Part A's `.layout` case:

```swift
        case let (.book, .layout(kept)), let (.keep, .layout(kept)):
            out.append(.book(kept))
```

In `enum Asked`, after Part A's `case layout(String)`:

```swift
        /// The app's own book of layouts, asked for and asked to keep.
        case book, keep
```

In `refused(_:_:)`, after Part A's `.layout` case:

```swift
        case .book:
            return [.bookRefused(BookRefusal(failure), keeping: false)]
        case .keep:
            return [.bookRefused(BookRefusal(failure), keeping: true)]
```

Before `public enum JawUpdate`, after Part A's `Looked`, add:

```swift
/// Why jawd didn't give the app's book of layouts, or keep it.
public enum BookRefusal: Sendable, Hashable {
    /// This jawd keeps no books: one from before P10 refuses format 2, and one from before P1 knows
    /// no layouts.
    case unknown
    /// No answer came: the connection dropped first, or jawd wasn't connected.
    case unanswered
    /// jawd refused it, in its words: a limit, or a book it can't keep.
    case refused(String)

    /// A request's failure read as one of these. A jawd from before P10 reads a layout as format
    /// 1's tree before it looks at its format, so it refuses a book with -32602 for a field only
    /// trees have, and no `data`; its protocol said it refused another format naming `layout.format`
    /// and the formats it takes, 2 not among them. P10's jawd names the param of each -32602 it
    /// gives a book.
    public init(_ failure: Failure) {
        guard case let .jawd(e) = failure else {
            self = .unanswered
            return
        }
        let unnamed = e.data?["param"] == nil
        let without2 = e.data?["allowed"]?.array.map { !$0.contains(.number(2)) } ?? false
        if e.code == ErrorCode.unknownMethod || (e.code == ErrorCode.invalidParams && (unnamed || without2)) {
            self = .unknown
        } else {
            self = .refused(e.message)
        }
    }
}
```

In `JawUpdate`, after Part A's `case looked(chat: String, Looked)`:

```swift
    /// The book of layouts jawd keeps for this interface, as `book()` or `keep(_:v:)` was
    /// answered, `kept` saying which.
    case book(KeptLayout)
    /// A book another of this interface's connections kept, as the `layout` push says it.
    case bookPushed(KeptLayout)
    /// What stopped `book()`, or `keep(_:v:)` when `keeping`.
    case bookRefused(BookRefusal, keeping: Bool)
```

- [ ] **Step 7: The app lets them go until Task 4**

In `mac/JawApp/Sources/AppModel.swift`, the last case of `apply(_:)` becomes:

```swift
        case .push, .chats, .layout, .book, .bookPushed, .bookRefused:
            break
```

- [ ] **Step 8: Run the tests and build the app**

Run: `mac/scripts/build-mac test JawKit`, then `mac/scripts/build-mac build`.
Expected: both exit 0, and JawKit passes with seven more tests than before. Check that the live test ran rather than being skipped: `ssh -o BatchMode=yes build-mac "grep -E 'keeps the app.s book as it was given' ~/src/build-mac/jaw/jaw-panes/mac/build/check.log"` shows it passed. If it says skipped, `target/debug/jawd` is missing: copy the Mac builder's as it is, as the Global Constraints say. If it failed on `kept as it was given`, the jawd copied is from before P10: stop and ask the controller.

- [ ] **Step 9: The parity list loses the rows**

Run: `make check-parity`
Expected: exit 1, with `check-parity: the Mac sends layout.set now: take it out of mac/parity.toml (step 1.1)` and `check-parity: the Mac reads the push layout now: take it out of mac/parity.toml (step 1.1)`.

In `mac/parity.toml`, delete this line from `[requests]`:

```toml
"layout.set" = "1.1"
```

and this one from `[pushes]`:

```toml
"layout" = "1.1"
```

Run `make check-parity` again.
Expected: exit 0, its list holding two rows fewer than before and no line for step 1.1.

- [ ] **Step 10: Commit**

```bash
git add mac/Tests/JawKitTests/BookTests.swift mac/Tests/JawKitTests/FixtureTests.swift mac/Tests/JawKitTests/LiveTests.swift \
    mac/JawKit/Sources/Protocol/Layouts.swift mac/JawKit/Sources/Client/Incoming.swift mac/JawKit/Sources/Client/Client.swift \
    mac/JawKit/Sources/Client/Jawd.swift mac/JawApp/Sources/AppModel.swift mac/parity.toml
git commit -F- <<'EOF'
JawKit keeps the app's book of layouts in jawd as P10's format 2: it asks layout.get for the connection's own in the format it was kept in, so a newer Jaw's book comes as it is rather than converted, and layout.set to keep one at a version, reads both answers and, for the first time in its sources, the layout push into KeptLayout, a book kept whole as JSON for JawUI to read, reads what stops one as BookRefusal, a jawd that keeps no books (one from before P10, which reads a layout as a tree before its format and so refuses a book for a field only trees have, or one from before P1), no answer, or jawd's own words, and at quitting waits a while for jawd's answer to the last book and gives back what came; the Mac now sends layout.set and reads the layout push, so their rows leave mac/parity.toml and make check-parity lists nothing for step 1.1; tests: P10's fixtures decode and the request equals its fixture, a fake jawd answering as P10's and as one from before it, each refusal read as it should be, another connection's book pushed, the wait at quitting answered, refused, timed out and never sent, and a real jawd keeping the book as it was given and giving it back
EOF
```

---

### Task 2: The book in format 2, nine layouts, and what moves from the preferences

**Files:**
- Modify: `mac/JawUI/Sources/Panes/LayoutBook.swift` (`import`s at 1, the doc comment's last line at 8, `Problem` and `most` at 17-30, after `names` at 45-47, after `switchTo(_:)` at 127-131, `check(_:)` at 133-138); `mac/JawUI/Sources/Panes/PaneGrid.swift` (after `with(chat:in:)` at 312-322); `mac/JawUI/Sources/Panes/LayoutEditor.swift` (`notReady` at 163-174)
- Test: `mac/Tests/JawUITests/LayoutBookTests.swift`, `mac/Tests/JawUITests/LayoutEditorTests.swift`

**Interfaces:**
- Consumes: `JSONValue` (JawKit); Part A's `protocolFixtures` in `mac/Tests/JawUITests/TerminalPanesTests.swift`; the test helpers `work()` and `book(_:named:)` the JawUI tests have.
- Produces, in JawUI:
  - `LayoutBook.most == 9`, `LayoutBook.nameMost == 64`, `LayoutBook.format == 2`, `LayoutBook.Problem.long`.
  - `LayoutBook.forJawd: JSONValue` and `LayoutBook.init(fromJawd book: JSONValue) throws(Problem)`.
  - `LayoutBook.isEmpty: Bool`, `LayoutBook.movable: (book: LayoutBook, left: [Saved])` and `LayoutBook.following(_ answered: LayoutBook) -> LayoutBook`.
  - `PaneGrid.following(_ answered: PaneGrid?) -> PaneGrid`.
  - In JawUITests: `fixtureJSON(_:)`, `workBook()` and `earlier(_:current:)`.

- [ ] **Step 1: Write the failing tests**

In `mac/Tests/JawUITests/LayoutBookTests.swift`, add `import JawKit` after `import Foundation`, replace the test `names are unique, kept trimmed, and there are 30 at most` with

```swift
@Test func `names are unique, kept trimmed and 64 bytes at most, and there are nine at most`() throws {
    var b = try book(work())
    #expect(throws: LayoutBook.Problem.taken) { try b.saveAs(" work ") }
    #expect(throws: LayoutBook.Problem.noName) { try b.saveAs("  ") }
    #expect(throws: LayoutBook.Problem.long) { try b.saveAs(String(repeating: "\u{E9}", count: 33)) }
    try b.saveAs(String(repeating: "\u{E9}", count: 32))
    try b.saveAs(" Family ")
    #expect(b.names.last == "Family" && b.current == "Family")
    try b.rename("Family", to: "Home")
    #expect(b.current == "Home")
    #expect(throws: LayoutBook.Problem.taken) { try b.rename("Home", to: "Work") }
    for i in 4 ... LayoutBook.most {
        try b.saveAs("L\(i)")
    }
    #expect(LayoutBook.most == 9 && b.names.count == 9)
    #expect(throws: LayoutBook.Problem.most) { try b.saveAs("one more") }
}
```

and add these at the end of the file:

```swift
/// A protocol fixture's JSON.
func fixtureJSON(_ name: String) throws -> JSONValue {
    try JSONValue.parse(Data(contentsOf: protocolFixtures.appending(path: name)))
}

/// The book the protocol's fixture asks jawd to keep: Work on screen, its saved shape of three
/// slots and its edit of two, the keys on its second slot, and the unsaved panes beside it.
func workBook() throws -> LayoutBook {
    let unnamed = PaneGrid(cols: [0, 0.5, 1], rows: [0, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: "351912345678@s.whatsapp.net"),
        .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1),
    ])
    let saved = PaneGrid(cols: [0, 0.6, 1], rows: [0, 0.5, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 2, chat: "120363012345678901@g.us"),
        .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1, chat: "100000000002@lid"),
        .init(id: 3, c0: 1, c1: 2, r0: 1, r1: 2, chat: "351919999999@s.whatsapp.net"),
    ])
    let edited = PaneGrid(cols: [0, 0.6, 1], rows: [0, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: "120363012345678901@g.us"),
        .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1, chat: "100000000002@lid"),
    ])
    var b = LayoutBook()
    b.update(saved)
    try b.saveAs("Work")
    b.update(edited)
    b.focus = 2
    b.switchTo(nil)
    b.update(unnamed)
    b.focus = 1
    b.switchTo("Work")
    return b
}

/// A book as an earlier Jaw kept it in the preferences, with as many named layouts as it had, each
/// showing Ana, as the unsaved panes do.
func earlier(_ names: [String], current: String? = nil) throws -> LayoutBook {
    let grid = #"{"cols":[0,1],"rows":[0,1],"slots":[{"id":1,"c0":0,"c1":1,"r0":0,"r1":1,"chat":"351912345678@s.whatsapp.net"}]}"#
    let layouts = names.map { #"{"name":"\#($0)","grid":\#(grid),"focus":1}"# }.joined(separator: ",")
    let on = current.map { #""current":"\#($0)","# } ?? ""
    return try JSONDecoder().decode(LayoutBook.self, from: Data(#"{"v":1,\#(on)"unnamed":\#(grid),"unnamedFocus":1,"layouts":[\#(layouts)]}"#.utf8))
}

@Test func `the book is kept for jawd as format 2, its own JSON key for key, as the protocol's fixture draws it`() throws {
    let asked = try #require(fixtureJSON("layout.set.request.book.json")["params"]?["layout"])
    let drawn = try workBook()
    #expect(drawn.forJawd == asked)
    #expect(try LayoutBook(fromJawd: asked) == drawn, "and read back as it was")
    #expect(throws: LayoutBook.Problem.version) { try LayoutBook(fromJawd: .object(["format": .number(3), "layouts": .array([])])) }
    #expect(throws: LayoutBook.Problem.version) { try LayoutBook(fromJawd: .null) }
}

@Test func `a book jawd answers reads back, and the app's own takes each chat as jawd knows it, a slot jawd emptied left empty`() throws {
    let base = try workBook()
    let answered = try LayoutBook(fromJawd: #require(fixtureJSON("layout.set.result.book.json")["result"]?["layout"]))
    let theirs = try #require(answered.layouts.first)
    #expect(answered.current == "Work" && theirs.grid.slot(2)?.chat == "351912222222@s.whatsapp.net" && theirs.grid.slot(3)?.chat == nil)
    let followed = base.following(answered)
    #expect(followed.layouts[0].grid.slot(2)?.chat == "351912222222@s.whatsapp.net", "the person's chat as jawd knows it now")
    #expect(followed.layouts[0].grid.slot(3)?.chat == nil,
            "the slot jawd emptied waits, since jawd doesn't say whether its chat went or a slot before it shows it")
    #expect(followed.layouts[0].edited?.slot(2)?.chat == "351912222222@s.whatsapp.net" && followed.unnamed == base.unnamed)
    let phone = "351913333333@s.whatsapp.net", lid = "100000000003@lid"
    let mine = PaneGrid(cols: [0, 0.5, 1], rows: [0, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: phone), .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1, chat: lid),
    ])
    for person in [lid, phone] {
        let kept = PaneGrid(cols: [0, 0.5, 1], rows: [0, 1], slots: [
            .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: person), .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1),
        ])
        #expect(mine.following(kept).chats == [person] && mine.following(kept).holds,
                "a person by phone and then by LID: the later slot, which jawd emptied, stays empty, whichever JID jawd keeps them under")
    }
    let terminal = try LayoutBook(fromJawd: #require(fixtureJSON("layout.get.result.book.json")["result"]?["layout"]))
    #expect(terminal.grid == PaneGrid(cols: [0, 0.5, 1], rows: [0, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: "351912345678@s.whatsapp.net"),
        .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1, chat: "120363012345678901@g.us"),
    ]) && terminal.focus == 2 && terminal.current == nil, "the terminal's tree as jawd reads it into a book, as PaneGrid.from does")
}

@Test func `what moves to jawd is the unnamed layout and the first nine whose names it keeps, and one on screen left behind leaves the unsaved panes`() throws {
    let long = String(repeating: "\u{E9}", count: 33)
    let old = try earlier(["L1", long] + (3 ... 12).map { "L\($0)" }, current: "L11")
    #expect(old.names.count == 12, "a book from before keeps all its layouts as it's read")
    let (moving, left) = old.movable
    #expect(moving.names == [1, 3, 4, 5, 6, 7, 8, 9, 10].map { "L\($0)" } && left.map(\.name) == [long, "L11", "L12"])
    #expect(moving.current == nil && moving.grid == old.unnamed, "L11 stays behind, so the unsaved panes are on screen")
    let small = try book(work())
    let (all, none) = small.movable
    #expect(all == small && none.isEmpty)
    #expect(LayoutBook().isEmpty && !small.isEmpty)
}
```

In `mac/Tests/JawUITests/LayoutEditorTests.swift`, in `Save says why it can't go`, add these after `#expect(save.notReady == nil)`:

```swift
    save.name = String(repeating: "\u{E9}", count: 33)
    #expect(save.notReady == "a layout's name is 64 bytes at most")
    let full = LayoutEditorModel(mode: .saveAs, grid: work(), name: nil, taken: (1 ... 9).map { "L\($0)" }) { $0 }
    full.name = "Home"
    #expect(full.notReady == "9 layouts at most, one for each of ⌃⌘1 to ⌃⌘9")
```

and add this test after it, since a book kept in the preferences, where a jawd that keeps no books leaves it, can hold a layout named before names had a limit:

```swift
@MainActor
@Test func `a layout's own name from before, too long for jawd, doesn't stop its shape being saved`() {
    let long = String(repeating: "\u{E9}", count: 33)
    let kept = LayoutEditorModel(mode: .edit, grid: work(), name: long, taken: [long, "Family"]) { $0 }
    #expect(kept.notReady == nil, "its own name, as it was")
    kept.name = long + "s"
    #expect(kept.notReady == "a layout's name is 64 bytes at most", "a new name is held to jawd's limit")
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 1, with the build failing on `type 'LayoutBook.Problem' has no member 'long'`, `value of type 'LayoutBook' has no member 'forJawd'` and `value of type 'LayoutBook' has no member 'movable'`.

- [ ] **Step 3: The book, as jawd keeps it**

In `mac/JawUI/Sources/Panes/LayoutBook.swift`, make seven changes. `init(from:)` checks no limit, so a book from before with more than nine still reads whole, and `saveAs` refuses a tenth.

After `import Foundation`, add `import JawKit`.

The doc comment's last line, `/// Kept as version 1 JSON in the app's preferences until jawd keeps an interface's state (P1).`, becomes:

```swift
/// jawd keeps it as a book of layouts, format 2 of `layout.set` (P10): this JSON key for key, with
/// `format` in place of `v` (`forJawd`). Earlier Jaws kept it in the app's preferences as version 1,
/// which moves to jawd once, and the preferences still keep it with a jawd that keeps no books. Ten
/// layouts at most, as jawd keeps them: the unnamed one and nine named, the ones ⌃⌘0 to ⌃⌘9 reach
/// (the owner, 28 October), each name 64 bytes at most. A book read from the preferences keeps
/// however many it has.
```

`Problem` and `most`, from `public enum Problem` to `public static let most = 30`, become:

```swift
    public enum Problem: Error, Equatable, Sendable {
        case noName, taken, most, version, long

        public var said: String {
            switch self {
            case .noName: "a layout needs a name"
            case .taken: "there's a layout by that name already"
            case .most: "\(LayoutBook.most) layouts at most, one for each of ⌃⌘1 to ⌃⌘9"
            case .version: "these layouts are a newer Jaw's"
            case .long: "a layout's name is \(LayoutBook.nameMost) bytes at most"
            }
        }
    }

    /// The named layouts a book holds besides the unnamed one: those ⌃⌘1 to ⌃⌘9 reach.
    public static let most = 9
    /// The longest name jawd keeps a layout under, in bytes.
    public static let nameMost = 64
    /// The format jawd keeps a book in, where the app's own JSON has its version.
    public static let format = 2
```

After `names`, add:

```swift
    /// Whether it holds nothing: no panes before any layout, and no layout.
    public var isEmpty: Bool {
        unnamed == nil && layouts.isEmpty
    }
```

After `switchTo(_:)`, add:

```swift
    /// What of a book an earlier Jaw kept moves to jawd: the unnamed layout and the first nine named
    /// whose names jawd keeps, the ones ⌃⌘1 to ⌃⌘9 reached unless a name was too long; and the named
    /// ones it doesn't move, which stay where they were. When the layout on screen is one of those,
    /// the unnamed one is on screen in its place.
    public var movable: (book: LayoutBook, left: [Saved]) {
        var book = self
        var left: [Saved] = []
        book.layouts = []
        for saved in layouts {
            if book.layouts.count < Self.most, saved.name.utf8.count <= Self.nameMost {
                book.layouts.append(saved)
            } else {
                left.append(saved)
            }
        }
        if book.index == nil { book.current = nil }
        return (book, left)
    }

    /// This book with each slot's chat as `answered`, the same book as jawd keeps it, has it: a chat
    /// jawd knows by another JID now takes that one, and a slot jawd emptied is empty, its chat gone
    /// or shown by a slot before it, which jawd doesn't say.
    public func following(_ answered: LayoutBook) -> LayoutBook {
        var book = self
        book.unnamed = unnamed.map { $0.following(answered.unnamed) }
        for i in book.layouts.indices {
            guard let theirs = answered.layouts.first(where: { $0.name == book.layouts[i].name }) else { continue }
            book.layouts[i].grid = book.layouts[i].grid.following(theirs.grid)
            book.layouts[i].edited = book.layouts[i].edited.map { $0.following(theirs.edited) }
        }
        return book
    }
```

In `check(_:)`, after `guard !name.isEmpty else { throw .noName }`, add:

```swift
        guard name.utf8.count <= Self.nameMost else { throw .long }
```

After `check(_:)`, add:

```swift
    // MARK: Keeping it

    /// The book as jawd keeps it, format 2: this JSON key for key, `format` in place of `v`.
    public var forJawd: JSONValue {
        guard let data = try? JSONEncoder().encode(self), case var .object(keys)? = try? JSONValue.parse(data) else { return .null }
        keys["v"] = nil
        keys["format"] = .number(Double(Self.format))
        return .object(keys)
    }

    /// A book as jawd keeps it, read back; one in another format is a newer Jaw's.
    public init(fromJawd book: JSONValue) throws(Problem) {
        guard case var .object(keys) = book, keys["format"] == .number(Double(Self.format)) else { throw .version }
        keys["format"] = nil
        keys["v"] = .number(1)
        do {
            self = try JSONDecoder().decode(LayoutBook.self, from: JSONEncoder().encode(JSONValue.object(keys)))
        } catch {
            throw .version
        }
    }
```

- [ ] **Step 4: A grid following jawd's**

In `mac/JawUI/Sources/Panes/PaneGrid.swift`, after `with(chat:in:)`, add:

```swift
    /// With each slot's chat as `answered`, this grid as jawd keeps it, has it: jawd's JID for a
    /// chat it knows by another now, and none where jawd emptied the slot. jawd doesn't say whether
    /// that slot's chat went or a slot before it shows the same person under another JID, and
    /// nothing else here can tell, so the slot isn't given its chat back.
    public func following(_ answered: PaneGrid?) -> PaneGrid {
        guard let answered else { return self }
        var g = self
        for i in g.slots.indices {
            g.slots[i].chat = answered.slot(g.slots[i].id)?.chat
        }
        return g
    }
```

- [ ] **Step 5: The editor says both limits before Save**

In `mac/JawUI/Sources/Panes/LayoutEditor.swift`, `notReady` becomes:

```swift
    /// Why Save can't go yet, or nil when it can.
    public var notReady: String? {
        let n = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let renamed = !n.isEmpty && n.caseInsensitiveCompare(original ?? "") != .orderedSame
        if mode == .saveAs, n.isEmpty { return "a layout needs a name" }
        if renamed, n.utf8.count > LayoutBook.nameMost { return LayoutBook.Problem.long.said }
        if renamed, taken.contains(where: { $0.caseInsensitiveCompare(n) == .orderedSame }) {
            return "there's a layout by that name already"
        }
        if original == nil, !n.isEmpty, taken.count >= LayoutBook.most { return LayoutBook.Problem.most.said }
        if grid.slots.count > PaneGrid.most { return "\(PaneGrid.most) panes at most" }
        return nil
    }
```

`original` is nil for Save Layout As… and for the unsaved panes edited, the two ways a name makes a new layout. A layout's own name is held to neither limit, so one named before names had a limit, as a book in the preferences can hold, keeps it while its shape is saved: `LayoutBook.rename` leaves a name that hasn't changed as it is.

- [ ] **Step 6: Run the tests and build the app**

Run: `mac/scripts/build-mac test JawUITests`, then `mac/scripts/build-mac build`.
Expected: both exit 0, and JawUITests passes with four more tests than before.

- [ ] **Step 7: Commit**

```bash
git add mac/JawUI/Sources/Panes/LayoutBook.swift mac/JawUI/Sources/Panes/PaneGrid.swift mac/JawUI/Sources/Panes/LayoutEditor.swift \
    mac/Tests/JawUITests/LayoutBookTests.swift mac/Tests/JawUITests/LayoutEditorTests.swift
git commit -F- <<'EOF'
LayoutBook is what jawd keeps of the Mac's layouts, format 2 of layout.set: forJawd writes its own JSON key for key with format 2 in place of v and init(fromJawd:) reads it back, a book holds nine named layouts besides the unnamed one, the ten ⌃⌘0 to ⌃⌘9 reach as the owner chose on 28 October, in place of 30, and a new name is 64 bytes at most, as jawd keeps them, the layout editor saying either before Save while a layout's own name from before stays as it is; movable says what of a book an earlier Jaw kept moves to jawd, the unnamed layout and the first nine named whose names jawd keeps, the unsaved panes on screen in place of one left behind, and what stays, and following takes each slot's chat as jawd knows it now, a slot jawd emptied left empty, since jawd doesn't say whether its chat went or a slot before it shows the same person under another JID; tests against P10's book fixtures, and of each
EOF
```

---

### Task 3: Where the layouts are kept, and when they go to jawd

**Files:**
- Create: `mac/JawUI/Sources/Panes/LayoutKeeper.swift`
- Test: `mac/Tests/JawUITests/LayoutKeeperTests.swift`

**Interfaces:**
- Consumes: `KeptLayout`'s `v`, `book`, `layout`, `kept` and `dropped`, `SplitLayout`, and `BookRefusal` (Task 1); `LayoutBook`'s `forJawd`, `init(fromJawd:)`, `isEmpty`, `movable`, `following(_:)`, `update(_:)` and `nameMost` (Task 2); `earlier(_:current:)` (Task 2's tests).
- Produces, in JawUI:
  - `public struct LayoutHeld: Codable, Hashable, Sendable` with `static let format = 1`, `book: LayoutBook?`, `v: UInt64`, `said: Bool`, `public init(book: LayoutBook? = nil, v: UInt64 = 0, said: Bool = false)`, and its own `init(from:)` and `encode(to:)`, which write `format` and read only format 1 with no key it doesn't know.
  - `public struct LayoutKeeper: Sendable`, with `public init(held: LayoutHeld = LayoutHeld())`, `public enum Place: String, Sendable { case asking, trying, jawd, preferences, nowhere }` and `public enum Step: Equatable, Sendable { case open(LayoutBook), follow(LayoutBook), ask, keep(LayoutBook, v: UInt64), writePreferences, hold(LayoutBook, v: UInt64), release(UInt64), leftBehind(past: [String], long: [String]), say(String, warn: Bool), tell(String, String) }`.
  - `static let settle: TimeInterval = 2` and `static let retry: ClosedRange<TimeInterval> = 1 ... 30`; `place` and `v`, read-only; `asks`, `unanswered`, `asked` (the version of the keep on its way) and `due`.
  - `mutating func hello() -> [Step]`, `heard(_ answer: KeptLayout, now: Date, preferences: () throws -> LayoutBook?) -> [Step]`, `pushed(_ push: KeptLayout, now: Date, preferences: () throws -> LayoutBook?) -> [Step]`, `refused(_ why: BookRefusal, keeping: Bool, now: Date, preferences: () throws -> LayoutBook?) -> [Step]`, `opened()`, `changed(_ book: LayoutBook, now: Date, done: Bool = true) -> [Step]`, `fire(now: Date) -> [Step]`, `quitting(_ book: LayoutBook, now: Date) -> [Step]` and `quit() -> [Step]`. `preferences` reads the preferences' book, throwing when it can't be read.
  - `public static let leftTitle`, `public static func left(past: [String], long: [String]) -> String` and `public static func listed(_ names: [String]) -> String`, the alert's words.
- The places: `asking` until jawd gives its book or keeps the one moved to it; `trying` when jawd keeps nothing and there was nothing to move, until the first keep says whether this jawd keeps books; `jawd`; `preferences`, for the rest of the launch, with a jawd that keeps no books or one that refused the book moved to it, before jawd has kept the layouts; and `nowhere`, for the rest of the launch, when jawd keeps a book this Jaw can't read, or keeps no books while the preferences hold one this Jaw can't read. `due` is the soonest of when the change waiting is to be kept, none while there's none, jawd hasn't answered the last keep, is away or keeps a book this Jaw hasn't read; when it's to be held on this Mac, while jawd is away; and when to ask for jawd's book again. `.follow` takes jawd's chats into the book on screen; `.hold` and `.release` are what the app writes of `LayoutHeld`; and `.leftBehind` is the alert, which the app keeps that it has said.

- [ ] **Step 1: Write the failing tests**

`mac/Tests/JawUITests/LayoutKeeperTests.swift`:

```swift
import Foundation
import JawKit
@testable import JawUI
import Testing

/// Where the layouts are kept and when they go to jawd: jawd's book opened at launch and nothing
/// kept before, the preferences' book moved once with its layouts past nine named, a move whose
/// answer is lost, a jawd from before P10 leaving the layouts in the preferences, refusals in
/// jawd's words, a change kept once it has stayed for two seconds, the newest of jawd's books
/// taken and one not kept read again as kept, a layout.get refused asked again, what's held at
/// quitting and moved at the next launch, a book this Jaw can't read left alone, chats followed,
/// what's held while jawd is away, jawd's book gone back, the move made again, and the alert's
/// words.

/// A moment `seconds` after the tests' launch, and its time in milliseconds, as versions are.
func moment(_ seconds: Double) -> Date {
    Date(timeIntervalSince1970: 1_792_000_000 + seconds)
}

func millis(_ seconds: Double) -> UInt64 {
    UInt64(moment(seconds).timeIntervalSince1970 * 1000)
}

/// jawd's answer: the book at `v`, as `layout.get` gives it, or `layout.set` when `kept` says.
func answer(_ book: LayoutBook?, v: UInt64, kept: Bool? = nil, dropped: [String] = []) -> KeptLayout {
    KeptLayout(interface: "jaw-mac", v: v, book: book?.forJawd, kept: kept, dropped: dropped)
}

/// A keeper that has opened `book`, which jawd keeps at `v`, the alert said.
func keeping(_ book: LayoutBook, v: UInt64 = 5) -> LayoutKeeper {
    var k = LayoutKeeper(held: LayoutHeld(said: true))
    _ = k.heard(answer(book, v: v), now: moment(0)) { nil }
    k.opened()
    return k
}

/// `book` with another chat in its one pane, a change of chats.
func showing(_ book: LayoutBook, _ chat: String) -> LayoutBook {
    var b = book
    b.update(.single(chat))
    return b
}

@Test func `at launch the book jawd keeps opens, nothing is kept before the app has opened it, and one with chats jawd dropped is kept once more`() throws {
    var k = LayoutKeeper(held: LayoutHeld(said: true))
    #expect(k.hello() == [.ask] && k.place == .asking)
    let book = try earlier(["Work"], current: "Work")
    let steps = k.heard(answer(book, v: 5, dropped: ["351919999999@s.whatsapp.net"]), now: moment(0)) {
        Issue.record("the preferences aren't read when jawd keeps a book and the alert has said")
        return nil
    }
    #expect(steps == [.open(book), .say("jawd emptied a pane: its chat isn't here any more, or a pane before it shows that chat", warn: false)])
    #expect(k.place == .jawd)
    var unopened = k
    #expect(unopened.changed(LayoutBook(), now: moment(0.5)) == [] && unopened.due == nil, "nothing is kept before the app has opened it")
    #expect(unopened.quitting(LayoutBook(), now: moment(0.5)) == [] && unopened.quit() == [], "not even at quitting")
    k.opened()
    #expect(k.changed(book, now: moment(1)) == [] && k.due == moment(3), "kept as it opened, so jawd forgets the chat and the next launch doesn't say so")
    var calm = keeping(book)
    #expect(calm.changed(book, now: moment(1)) == [] && calm.due == nil, "what jawd keeps needs no keeping")
}

@Test func `the first time jawd keeps nothing, the preferences' book moves, its first nine whose names jawd keeps, opening once jawd has kept it, and the rest are named once`() throws {
    let long = String(repeating: "\u{E9}", count: 33)
    let old = try earlier((1 ... 8).map { "L\($0)" } + [long, "L10", "L11", "L12"], current: "L11")
    var k = LayoutKeeper()
    let asked = k.heard(answer(nil, v: 0), now: moment(0)) { old }
    guard case let .keep(moving, v)? = asked.first, asked.count == 1 else {
        Issue.record("not one book asked to keep: \(asked)")
        return
    }
    #expect(moving.names == (1 ... 8).map { "L\($0)" } + ["L10"] && moving.current == nil && moving.grid == old.unnamed,
            "the unsaved panes on screen in place of L11")
    #expect(k.place == .asking, "nothing opens until jawd has kept it")
    let opened = k.heard(answer(moving, v: v, kept: true), now: moment(0.25)) {
        Issue.record("read once is enough")
        return nil
    }
    #expect(opened == [.open(moving), .leftBehind(past: ["L11", "L12"], long: [long])] && k.place == .jawd)
    k.opened()
    #expect(k.changed(showing(moving, "351913333333@s.whatsapp.net"), now: moment(1)) == [] && k.due == moment(3),
            "kept by jawd from here, never in the preferences")
    var small = LayoutKeeper()
    let work = try earlier(["Work"])
    let sent = small.heard(answer(nil, v: 0), now: moment(0)) { work }
    #expect(sent == [.keep(work, v: millis(0))])
    let none = small.heard(answer(work, v: millis(0), kept: true), now: moment(0.25)) { nil }
    #expect(none == [.open(work), .leftBehind(past: [], long: [])], "nothing to name, which is kept too")
}

@Test func `a move whose answer is lost waits for jawd's book at the next hello, which opens what jawd kept or moves it again, and the alert still comes`() throws {
    let old = try earlier((1 ... 10).map { "L\($0)" })
    var k = LayoutKeeper()
    guard case let .keep(moving, v)? = k.heard(answer(nil, v: 0), now: moment(0), preferences: { old }).first else {
        Issue.record("nothing asked to keep")
        return
    }
    #expect(k.refused(.unanswered, keeping: true, now: moment(1)) { nil } == [] && k.place == .asking, "nothing opens or is written")
    #expect(k.due == moment(2), "and jawd's book is asked for again in a second if no hello comes first")
    var kept = k
    #expect(kept.hello() == [.ask])
    let opened = kept.heard(answer(moving, v: v), now: moment(5)) { old }
    #expect(opened == [.open(moving), .leftBehind(past: ["L10"], long: [])], "jawd kept it before the connection dropped")
    var unkept = k
    let again = unkept.heard(answer(nil, v: 0), now: moment(5)) { old }
    #expect(again == [.keep(moving, v: millis(5))], "jawd never read it, so the same nine move again")
    var beaten = LayoutKeeper()
    _ = beaten.heard(answer(nil, v: 0), now: moment(0)) { old }
    let other = showing(moving, "351913333333@s.whatsapp.net")
    let notKept = beaten.heard(answer(other, v: millis(0) + 9, kept: false), now: moment(0.25)) { nil }
    #expect(notKept == [.ask], "another connection's book was kept first, and its answer is in the format asked for")
    let read = beaten.heard(answer(other, v: millis(0) + 9), now: moment(0.5)) { old }
    #expect(read == [.open(other), .leftBehind(past: ["L10"], long: [])])
}

@Test func `a jawd that keeps no books leaves the layouts in the preferences as before, the preferences' book whole, and the next launch asks again, but for a book there this Jaw can't read`() throws {
    let old = try earlier((1 ... 12).map { "L\($0)" }, current: "L11")
    var k = LayoutKeeper()
    _ = k.heard(answer(nil, v: 0), now: moment(0)) { old }
    let fell = k.refused(.unknown, keeping: true, now: moment(0)) {
        Issue.record("read once is enough")
        return nil
    }
    #expect(fell == [.open(old)] && k.place == .preferences && !k.asks, "all twelve, L11 on screen, and nothing said")
    k.opened()
    #expect(k.changed(old, now: moment(0.5), done: false) == [], "a line still being dragged is written once its drag ends, as before")
    #expect(k.changed(old, now: moment(1)) == [.writePreferences] && k.due == nil)
    #expect(k.quitting(old, now: moment(2)) == [.writePreferences] && k.quit() == [])
    #expect(k.hello() == [], "this launch asks no more")
    var before = LayoutKeeper()
    let opened = before.refused(.unknown, keeping: false, now: moment(0)) { old }
    #expect(opened == [.open(old)] && before.place == .preferences, "a jawd from before P1 knows no layouts")
    var empty = LayoutKeeper()
    let nothing = empty.heard(answer(nil, v: 0), now: moment(0)) { nil }
    #expect(nothing == [.open(LayoutBook())] && empty.place == .trying)
    empty.opened()
    let first = showing(LayoutBook(), "351912345678@s.whatsapp.net")
    var quick = empty
    #expect(empty.changed(first, now: moment(1)) == [], "nothing written to the preferences while jawd might keep it")
    #expect(empty.fire(now: moment(3)) == [.keep(first, v: millis(3))])
    let refused = empty.refused(.unknown, keeping: true, now: moment(3.25)) { nil }
    #expect(refused == [.writePreferences] && empty.place == .preferences, "the preferences held no book, so the one on screen goes there")
    #expect(quick.quitting(first, now: moment(1)) == [.keep(first, v: millis(1))])
    _ = quick.refused(.unknown, keeping: true, now: moment(1.25)) { nil }
    #expect(quick.quit() == [.writePreferences], "refused in the second the app waits as it quits, so the preferences keep it, not this Mac")
    let held = showing(LayoutBook(), "351913333333@s.whatsapp.net")
    var never = LayoutKeeper(held: LayoutHeld(book: held, v: millis(9)))
    #expect(never.heard(answer(nil, v: 0), now: moment(20)) { nil } == [.keep(held, v: millis(20))])
    #expect(never.refused(.unknown, keeping: true, now: moment(20.25)) { nil } == [.open(held)] && never.place == .preferences,
            "held by a launch that never found out whether jawd keeps books, it opens, and the preferences keep the layouts")
    never.opened()
    #expect(never.changed(held, now: moment(21)) == [.writePreferences, .release(millis(9))], "and what was held goes once they have it")
    var newer = LayoutKeeper()
    let unread = newer.heard(answer(nil, v: 0), now: moment(0)) { throw LayoutBook.Problem.version }
    #expect(unread == [.open(LayoutBook()), .say("the saved layouts couldn't be read: these layouts are a newer Jaw's; they're left as they were", warn: true)])
    newer.opened()
    _ = newer.changed(first, now: moment(1))
    _ = newer.fire(now: moment(3))
    #expect(newer.refused(.unknown, keeping: true, now: moment(3.25)) { nil } == [.tell(LayoutKeeper.cantKeepTitle, LayoutKeeper.cantKeep(LayoutKeeper.keepsNone))],
            "they can't keep the layouts either, since it's never written over, which an alert says")
    #expect(newer.place == .nowhere && newer.changed(first, now: moment(4)) == [] && newer.due == nil)
}

@Test func `a refusal in jawd's words: of the preferences' book, they keep it for the launch, said in an alert; of a change, it's said and tried again; of the held book, it opens and stays held, as when a jawd that keeps none refuses it, said once`() throws {
    let old = try earlier(["Work", "Family"], current: "Work")
    let name = #"a layout's name is 1 to 64 bytes, and "" isn't"#
    var move = LayoutKeeper()
    _ = move.heard(answer(nil, v: 0), now: moment(0)) { old }
    let fell = move.refused(.refused(name), keeping: true, now: moment(0.25)) { nil }
    #expect(fell == [
        .open(old),
        .tell("jawd didn't take Jaw's layouts", "jawd said: \(name). This Mac's preferences keep them meanwhile, as before, and Jaw tries again at its next launch."),
    ] && move.place == .preferences)
    let quarantine = "jawd couldn't keep the layout; what failed is kept in quarantine"
    let notKept = "jawd didn't keep the layouts: \(quarantine); Jaw tries again with the next change and when it quits"
    var k = keeping(old)
    _ = k.changed(showing(old, "351914444444@s.whatsapp.net"), now: moment(0))
    _ = k.fire(now: moment(2))
    #expect(k.refused(.refused(quarantine), keeping: true, now: moment(2.25)) { nil } == [.say(notKept, warn: true)])
    #expect(k.place == .jawd && k.due == nil)
    let again = showing(old, "351913333333@s.whatsapp.net")
    _ = k.changed(again, now: moment(3))
    #expect(k.fire(now: moment(5)) == [.keep(again, v: millis(5))], "the next change tries jawd again")
    let held = showing(old, "351915555555@s.whatsapp.net")
    var launch = LayoutKeeper(held: LayoutHeld(book: held, v: millis(9), said: true))
    let moving = launch.heard(answer(old, v: 5), now: moment(20)) { nil }
    #expect(moving == [.keep(held, v: millis(20))])
    let opened = launch.refused(.refused(quarantine), keeping: true, now: moment(20.25)) { nil }
    #expect(opened == [.open(held), .say(notKept, warn: true)], "it opens, and nothing lets it go")
    launch.opened()
    #expect(launch.place == .jawd && launch.changed(held, now: moment(21)) == [] && launch.due == moment(23),
            "jawd keeps the older book, so the held one is asked to keep again")
    var swapped = LayoutKeeper(held: LayoutHeld(book: held, v: millis(9), said: true))
    _ = swapped.heard(answer(old, v: 5), now: moment(20)) { nil }
    let none = "jawd didn't keep the layouts: the jawd running now keeps no layouts; Jaw tries again with the next change and when it quits"
    #expect(swapped.refused(.unknown, keeping: true, now: moment(20.25)) { nil } == [.open(held), .say(none, warn: true)] && swapped.place == .jawd,
            "a jawd that keeps none, once jawd kept the layouts, can't be on this folder, so it reads as a refusal in jawd's words")
    swapped.opened()
    _ = swapped.changed(showing(old, "351916666666@s.whatsapp.net"), now: moment(21))
    #expect(swapped.fire(now: moment(23)).count == 1 && swapped.refused(.unknown, keeping: true, now: moment(23.25)) { nil } == [],
            "said once, not at every change")
}

@Test func `a change is kept once it has stayed for two seconds, only the newest, at a version past any jawd told of`() throws {
    let book = try earlier(["Work"], current: "Work")
    var k = keeping(book, v: millis(5))
    let one = showing(book, "351913333333@s.whatsapp.net"), two = showing(book, "351914444444@s.whatsapp.net")
    _ = k.changed(one, now: moment(0))
    #expect(k.due == moment(2))
    _ = k.changed(two, now: moment(1))
    #expect(k.due == moment(3), "each change waits two seconds more")
    #expect(k.fire(now: moment(2.5)) == [])
    #expect(k.fire(now: moment(3)) == [.keep(two, v: millis(5) + 1)] && k.due == nil, "one past jawd's version, which is ahead of the clock")
    #expect(k.heard(answer(two, v: millis(5) + 1, kept: true), now: moment(3.25)) { nil } == [])
    _ = k.changed(book, now: moment(10))
    #expect(k.fire(now: moment(12)) == [.keep(book, v: millis(12))], "the clock, once it's ahead")
}

@Test func `the newest book jawd tells of is taken, an older one isn't, a change waiting goes after it, and an older answer undoes nothing`() throws {
    let book = try earlier(["Work"], current: "Work")
    let other = showing(book, "351913333333@s.whatsapp.net"), mine = showing(book, "351914444444@s.whatsapp.net")
    var k = keeping(book, v: 5)
    #expect(k.pushed(answer(other, v: 7), now: moment(0)) { nil } == [.open(other)], "another connection's newer book")
    #expect(k.pushed(answer(book, v: 6), now: moment(0)) { nil } == [], "an older push after it")
    #expect(k.heard(answer(book, v: 7), now: moment(0)) { nil } == [], "an answer no newer, after the push")
    _ = k.changed(mine, now: moment(0))
    #expect(k.pushed(answer(book, v: millis(9)), now: moment(1)) { nil } == [], "a change of the app's own waits")
    #expect(k.fire(now: moment(2)) == [.keep(mine, v: millis(9) + 1)], "and goes after it")
    var raced = keeping(book, v: 5)
    _ = raced.changed(mine, now: moment(0))
    #expect(raced.fire(now: moment(2)) == [.keep(mine, v: millis(2))])
    #expect(raced.pushed(answer(other, v: millis(2) + 1), now: moment(2.25)) { nil } == [.open(other)],
            "another connection's, kept after jawd read this one back, pushed before its answer")
    #expect(raced.heard(answer(mine, v: millis(2), kept: true), now: moment(2.5)) { nil } == [], "the older answer opens nothing over it")
    #expect(raced.changed(other, now: moment(3)) == [] && raced.due == nil, "the newer book is the one jawd keeps")
}

@Test func `a book jawd answers it didn't keep is in the format asked for, so it's taken only once jawd gives it as it was kept, and nothing is kept or held meanwhile`() throws {
    let book = try earlier(["Work"], current: "Work")
    let mine = showing(book, "351914444444@s.whatsapp.net"), other = showing(book, "351913333333@s.whatsapp.net")
    var k = keeping(book, v: 5)
    _ = k.changed(mine, now: moment(0))
    _ = k.fire(now: moment(2))
    #expect(k.heard(answer(other, v: millis(2) + 5, kept: false), now: moment(2.25)) { nil } == [.ask], "asked for again, as it was kept")
    let next = showing(book, "351916666666@s.whatsapp.net")
    _ = k.changed(next, now: moment(3))
    #expect(k.due == nil && k.fire(now: moment(10)) == [], "nothing is kept over a book this Jaw hasn't read")
    var quitting = k
    #expect(quitting.quitting(next, now: moment(10)) == [] && quitting.quit() == [], "nor held at quitting")
    #expect(k.heard(answer(other, v: millis(2) + 5), now: moment(10.25)) { nil } == [.keep(next, v: millis(10.25))],
            "kept in format 2, it's taken, and the change that waited goes after it")
    var newer = keeping(book, v: 5)
    _ = newer.changed(mine, now: moment(0))
    _ = newer.fire(now: moment(2))
    _ = newer.heard(answer(other, v: millis(2) + 5, kept: false), now: moment(2.25)) { nil }
    let unreadable = KeptLayout(interface: "jaw-mac", v: millis(2) + 5, book: .object(["format": .number(3)]))
    #expect(newer.heard(unreadable, now: moment(2.5)) { nil } == [.tell(LayoutKeeper.unreadableTitle, LayoutKeeper.unreadable)])
    #expect(newer.place == .nowhere, "kept in a format this Jaw can't read, so it's never written over")
}

@Test func `a layout.get jawd refuses is asked again after a while, at launch, while a change lost with the connection waits for jawd to be back, and while jawd keeps a book this Jaw hasn't read`() throws {
    var k = LayoutKeeper(held: LayoutHeld(said: true))
    let reading = "jawd couldn't read the layout: database is locked"
    let notGiven = "jawd didn't give its layouts: \(reading); Jaw asks again in a moment, and keeps no change till then"
    let first = k.refused(.refused(reading), keeping: false, now: moment(0)) {
        Issue.record("the preferences' book could be older than jawd's, so it doesn't open")
        return nil
    }
    #expect(first == [.say(notGiven, warn: true)] && k.place == .asking && k.due == moment(1))
    #expect(k.fire(now: moment(1)) == [.ask])
    #expect(k.refused(.refused(reading), keeping: false, now: moment(1.25)) { nil } == [] && k.due == moment(3.25),
            "said once, and asked again later each time, up to thirty seconds")
    let book = try earlier(["Work"], current: "Work")
    #expect(k.fire(now: moment(3.25)) == [.ask])
    #expect(k.heard(answer(book, v: 5), now: moment(3.5)) { nil } == [.open(book)] && k.due == nil)
    var w = keeping(book, v: 5)
    let mine = showing(book, "351914444444@s.whatsapp.net")
    _ = w.changed(mine, now: moment(0))
    #expect(w.fire(now: moment(2)) == [.keep(mine, v: millis(2))] && w.unanswered && w.asked == millis(2))
    #expect(w.refused(.unanswered, keeping: true, now: moment(3)) { nil } == [.hold(mine, v: millis(2))] && w.due == nil && w.unanswered,
            "held on this Mac at once, and nothing is sent while jawd is away")
    #expect(w.hello() == [.ask])
    #expect(w.refused(.refused(reading), keeping: false, now: moment(4)) { nil }.count == 1 && w.due == moment(5),
            "the layout.get after it, refused, is asked again")
    #expect(w.fire(now: moment(5)) == [.ask])
    #expect(w.heard(answer(book, v: 5), now: moment(5.25)) { nil } == [.keep(mine, v: millis(5.25))],
            "once jawd has said what it keeps, the change goes")
    #expect(w.heard(answer(mine, v: millis(5.25), kept: true), now: moment(5.5)) { nil } == [.release(millis(5.25))],
            "and what was held goes once jawd keeps it")
    var u = keeping(book, v: 5)
    _ = u.changed(mine, now: moment(0))
    _ = u.fire(now: moment(2))
    _ = u.heard(answer(showing(book, "351913333333@s.whatsapp.net"), v: millis(2) + 5, kept: false), now: moment(2.25)) { nil }
    #expect(u.refused(.unanswered, keeping: false, now: moment(2.5)) { nil } == [] && u.due == moment(3.5),
            "the layout.get for a book this Jaw hasn't read, lost with the connection, is asked again too")
    #expect(u.fire(now: moment(3.5)) == [.ask])
}

@Test func `quitting keeps what's on screen at once, and holds it when jawd didn't answer, refused it or was away, at a version past any sent, but not what jawd kept, a newer book, or another connection's on screen`() throws {
    let book = try earlier(["Work"], current: "Work")
    let last = showing(book, "351913333333@s.whatsapp.net")
    var kept = keeping(book)
    #expect(kept.quitting(last, now: moment(1)) == [.keep(last, v: millis(1))], "not waiting for its two seconds")
    _ = kept.heard(answer(last, v: millis(1), kept: true), now: moment(1.25)) { nil }
    #expect(kept.quit() == [], "jawd kept it")
    var silent = keeping(book)
    _ = silent.quitting(last, now: moment(1))
    #expect(silent.quit() == [.hold(last, v: millis(1))], "jawd didn't answer in its second, so it's held at the version asked")
    var refused = keeping(book)
    _ = refused.quitting(last, now: moment(1))
    _ = refused.refused(.refused("jawd couldn't keep the layout; what failed is kept in quarantine"), keeping: true, now: moment(1.25)) { nil }
    #expect(refused.quit() == [.hold(last, v: millis(1))])
    var newer = keeping(book)
    _ = newer.quitting(last, now: moment(1))
    _ = newer.pushed(answer(book, v: millis(1) + 1), now: moment(1.25)) { nil }
    #expect(newer.quit() == [], "another connection's newer book wins")
    var away = keeping(book, v: millis(10))
    _ = away.changed(last, now: moment(10))
    _ = away.fire(now: moment(12))
    _ = away.refused(.unanswered, keeping: true, now: moment(12.5)) { nil }
    #expect(away.quitting(last, now: moment(5)) == [], "jawd is away, so nothing is sent")
    #expect(away.quit() == [.hold(last, v: millis(12) + 1)], "held past every version sent, though the clock went back")
    var calm = keeping(book)
    #expect(calm.quitting(book, now: moment(1)) == [] && calm.quit() == [], "nothing to keep")
    let other = showing(book, "351916666666@s.whatsapp.net")
    var theirs = keeping(book)
    _ = theirs.changed(last, now: moment(0))
    _ = theirs.fire(now: moment(2))
    #expect(theirs.pushed(answer(other, v: millis(2) + 1), now: moment(2.1)) { nil } == [.open(other)])
    #expect(theirs.quitting(other, now: moment(3)) == [] && theirs.quit() == [],
            "another connection's newer book on screen, a keep of the app's own on its way: no copy here to outrank that connection's next keep")
    var back = keeping(book)
    _ = back.changed(last, now: moment(0))
    _ = back.fire(now: moment(2))
    #expect(back.quitting(book, now: moment(3)) == [] && back.quit() == [.hold(book, v: millis(3))],
            "back to what jawd kept before, with a newer keep on its way that jawd may yet keep over it")
    var again = keeping(book)
    _ = again.changed(last, now: moment(0))
    _ = again.fire(now: moment(2))
    _ = again.pushed(answer(other, v: millis(2) + 1), now: moment(2.1)) { nil }
    #expect(again.quitting(last, now: moment(3)) == [] && again.quit() == [.hold(last, v: millis(3))],
            "back on the book on its way after a newer one was told of, which jawd will refuse it for: held past both")
    var waited = keeping(book)
    _ = waited.changed(last, now: moment(0))
    _ = waited.fire(now: moment(2))
    _ = waited.changed(showing(book, "351917777777@s.whatsapp.net"), now: moment(2.05))
    #expect(waited.pushed(answer(other, v: millis(2) + 1), now: moment(2.1)) { nil } == [], "not taken, since a change waits")
    #expect(waited.quitting(book, now: moment(3)) == [] && waited.quit() == [.hold(book, v: millis(3))],
            "back on the app's own book from before, which the newer one told of never replaced: held past it")
}

@Test func `what this Mac held at quitting moves to jawd at the next launch when jawd's book is older, and goes once jawd keeps it or a newer one`() throws {
    let book = try earlier(["Work"], current: "Work")
    let held = showing(book, "351913333333@s.whatsapp.net")
    var next = LayoutKeeper(held: LayoutHeld(book: held, v: millis(10), said: true))
    let moving = next.heard(answer(book, v: 5), now: moment(20)) {
        Issue.record("the preferences aren't read for a held book")
        return nil
    }
    #expect(moving == [.keep(held, v: millis(20))] && next.place == .asking, "jawd's is older, so the held book moves, opening once kept")
    #expect(next.heard(answer(held, v: millis(20), kept: true), now: moment(20.25)) { nil } == [.open(held), .release(millis(20))])
    #expect(next.place == .jawd)
    var caught = LayoutKeeper(held: LayoutHeld(book: held, v: millis(10), said: true))
    #expect(caught.heard(answer(book, v: millis(10)), now: moment(20)) { nil } == [.open(book), .release(millis(10))],
            "jawd keeps one as new: it opens, and the held one goes")
    var nothing = LayoutKeeper(held: LayoutHeld(book: held, v: millis(10), said: true))
    #expect(nothing.heard(answer(nil, v: 0), now: moment(20)) { nil } == [.keep(held, v: millis(20))],
            "with nothing kept, the held book moves, not the preferences'")
}

@Test func `a book jawd keeps that this Jaw can't read is a newer Jaw's: left as it is, with nothing kept or held for the launch, and said`() throws {
    let work = try earlier(["Work"])
    var k = LayoutKeeper(held: LayoutHeld(book: work, v: 12, said: true))
    let newer = KeptLayout(interface: "jaw-mac", v: 9, book: .object(["format": .number(3)]))
    let opened = k.heard(newer, now: moment(0)) {
        Issue.record("nothing is moved over it")
        return nil
    }
    #expect(opened == [.open(LayoutBook()), .tell(LayoutKeeper.unreadableTitle, LayoutKeeper.unreadable)])
    #expect(k.place == .nowhere && !k.asks, "the held book, newer by its version, isn't moved over it either")
    k.opened()
    let mine = showing(LayoutBook(), "351912345678@s.whatsapp.net")
    #expect(k.changed(mine, now: moment(1)) == [] && k.due == nil)
    #expect(k.quitting(mine, now: moment(2)) == [] && k.quit() == [], "nothing held, for a newer Jaw to move over it")
    var tree = LayoutKeeper(held: LayoutHeld(said: true))
    let kept = KeptLayout(interface: "jaw-mac", v: 4, layout: SplitLayout(focused: 0, tree: .chat("351912345678@s.whatsapp.net")))
    #expect(tree.heard(kept, now: moment(0)) { nil }.first == .open(LayoutBook()) && tree.place == .nowhere,
            "a tree kept under the app's name is none this Jaw reads as its book")
    var later = keeping(work, v: 5)
    let pushed = later.pushed(KeptLayout(interface: "jaw-mac", v: 6, book: .object(["format": .number(3)])), now: moment(0)) { nil }
    #expect(pushed == [.tell(LayoutKeeper.unreadableTitle, LayoutKeeper.unreadable)] && later.place == .nowhere, "another connection's, pushed")
}

@Test func `a book jawd keeps with a chat it knows by another JID now is followed, the keys and lines staying, but not over a change waiting`() throws {
    let book = try earlier(["Work"], current: "Work")
    let mine = showing(book, "351913333333@s.whatsapp.net"), resolved = showing(book, "100000000003@lid")
    var k = keeping(book)
    _ = k.changed(mine, now: moment(0))
    _ = k.fire(now: moment(2))
    #expect(k.heard(answer(resolved, v: millis(2), kept: true), now: moment(2.25)) { nil } == [.follow(resolved)])
    #expect(k.changed(resolved, now: moment(2.5)) == [] && k.due == nil, "followed, it's the book jawd keeps")
    var busy = keeping(book)
    _ = busy.changed(mine, now: moment(0))
    _ = busy.fire(now: moment(2))
    _ = busy.changed(showing(book, "351917777777@s.whatsapp.net"), now: moment(2.1))
    #expect(busy.heard(answer(resolved, v: millis(2), kept: true), now: moment(2.25)) { nil } == [], "a change waits, a line being dragged say")
}

@Test func `a keep the connection lost is held on this Mac at once, and each change while jawd is away once it has stayed for two seconds, till jawd keeps it`() throws {
    let book = try earlier(["Work"], current: "Work")
    let one = showing(book, "351913333333@s.whatsapp.net"), two = showing(book, "351914444444@s.whatsapp.net")
    var k = keeping(book, v: 5)
    _ = k.changed(one, now: moment(0))
    #expect(k.fire(now: moment(2)) == [.keep(one, v: millis(2))])
    #expect(k.refused(.unanswered, keeping: true, now: moment(2.5)) { nil } == [.hold(one, v: millis(2))], "so a crash now loses nothing")
    _ = k.changed(two, now: moment(3))
    #expect(k.due == moment(5) && k.fire(now: moment(5)) == [.hold(two, v: millis(5))], "each change while jawd is away, once it has stayed")
    #expect(k.due == nil, "and nothing more till the next change or jawd's return")
    _ = k.changed(book, now: moment(6))
    #expect(k.fire(now: moment(8)) == [.hold(book, v: millis(8))],
            "back to the book jawd kept before, held like any other change, since jawd may have kept the lost one")
    _ = k.changed(two, now: moment(9))
    #expect(k.fire(now: moment(11)) == [.hold(two, v: millis(11))])
    #expect(k.hello() == [.ask])
    #expect(k.heard(answer(book, v: 5), now: moment(20)) { nil } == [.keep(two, v: millis(20))], "jawd is back, so the change goes")
    #expect(k.heard(answer(two, v: millis(20), kept: true), now: moment(20.25)) { nil } == [.release(millis(20))],
            "and what was held goes once jawd keeps it")
    var quit = keeping(book, v: 5)
    _ = quit.changed(one, now: moment(0))
    _ = quit.fire(now: moment(2))
    _ = quit.refused(.unanswered, keeping: true, now: moment(2.5)) { nil }
    #expect(quit.quitting(two, now: moment(3)) == [] && quit.quit() == [.hold(two, v: millis(3))], "quitting holds what's on screen")
}

@Test func `a keep the connection lost may have been kept, so going back to the book jawd kept before is held, and kept once jawd is back, whatever it answers`() throws {
    let book = try earlier(["Work"], current: "Work")
    let one = showing(book, "351913333333@s.whatsapp.net")
    var k = keeping(book, v: 5)
    _ = k.changed(one, now: moment(0))
    _ = k.fire(now: moment(2))
    _ = k.refused(.unanswered, keeping: true, now: moment(2.5)) { nil }
    _ = k.changed(book, now: moment(3))
    #expect(k.fire(now: moment(5)) == [.hold(book, v: millis(5))], "jawd may have kept the lost book, so the one on screen is held")
    var away = k
    #expect(away.quitting(book, now: moment(6)) == [] && away.quit() == [.hold(book, v: millis(6))], "and so is it at quitting while jawd is away")
    #expect(k.hello() == [.ask])
    #expect(k.heard(answer(one, v: millis(2)), now: moment(7)) { nil } == [.keep(book, v: millis(7))],
            "jawd kept the lost book after all, at a version no newer than the app knows, and the one on screen goes over it")
}

@Test func `when jawd's book goes back to one older than it confirmed, as a restored jaw.db's, the book on screen is kept again at once`() throws {
    let book = try earlier(["Work"], current: "Work")
    let mine = showing(book, "351913333333@s.whatsapp.net")
    var k = keeping(book, v: 5)
    _ = k.changed(mine, now: moment(0))
    _ = k.fire(now: moment(2))
    _ = k.heard(answer(mine, v: millis(2), kept: true), now: moment(2.25)) { nil }
    #expect(k.hello() == [.ask], "jawd started again")
    #expect(k.heard(answer(book, v: 5), now: moment(10)) { nil } == [.keep(mine, v: millis(10))],
            "it keeps the book from before, so what's on screen goes again, though nothing changed")
    _ = k.heard(answer(mine, v: millis(10), kept: true), now: moment(10.25)) { nil }
    #expect(k.heard(answer(nil, v: 0), now: moment(20)) { nil } == [.keep(mine, v: millis(20))], "and when it keeps nothing at all")
    var newer = keeping(book, v: 5)
    _ = newer.changed(mine, now: moment(0))
    _ = newer.fire(now: moment(2))
    _ = newer.heard(answer(mine, v: millis(2), kept: true), now: moment(2.25)) { nil }
    let unreadable = KeptLayout(interface: "jaw-mac", v: 4, book: .object(["format": .number(3)]))
    #expect(newer.heard(unreadable, now: moment(10)) { nil } == [.tell(LayoutKeeper.unreadableTitle, LayoutKeeper.unreadable)]
        && newer.place == .nowhere, "gone back to a newer Jaw's book, which is left as it is")
}

@Test func `a launch that finds jawd keeping nothing once the move was made brings the layouts from before it back, and says so`() throws {
    let old = try earlier((1 ... 10).map { "L\($0)" })
    let (moving, _) = old.movable
    var k = LayoutKeeper(held: LayoutHeld(said: true))
    #expect(k.heard(answer(nil, v: 0), now: moment(0)) { old } == [.keep(moving, v: millis(0))], "the nine that moved the first time")
    #expect(k.heard(answer(moving, v: millis(0), kept: true), now: moment(0.25)) { nil } == [.open(moving), .tell(LayoutKeeper.backTitle, LayoutKeeper.back)],
            "opened once jawd keeps them again, and said, the layout left behind not named again")
    #expect(k.place == .jawd)
    var none = LayoutKeeper(held: LayoutHeld(said: true))
    #expect(none.heard(answer(nil, v: 0), now: moment(0)) { nil } == [.open(LayoutBook()), .say(LayoutKeeper.noneBack, warn: true)]
        && none.place == .trying, "with nothing there to bring back, the status line says jawd keeps none")
}

@Test func `the alert names the layouts that stayed in the preferences, with why`() {
    let start = "It keeps ten at most, the unsaved panes and nine named, the ones ⌃⌘0 to ⌃⌘9 reach, each name 64 bytes at most. "
    #expect(LayoutKeeper.left(past: ["Evening"], long: []) == start
        + "Evening stays in this Mac's preferences, where an earlier Jaw still opens it: it came past the nine.")
    #expect(LayoutKeeper.left(past: ["Evening", "Night"], long: ["A name far too long"]) == start
        + "Evening, Night and A name far too long stay in this Mac's preferences, where an earlier Jaw still opens them: "
        + "Evening and Night came past the nine, and A name far too long has a name longer than that.")
    #expect(LayoutKeeper.left(past: [], long: ["A", "B"]) == start
        + "A and B stay in this Mac's preferences, where an earlier Jaw still opens them: their names are longer than that.")
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 1, with the build failing on `cannot find 'LayoutKeeper' in scope` and `cannot find 'LayoutHeld' in scope`.

- [ ] **Step 3: The keeper**

`mac/JawUI/Sources/Panes/LayoutKeeper.swift`:

```swift
import Foundation
import JawKit

/// What this Mac holds of the layouts for a jaw folder, beside the book the preferences kept before
/// jawd did: what was on screen that jawd didn't confirm keeping, when the app last quit or while
/// jawd was away, at the version it would have in jawd, and whether the alert has named the layouts
/// of the preferences' book that didn't move to jawd, or found none. One value, written whole, in
/// format 1: one in another format, or with a key this Jaw doesn't know, is a newer Jaw's, which
/// this Jaw can't read and never writes over.
public struct LayoutHeld: Codable, Hashable, Sendable {
    /// The format this Jaw reads and writes.
    public static let format = 1

    public var book: LayoutBook?
    public var v: UInt64
    public var said: Bool

    public init(book: LayoutBook? = nil, v: UInt64 = 0, said: Bool = false) {
        self.book = book
        self.v = v
        self.said = said
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        let keys = try decoder.container(keyedBy: Named.self).allKeys
        guard try c.decode(Int.self, forKey: .format) == Self.format, keys.allSatisfy({ Keys(rawValue: $0.stringValue) != nil }) else {
            throw LayoutBook.Problem.version
        }
        book = try c.decodeIfPresent(LayoutBook.self, forKey: .book)
        v = try c.decode(UInt64.self, forKey: .v)
        said = try c.decode(Bool.self, forKey: .said)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(Self.format, forKey: .format)
        try c.encodeIfPresent(book, forKey: .book)
        try c.encode(v, forKey: .v)
        try c.encode(said, forKey: .said)
    }

    enum Keys: String, CodingKey {
        case format, book, v, said
    }

    /// Any key, by its name, to find those this Jaw doesn't know.
    struct Named: CodingKey {
        let stringValue: String
        var intValue: Int? { nil }

        init(stringValue: String) {
            self.stringValue = stringValue
        }

        init?(intValue _: Int) {
            nil
        }
    }
}

/// Where the app's layouts are kept, and when they go to jawd. jawd is the one store, as the
/// terminal has it (the owner, 29 October): it keeps the Mac's whole book as P10's format 2, asked
/// for at every hello and kept with `layout.set` once a change has stayed as it is for two seconds,
/// and at once when the app quits. The preferences' book moves to jawd once, the first time jawd
/// keeps nothing for the app, and again only if jawd keeps nothing after the move; it's written only
/// while the preferences keep the layouts for a launch, as before P10, with a jawd that keeps no
/// books or one that refused a move in its own words. What jawd didn't confirm keeping, at quitting
/// or while it was away, is held on this Mac and moved to jawd at the next launch. Of the books jawd
/// tells of, the newest by version is taken, unless a change of the app's own waits, which then goes
/// after it. A book jawd keeps that this Jaw can't read is never written over. A value: the app's
/// model does what its steps say.
public struct LayoutKeeper: Sendable {
    /// Where the layouts are kept.
    public enum Place: String, Sendable {
        /// Not known until jawd gives its book, or keeps the one moved to it: nothing opens or is
        /// kept meanwhile.
        case asking
        /// jawd keeps nothing for the app and there was nothing to move, so the first keep finds out
        /// whether this jawd keeps books.
        case trying
        /// By jawd.
        case jawd
        /// In the app's preferences, as before P10, for the rest of the launch: this jawd keeps no
        /// books, or refused the book moved to it before it had kept any.
        case preferences
        /// Nowhere, for the rest of the launch: jawd keeps a book this Jaw can't read, a newer Jaw's,
        /// which is left as it is, or keeps no books while the preferences hold one this Jaw can't
        /// read.
        case nowhere
    }

    /// What the app does.
    public enum Step: Equatable, Sendable {
        /// Opens this book in place of what's on screen, as switching does.
        case open(LayoutBook)
        /// Takes the chats of this book, as jawd keeps it, into the book on screen
        /// (`LayoutBook.following`), the keys and the lines staying where they are.
        case follow(LayoutBook)
        /// Asks jawd for its book.
        case ask
        /// Asks jawd to keep this book at version `v`.
        case keep(LayoutBook, v: UInt64)
        /// Writes the book on screen to the preferences, as before P10.
        case writePreferences
        /// Holds this book on this Mac at version `v`, for the next launch to move to jawd.
        case hold(LayoutBook, v: UInt64)
        /// Lets the book held on this Mac go: jawd keeps it, or a newer one, at this version.
        case release(UInt64)
        /// Names the layouts of the preferences' book that didn't move to jawd, past the nine or
        /// with names longer than it takes, in an alert, and keeps that it has; with none, keeps
        /// that there's nothing to name.
        case leftBehind(past: [String], long: [String])
        /// Says this in the status line.
        case say(String, warn: Bool)
        /// Says this in an alert over the window: its title, and its words.
        case tell(String, String)
    }

    /// How long a change stays as it is before it's kept, as the terminal's `KEEP_AFTER`.
    public static let settle: TimeInterval = 2
    /// How long after jawd didn't give its book it's asked for again: a second at first, doubling
    /// to thirty.
    public static let retry: ClosedRange<TimeInterval> = 1 ... 30

    public private(set) var place: Place = .asking
    /// The newest version jawd has told of or been asked to keep, and at launch the held book's:
    /// the next is newer, and no older book of jawd's is taken.
    public private(set) var v: UInt64
    /// The book held on this Mac, as the launch found it or as it held one since, until jawd keeps
    /// it or a newer one; and whether the alert has named the preferences' layouts that didn't move,
    /// or found none, which a launch sets only once it finds jawd keeping a book.
    var held: Sent?
    var said: Bool
    /// Whether the app has opened the book: nothing is kept before.
    var shown = false
    /// The book jawd keeps, as far as the app knows, and the version it came at; none at launch
    /// when jawd dropped chats from it, so it's kept once more and jawd forgets them, and none once
    /// a keep is lost, which jawd may have kept, so what's on screen is held, and kept once jawd is
    /// back whatever jawd answers.
    var kept: LayoutBook?
    var keptAt: UInt64 = 0
    /// The newest change of the app's own not asked to keep yet, and when it's due; the book asked
    /// to keep that jawd hasn't answered; and whether jawd went away before it answered, so the
    /// change waits for jawd's book at the next hello, held on this Mac once it's due.
    var waiting: LayoutBook?
    var waitingDue: Date?
    var sent: Sent?
    var lost = false
    /// A book moving to jawd, until jawd keeps it.
    var moving: Moving?
    /// The version of a book jawd keeps that this Jaw hasn't read as it was kept, which a `kept`
    /// false answer told of in the format asked for: nothing is kept or held till it's read.
    var unread: UInt64 = 0
    /// When to ask for jawd's book again, how long the wait after that one is, and whether jawd's
    /// refusal to give it has been said.
    var retryAt: Date?
    var backoff = LayoutKeeper.retry.lowerBound
    var notGivenSaid = false
    /// Whether the preferences' book couldn't be read when the launch looked, so the preferences
    /// can't keep the layouts; and whether a jawd that keeps no books, found after jawd kept them,
    /// has been said.
    var oldUnreadable = false
    var keepsNoneSaid = false
    /// Whether the app is quitting, so nothing more is taken from jawd or asked of it; what's on
    /// screen then, at the version it was asked to keep at or would have had; and the version of
    /// the last book jawd confirmed keeping as asked.
    var leaving = false
    var last: Sent?
    var confirmed: UInt64 = 0

    struct Sent: Equatable, Sendable {
        let book: LayoutBook
        let v: UInt64
    }

    /// A book moving to jawd: the preferences' first nine, with that book whole as `old` and whether
    /// they come `back` once jawd kept none after the move, or the one held on this Mac, with no
    /// `old`.
    struct Moving: Sendable {
        let book: LayoutBook
        let old: LayoutBook?
        var back = false
    }

    /// A keeper for a launch, with what this Mac holds of the layouts.
    public init(held: LayoutHeld = LayoutHeld()) {
        self.held = held.book.map { Sent(book: $0, v: held.v) }
        said = held.said
        v = held.v
    }

    /// Whether to ask jawd for the book at each hello: unless the layouts are kept in the
    /// preferences or nowhere for the rest of the launch.
    public var asks: Bool {
        place != .preferences && place != .nowhere
    }

    /// Whether a book asked to keep hasn't been answered: on its way, or lost with the connection.
    public var unanswered: Bool {
        sent != nil || lost
    }

    /// The version the book on its way to jawd was asked to keep at, until jawd answers.
    public var asked: UInt64? {
        sent?.v
    }

    /// When `fire` next has something to do: keep the change waiting, hold it on this Mac while
    /// jawd is away, or ask for jawd's book again.
    public var due: Date? {
        [keepDue, holdDue, retryAt].compactMap(\.self).min()
    }

    var keepDue: Date? {
        guard shown, !leaving, place == .trying || place == .jawd, sent == nil, !lost, unread == 0 else { return nil }
        return waitingDue
    }

    var holdDue: Date? {
        guard shown, !leaving, place == .trying || place == .jawd, lost, unread == 0 else { return nil }
        return waitingDue
    }

    // MARK: What jawd says

    /// jawd answered hello: its book is asked for at every hello, since a dropped connection misses
    /// what's pushed.
    public mutating func hello() -> [Step] {
        asks && !leaving ? [ask()] : []
    }

    /// jawd's answer to `book()`, or to `keep` when it says whether it `kept` the book.
    /// `preferences` reads the preferences' book, whole, throwing when it can't be read: to move it,
    /// to open it in jawd's place, or to name, once, the layouts of it that didn't move.
    public mutating func heard(_ answer: KeptLayout, now: Date, preferences: () throws -> LayoutBook?) -> [Step] {
        let book = Self.read(answer)
        if answer.kept != nil {
            guard let sent else { return [] }
            self.sent = nil
            return answered(answer, book, sent, preferences) + keepIfDue(now)
        }
        if answer.v >= unread { unread = 0 }
        backoff = Self.retry.lowerBound
        if place == .asking, moving == nil { return first(answer, book, now: now, preferences: preferences) }
        if lost {
            lost = false
            if waiting != nil { waitingDue = now }
        }
        if answer.v < confirmed { return wentBack(answer.v, book, now: now) + keepIfDue(now) }
        return told(answer.v, book, preferences) + keepIfDue(now)
    }

    /// A book another of the app's connections kept, as jawd pushes it, in the format it was kept in.
    public mutating func pushed(_ push: KeptLayout, now: Date, preferences: () throws -> LayoutBook?) -> [Step] {
        told(push.v, Self.read(push), preferences) + keepIfDue(now)
    }

    /// What stopped `book()`, or `keep` when `keeping`.
    public mutating func refused(_ why: BookRefusal, keeping: Bool, now: Date, preferences: () throws -> LayoutBook?) -> [Step] {
        guard keeping else { return notGiven(why, now: now, preferences: preferences) }
        guard let sent else { return [] }
        self.sent = nil
        if let moving {
            self.moving = nil
            return notMoved(why, moving, now: now, preferences: preferences)
        }
        switch why {
        case .unanswered:
            lost = true
            kept = nil
            if waiting == nil { waiting = sent.book }
            return hold(sent.book, v: sent.v)
        case .unknown where place == .trying:
            return inPreferences(readable: !oldUnreadable, why: Self.keepsNone, now: true)
        case let .refused(words) where place == .trying:
            let steps = inPreferences(readable: !oldUnreadable, why: "jawd didn't take this Jaw's either: \(words)", now: true)
            return steps + (place == .preferences && !leaving ? [.tell(Self.notTakenTitle, Self.notTaken(words))] : [])
        case .unknown:
            guard !leaving, !keepsNoneSaid else { return [] }
            keepsNoneSaid = true
            return [.say(Self.notKept(Self.keepsNone), warn: true)]
        case let .refused(words):
            return leaving ? [] : [.say(Self.notKept(words), warn: true)]
        }
    }

    // MARK: What the app does

    /// The app has opened the book the keeper gave it, once the chat list has its chats: changes
    /// count from here, so nothing before, an empty book or one half open, is kept over jawd's.
    public mutating func opened() {
        shown = true
    }

    /// The book on screen changed: kept by jawd once it has stayed as it is for two seconds, every
    /// move of a line being dragged counting, or written to the preferences at once, as before P10,
    /// a dragged line once its drag is `done`. Back to what jawd keeps, as jawd last said, nothing
    /// waits or is held.
    public mutating func changed(_ book: LayoutBook, now: Date, done: Bool = true) -> [Step] {
        guard shown, !leaving else { return [] }
        switch place {
        case .asking, .nowhere:
            return []
        case .preferences:
            return done ? [.writePreferences] + releaseHeld() : []
        case .trying, .jawd:
            if book == kept, sent == nil, !lost {
                waiting = nil
                waitingDue = nil
                return releaseHeld()
            }
            if book != waiting {
                waiting = book
                waitingDue = now.addingTimeInterval(Self.settle)
            }
            return []
        }
    }

    /// The time `due` said has come.
    public mutating func fire(now: Date) -> [Step] {
        var steps: [Step] = []
        if let at = retryAt, at <= now { steps.append(ask()) }
        return steps + keepIfDue(now)
    }

    /// The app quits: what's on screen is kept at once where the layouts are kept, by jawd unless
    /// it's what jawd keeps or was last asked to keep, and nothing more is taken from jawd. Once the
    /// app has waited for jawd's answer, `quit` says what to hold: nothing when what's on screen is
    /// another connection's book jawd told of after the app's own keep went, since a copy held here
    /// could outrank that connection's next keep; the keep on its way, at its version, when it's
    /// what's on screen and jawd has told of nothing newer since; and otherwise what's on screen,
    /// at a version past any jawd told of.
    public mutating func quitting(_ book: LayoutBook, now: Date) -> [Step] {
        guard shown, !leaving else { return [] }
        leaving = true
        waiting = nil
        waitingDue = nil
        retryAt = nil
        switch place {
        case .asking, .nowhere:
            return []
        case .preferences:
            return [.writePreferences] + releaseHeld()
        case .trying, .jawd:
            guard unread == 0 else { return [] }
            if let sent {
                guard book != kept || keptAt <= sent.v else { return [] }
                last = sent.book == book && v == sent.v ? sent : Sent(book: book, v: next(now))
                return []
            }
            if book == kept { return releaseHeld() }
            if lost {
                last = Sent(book: book, v: next(now))
                return []
            }
            let step = send(book, now)
            last = sent
            return [step]
        }
    }

    /// The app has waited its time for jawd's answer as it quits: what's on screen is held on this
    /// Mac, unless jawd confirmed keeping it or told of a newer book meanwhile, at the version it
    /// was asked to keep at, or one past any jawd told of when it couldn't be sent. A jawd that
    /// said meanwhile that it keeps no books has the preferences keep it instead.
    public mutating func quit() -> [Step] {
        if place == .preferences, last != nil { return [.writePreferences] + releaseHeld() }
        guard let last, place != .nowhere, unread == 0, v == last.v, confirmed != last.v else { return [] }
        return [.hold(last.book, v: last.v)]
    }

    // MARK: Inside

    /// jawd's first answer to `book()`: its book opens, unless the one held on this Mac is newer,
    /// which moves to jawd. With nothing kept, what's held moves, or else the preferences' first
    /// nine, back again when the alert has said and so the move was made, or the first change finds
    /// out whether this jawd keeps books. One this Jaw can't read is left as it is.
    mutating func first(_ answer: KeptLayout, _ book: LayoutBook?, now: Date, preferences: () throws -> LayoutBook?) -> [Step] {
        v = max(v, answer.v)
        if answer.v > 0, book == nil { return cantRead() }
        kept = book
        keptAt = answer.v
        if let held, held.v > answer.v { return move(held.book, old: nil, now) }
        if let book { return took(book, at: answer.v, dropped: answer.dropped.count, preferences) }
        let old: LayoutBook?
        do {
            old = try preferences()
        } catch {
            oldUnreadable = true
            return tryKeeping() + [.say(Self.notRead(error), warn: true)]
        }
        guard let old, !old.isEmpty else { return tryKeeping() + (said ? [.say(Self.noneBack, warn: true)] : []) }
        return move(old.movable.book, old: old, now, back: said)
    }

    /// jawd keeps nothing for the app and there's nothing to move: the unsaved panes open empty,
    /// and the first change kept finds out whether this jawd keeps books.
    mutating func tryKeeping() -> [Step] {
        place = .trying
        kept = LayoutBook()
        return [.open(LayoutBook())]
    }

    /// Asks jawd to keep a book this Mac kept, held from the last quit or the preferences' first
    /// nine, `back` when they come again; it opens once jawd has kept it.
    mutating func move(_ book: LayoutBook, old: LayoutBook?, _ now: Date, back: Bool = false) -> [Step] {
        moving = Moving(book: book, old: old, back: back)
        return [send(book, now)]
    }

    /// jawd's book, taken as the one on screen; a book held on this Mac that's no newer goes.
    mutating func took(_ book: LayoutBook, at w: UInt64, dropped: Int = 0, _ preferences: () throws -> LayoutBook?) -> [Step] {
        place = .jawd
        moving = nil
        kept = dropped == 0 ? book : nil
        keptAt = w
        let gone: [Step] = dropped == 0 ? [] : [.say(Self.gone(dropped), warn: false)]
        return [.open(book)] + released(at: w) + leftBehind(preferences) + gone
    }

    /// jawd's answer to the book asked to keep. Kept as asked, it's the book jawd keeps, each chat
    /// as jawd knows it now and a slot it emptied empty, said in the status line, and a book moved
    /// there opens. Not kept, another connection's newer book is, which the answer gives in the
    /// format asked for rather than the one it was kept in, so it's asked for again, and nothing is
    /// kept till it comes.
    mutating func answered(_ answer: KeptLayout, _ book: LayoutBook?, _ sent: Sent, _ preferences: () throws -> LayoutBook?) -> [Step] {
        guard answer.kept == true, answer.v == sent.v else {
            let wasMoving = moving != nil
            moving = nil
            if answer.v > v { unread = max(unread, answer.v) }
            return !leaving && (answer.v > v || wasMoving) ? [ask()] : []
        }
        confirmed = sent.v
        guard sent.v == v else { return [] }
        let resolved = book.map { sent.book.following($0) } ?? sent.book
        kept = resolved
        keptAt = sent.v
        place = .jawd
        if let move = moving {
            moving = nil
            let back: [Step] = move.back ? [.tell(Self.backTitle, Self.back)] : []
            return [.open(resolved)] + released(at: sent.v) + leftBehind { try move.old ?? preferences() } + back
        }
        let follow: [Step] = if let book, !leaving, waiting == nil, resolved != sent.book { [.follow(book)] } else { [] }
        let gone: [Step] = follow.isEmpty || answer.dropped.isEmpty ? [] : [.say(Self.gone(answer.dropped.count), warn: false)]
        return follow + gone + released(at: sent.v) + leftBehind(preferences)
    }

    /// A book jawd told of at version `w`, another connection's or its answer to `book()`: taken if
    /// it's newer than any the app knows, unless a change of the app's own waits to go after it, or
    /// the app quits; one this Jaw can't read is left as it is. What's held stays till the change
    /// waiting is kept.
    mutating func told(_ w: UInt64, _ book: LayoutBook?, _ preferences: () throws -> LayoutBook?) -> [Step] {
        if w >= unread { unread = 0 }
        guard w > v else { return [] }
        v = w
        guard asks, !leaving else { return [] }
        guard let book else { return cantRead() }
        guard waiting == nil else { return [] }
        return took(book, at: w, preferences)
    }

    /// jawd's answer to `book()` at version `w`, older than a book it confirmed keeping: its book
    /// went back, as when its jaw.db is restored or linked again. What it keeps now is the book it
    /// keeps, as far as the app knows, so the one on screen is kept again at once, unless a keep is
    /// on its way or a change waits, which goes anyway; one this Jaw can't read is left as it is.
    mutating func wentBack(_ w: UInt64, _ book: LayoutBook?, now: Date) -> [Step] {
        confirmed = w
        guard asks, !leaving else { return [] }
        if w > 0, book == nil { return cantRead() }
        if sent == nil, waiting == nil, let onScreen = kept {
            waiting = onScreen
            waitingDue = now
        }
        kept = book
        keptAt = w
        return []
    }

    /// The book held on this Mac goes once jawd keeps one as new, at version `w`.
    mutating func released(at w: UInt64) -> [Step] {
        guard let held, w >= held.v else { return [] }
        self.held = nil
        return [.release(w)]
    }

    /// The book held on this Mac goes, whatever its version: what's on screen is what jawd keeps,
    /// as jawd last said, or the preferences keep it now.
    mutating func releaseHeld() -> [Step] {
        guard let held else { return [] }
        self.held = nil
        return [.release(held.v)]
    }

    /// Holds `book` on this Mac at version `v`, for the next launch to move to jawd: a keep lost with
    /// the connection, or a change made while jawd is away. Nothing is held while jawd keeps a book
    /// this Jaw hasn't read, nor once the app quits, when `quit` says what to hold.
    mutating func hold(_ book: LayoutBook, v: UInt64) -> [Step] {
        guard unread == 0, !leaving else { return [] }
        held = Sent(book: book, v: v)
        return [.hold(book, v: v)]
    }

    /// The layouts of the preferences' book that didn't move to jawd, named once: those `movable`
    /// leaves out, past the nine or with names longer than jawd takes; none when the book can't be
    /// read, which is said.
    mutating func leftBehind(_ preferences: () throws -> LayoutBook?) -> [Step] {
        guard !said else { return [] }
        said = true
        do {
            let left = try preferences()?.movable.left.map(\.name) ?? []
            let long = left.filter { $0.utf8.count > LayoutBook.nameMost }
            return [.leftBehind(past: left.filter { !long.contains($0) }, long: long)]
        } catch {
            return [.leftBehind(past: [], long: []), .say(Self.notRead(error), warn: true)]
        }
    }

    /// jawd keeps a layout for the app that this Jaw can't read as its book, a newer Jaw's: it's
    /// never written over, and nothing is kept or held for the rest of the launch, which an alert
    /// says, the unsaved panes opening empty when nothing is open yet.
    mutating func cantRead() -> [Step] {
        let opening = place == .asking
        place = .nowhere
        moving = nil
        waiting = nil
        waitingDue = nil
        lost = false
        retryAt = nil
        return (opening ? [.open(LayoutBook())] : []) + [.tell(Self.unreadableTitle, Self.unreadable)]
    }

    /// What stopped `book()`. A jawd from before P1 knows no layouts, so the preferences keep them,
    /// as before, opening what's held or else their book; otherwise it's asked for again after a
    /// while, and at the next hello, while the app waits for it, at launch, with a change waiting
    /// for jawd to be back, or while jawd keeps a book this Jaw hasn't read, jawd's words said once.
    mutating func notGiven(_ why: BookRefusal, now: Date, preferences: () throws -> LayoutBook?) -> [Step] {
        if why == .unknown, place == .asking, moving == nil {
            do {
                let old = try preferences()
                return [.open(held?.book ?? old ?? LayoutBook())] + inPreferences(readable: true, why: Self.keepsNone, now: false)
            } catch {
                return [.open(held?.book ?? LayoutBook()), .say(Self.notRead(error), warn: true)]
                    + inPreferences(readable: false, why: Self.keepsNone, now: false)
            }
        }
        guard place == .asking || lost || unread != 0, !leaving else { return [] }
        retryAt = now.addingTimeInterval(backoff)
        backoff = min(backoff * 2, Self.retry.upperBound)
        guard case let .refused(words) = why, !notGivenSaid else { return [] }
        notGivenSaid = true
        return [.say(Self.notGiven(words), warn: true)]
    }

    /// What stopped a book moving to jawd. Lost, jawd's book is asked for again. The preferences'
    /// book, refused, opens whole and the preferences keep the layouts for the launch, as before
    /// P10, jawd's words said in an alert; the next launch moves it again. The held book, refused,
    /// opens and stays held, jawd's words said: the next change, quitting and the next launch try
    /// jawd again. Refused by a jawd that keeps no books before jawd kept the layouts here, the held
    /// book opens and goes to the preferences with their first write; after, that can't happen on
    /// one folder, and it's read as jawd's words, said once.
    mutating func notMoved(_ why: BookRefusal, _ move: Moving, now: Date, preferences: () throws -> LayoutBook?) -> [Step] {
        if why == .unanswered {
            retryAt = now.addingTimeInterval(backoff)
            return []
        }
        if let old = move.old {
            let steps: [Step] = [.open(old)] + inPreferences(readable: true, why: Self.keepsNone, now: false)
            guard case let .refused(words) = why else { return steps }
            return steps + [.tell(Self.notTakenTitle, Self.notTaken(words))]
        }
        if why == .unknown, !said {
            do {
                _ = try preferences()
                return [.open(move.book)] + inPreferences(readable: true, why: Self.keepsNone, now: false)
            } catch {
                return [.open(move.book)] + inPreferences(readable: false, why: Self.keepsNone, now: false)
            }
        }
        place = .jawd
        if case let .refused(words) = why { return [.open(move.book), .say(Self.notKept(words), warn: true)] }
        keepsNoneSaid = true
        return [.open(move.book), .say(Self.notKept(Self.keepsNone), warn: true)]
    }

    /// The preferences keep the layouts for the rest of the launch, as before P10, nothing waiting
    /// for jawd, and what's held goes once they've kept what's on screen, at once when it's `now`
    /// on screen to keep. When they hold a book this Jaw can't read, which is never written over,
    /// the layouts are kept nowhere for the launch instead, an alert saying so and `why` jawd keeps
    /// them neither; refused so as the app quits, they stay where they were for `quit` to hold.
    mutating func inPreferences(readable: Bool, why: String, now: Bool) -> [Step] {
        waiting = nil
        waitingDue = nil
        lost = false
        retryAt = nil
        guard readable else {
            guard !leaving else { return [] }
            place = .nowhere
            return [.tell(Self.cantKeepTitle, Self.cantKeep(why))]
        }
        place = .preferences
        return now && !leaving ? [.writePreferences] + releaseHeld() : []
    }

    mutating func ask() -> Step {
        retryAt = nil
        return .ask
    }

    /// The change waiting, once it's due: asked to keep, or held on this Mac while jawd is away,
    /// where it waits for jawd to be back. Back to what jawd keeps, as jawd last said, nothing is
    /// held.
    mutating func keepIfDue(_ now: Date) -> [Step] {
        if let at = holdDue, at <= now, let waiting {
            waitingDue = nil
            guard waiting != kept else { return releaseHeld() }
            let version = next(now)
            return hold(waiting, v: version)
        }
        guard let at = keepDue, at <= now, let waiting else { return [] }
        guard waiting != kept else {
            self.waiting = nil
            waitingDue = nil
            return releaseHeld()
        }
        return [send(waiting, now)]
    }

    /// Asks jawd to keep `book` at a version newer than any the app knows.
    mutating func send(_ book: LayoutBook, _ now: Date) -> Step {
        let at = next(now)
        sent = Sent(book: book, v: at)
        waiting = nil
        waitingDue = nil
        return .keep(book, v: at)
    }

    /// The next version: the time in milliseconds, or one past the newest when that's ahead of it,
    /// as `crates/jaw/src/view/panes/kept.rs` versions the terminal's.
    mutating func next(_ now: Date) -> UInt64 {
        v = max(UInt64(max(now.timeIntervalSince1970, 0) * 1000), v + 1)
        return v
    }

    static func read(_ kept: KeptLayout) -> LayoutBook? {
        kept.book.flatMap { try? LayoutBook(fromJawd: $0) }
    }

    // MARK: What's said

    /// What's said when jawd emptied slots, whose chats are gone or shown by a slot before them: how
    /// many, since jawd doesn't say which slot each was in, or which of the two it was.
    static func gone(_ n: Int) -> String {
        n == 1 ? "jawd emptied a pane: its chat isn't here any more, or a pane before it shows that chat"
            : "jawd emptied \(n) panes: their chats aren't here any more, or panes before them show those chats"
    }

    static let keepsNone = "the jawd running now keeps no layouts"

    static func notRead(_ error: any Error) -> String {
        "the saved layouts couldn't be read: \((error as? LayoutBook.Problem)?.said ?? "\(error)"); they're left as they were"
    }

    static let noneBack = "jawd keeps none of Jaw's layouts, as when its jaw.db is new or restored from before, and this Mac has none from before to bring back"

    static let backTitle = "Jaw's layouts came back"

    static let back =
        "jawd kept none of Jaw's layouts, as when its jaw.db is new or restored from before, so those this Mac kept before they moved to jawd came back, and jawd keeps them again. Any deleted since are back too, and any changed since are as they were then."

    static let cantKeepTitle = "Jaw can't keep its layouts this time"

    static func cantKeep(_ why: String) -> String {
        "This Mac's preferences hold layouts this Jaw can't read, a newer Jaw's or damaged ones, and it leaves them as they are; and \(why). So the panes you make now aren't kept."
    }

    static func notKept(_ why: String) -> String {
        "jawd didn't keep the layouts: \(why); Jaw tries again with the next change and when it quits"
    }

    static func notGiven(_ why: String) -> String {
        "jawd didn't give its layouts: \(why); Jaw asks again in a moment, and keeps no change till then"
    }

    static let notTakenTitle = "jawd didn't take Jaw's layouts"

    static func notTaken(_ why: String) -> String {
        "jawd said: \(why). This Mac's preferences keep them meanwhile, as before, and Jaw tries again at its next launch."
    }

    static let unreadableTitle = "jawd keeps layouts a newer Jaw made"

    static let unreadable =
        "This Jaw can't read them, so it leaves them as they are and keeps no layout of its own: the panes you make now aren't kept. A newer Jaw opens them as they were."

    /// The title of the alert naming the preferences' layouts that didn't move to jawd.
    public static let leftTitle = "jawd keeps Jaw's layouts now"

    /// What the alert says of the preferences' layouts that didn't move to jawd: past the nine it
    /// keeps, or with names longer than it takes.
    public static func left(past: [String], long: [String]) -> String {
        let all = past + long, one = all.count == 1
        let why: String = switch (past.isEmpty, long.isEmpty) {
        case (false, true): one ? "it came past the nine" : "they came past the nine"
        case (true, false): one ? "its name is longer than that" : "their names are longer than that"
        default: "\(listed(past)) came past the nine, and \(listed(long)) \(long.count == 1 ? "has a name" : "have names") longer than that"
        }
        return "It keeps ten at most, the unsaved panes and nine named, the ones ⌃⌘0 to ⌃⌘9 reach, each name \(LayoutBook.nameMost) bytes at most. "
            + "\(listed(all)) \(one ? "stays" : "stay") in this Mac's preferences, where an earlier Jaw still opens \(one ? "it" : "them"): \(why)."
    }

    /// Names as a sentence lists them: "Evening", "Evening and Night", "Evening, Night and Weekend".
    public static func listed(_ names: [String]) -> String {
        names.count < 2 ? names.joined() : names.dropLast().joined(separator: ", ") + " and \(names[names.count - 1])"
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 0, with eighteen more tests than before.

- [ ] **Step 5: Commit**

```bash
git add mac/JawUI/Sources/Panes/LayoutKeeper.swift mac/Tests/JawUITests/LayoutKeeperTests.swift
git commit -F- <<'EOF'
LayoutKeeper, in JawUI, says where the Mac's layouts are kept and when they go to jawd, the one store as the owner chose on 29 October: the book jawd keeps opens at launch, nothing being kept before the app has opened it, and one with chats jawd dropped is kept once more; the preferences' book moves to jawd the first time jawd keeps nothing, its first nine whose names jawd keeps, and opens once jawd has kept it, never written again, the rest named once in an alert whose words it gives, at whichever launch first finds jawd keeping a book, so a move whose answer is lost or beaten loses nothing, and a launch that finds jawd keeping nothing once the move was made brings them back and says so, as the owner chose; a change is kept once it has stayed for two seconds, only the newest, at a version past any jawd told of, the newest of jawd's books taken unless a change waits, a book jawd didn't keep taken only once jawd gives it as it was kept, a layout.get jawd refuses asked again on a backoff, and the book on screen kept again when jawd's goes back to an older one; a keep the connection lost, and each change while jawd is away once it has settled, is held on this Mac in LayoutHeld, one value in format 1, so a crash then loses only two seconds, and since jawd may have kept the lost book the app stops counting on the one jawd last confirmed, so going back to it is held too and kept once jawd is back, whatever jawd answers; at quitting what's on screen is kept at once and held when jawd doesn't confirm it, at a version past any sent unless it's the keep on its way with nothing newer told of, but not another connection's book on screen, and the next launch moves what's held to jawd when jawd's book is older, a refusal there leaving it held; a jawd that keeps no books leaves the layouts in the preferences as before P10, written once a dragged line's drag ends, as does a refusal of the move in jawd's words, said in an alert, and with a book in the preferences this Jaw can't read the layouts are kept nowhere, said in an alert, as when jawd keeps a book this Jaw can't read, which is never written over; a value the app's model follows, tested on its own
EOF
```

---

### Task 4: The app keeps its layouts in jawd, and holds on this Mac what jawd didn't confirm

**Files:**
- Create: `mac/JawUI/Sources/Panes/LayoutPreferences.swift`
- Modify: `mac/JawUI/Sources/Window/Prompts.swift` (the enum's doc comment, and after `deleteLayout(_:)`); `mac/JawApp/Sources/MainWindowController.swift` (its Layouts extension, after `askLayoutDelete()`); `mac/JawApp/Sources/AppModel.swift` (`keepKept` and its doc comment, which go; properties after Part A's `unopened`; `start()`; `stop()`; `apply(_:)`'s `.hello` case and last cases; `hear(_:)` after Part A's `lookedUp(_:_:)`; `moved(_:to:done:)`; and the "Keeping the panes" section from `struct Kept` to the end of `restorePanes()`); `mac/JawApp/Sources/Bench.swift` (`settled` and its doc comment; `quitting(_:model:)` after `settled`); `mac/JawApp/Sources/Launch.swift` (after `socket`, at 53-55); `mac/JawApp/Sources/main.swift`
- Test: `mac/Tests/JawUITests/LayoutPreferencesTests.swift`

**Interfaces:**
- Consumes: Tasks 1 to 3; Part A's `reconcile(focus:)`, `lookedUp(_:_:)`, `unopened` and `TerminalPanes`; the harness's `Prompts`.
- Produces:
  - In JawUI: `public struct LayoutPreferences`, with `public init(_ defaults: UserDefaults, home: String)`, `public func book() throws -> LayoutBook?`, `public func keep(_ book: LayoutBook)`, `public func held() throws -> LayoutHeld`, `public func hold(_ book: LayoutBook, v: UInt64)`, `public func release(through v: UInt64)` and `public func markSaid()`; its keys, each with the jaw folder's path after it, are `layouts:`, the path as typed, as earlier Jaws wrote it, and `layouts.held:`, the path standardized, and it reads the older `grid:` and `panes:` too. `Prompts.layouts(_ title: String, _ words: String) -> NSAlert`.
  - In the app: `MainWindowController.tellLayouts(_:_:) -> Bool`; `AppModel`'s `keeper`, `opening`, `keeperWakes`, `stopped`, `alerted`, `keptHere`, `kept(_:)`, `heldHere()`, `hear(_:)`, `run(_:)`, `tellLeftBehind(past:long:)`, `wakeKeeper()`, `take(_:)` and `savePanes(done:)`; `Launch.refusal: String?`; `Bench.quitting(_:model:)`, a snapshot's `quit:` line.

- [ ] **Step 1: Write the failing tests**

Create `mac/Tests/JawUITests/LayoutPreferencesTests.swift`:

```swift
import Foundation
@testable import JawUI
import Testing

/// What this Mac keeps of the layouts in its preferences, each test in preferences of its own,
/// removed after it: a book from before and an earlier build's panes read back, each jaw folder's
/// own, and kept as before with a jawd that keeps no books; what's held, one value written whole in
/// format 1; and a value this Jaw can't read, never written over.

/// Preferences of a test's own, and their name, for removing them after it.
func scratchPreferences() -> (UserDefaults, String) {
    let name = "jaw-mac-tests.\(UUID().uuidString)"
    return (UserDefaults(suiteName: name)!, name)
}

@Test func `a book kept before P10 reads back and is kept as before, and what an earlier build kept reads as the unnamed layout, each jaw folder's own`() throws {
    let (defaults, name) = scratchPreferences()
    defer { defaults.removePersistentDomain(forName: name) }
    let old = try earlier(["Work", "Family"], current: "Family")
    defaults.set(try JSONEncoder().encode(old), forKey: "layouts:/jaw/a")
    let here = LayoutPreferences(defaults, home: "/jaw/a")
    #expect(try here.book() == old)
    #expect(try LayoutPreferences(defaults, home: "/jaw/b").book() == nil)
    let work = try earlier(["Work"])
    here.keep(work)
    #expect(try here.book() == work, "with a jawd that keeps no books")
    let grid = PaneGrid.single("351912345678@s.whatsapp.net")
    defaults.set(try JSONEncoder().encode(LayoutPreferences.Kept(grid: grid, focus: 1)), forKey: "grid:/jaw/b")
    let panes = try #require(try LayoutPreferences(defaults, home: "/jaw/b").book())
    #expect(panes.grid == grid && panes.focus == 1 && panes.names.isEmpty, "mac-m1's grid, as the unnamed layout")
}

@Test func `what's held is one value, written whole in format 1: the book jawd didn't confirm, its version, and whether the alert has said, each folder's own`() throws {
    let (defaults, name) = scratchPreferences()
    defer { defaults.removePersistentDomain(forName: name) }
    let here = LayoutPreferences(defaults, home: "/jaw/a")
    #expect(try here.held() == LayoutHeld())
    let book = try earlier(["Work"], current: "Work")
    here.hold(book, v: 1_792_000_000_789)
    here.markSaid()
    #expect(try here.held() == LayoutHeld(book: book, v: 1_792_000_000_789, said: true))
    #expect(defaults.persistentDomain(forName: name)?.keys.sorted() == ["layouts.held:/jaw/a"], "one key")
    let written = try JSONSerialization.jsonObject(with: #require(defaults.data(forKey: "layouts.held:/jaw/a"))) as? [String: Any]
    #expect(written?["format"] as? Int == 1)
    #expect(try LayoutPreferences(defaults, home: "/jaw/./a/").held() == LayoutHeld(book: book, v: 1_792_000_000_789, said: true),
            "the folder's path standardized, however it's spelt")
    #expect(try LayoutPreferences(defaults, home: "/jaw/b").held() == LayoutHeld())
    here.release(through: 1_792_000_000_788)
    #expect(try here.held().book == book, "jawd keeps an older one, so it stays")
    here.release(through: 1_792_000_000_789)
    #expect(try here.held() == LayoutHeld(said: true), "jawd keeps it, so it goes, and that the alert said stays")
}

@Test func `a value here this Jaw can't read is never written over: a newer book, and a held value damaged, in another format, or with a key this Jaw doesn't know`() throws {
    let (defaults, name) = scratchPreferences()
    defer { defaults.removePersistentDomain(forName: name) }
    let newer = Data(#"{"v":2,"layouts":[]}"#.utf8)
    defaults.set(newer, forKey: "layouts:/jaw/a")
    let here = LayoutPreferences(defaults, home: "/jaw/a")
    #expect(throws: LayoutBook.Problem.version) { try here.book() }
    let work = try earlier(["Work"])
    here.keep(work)
    #expect(defaults.data(forKey: "layouts:/jaw/a") == newer, "the book left as it is")
    let unreadable = [
        Data("not a held value".utf8),
        Data(#"{"format":2,"v":0,"said":true}"#.utf8),
        Data(#"{"format":1,"v":0,"said":true,"pinned":"Work"}"#.utf8),
    ]
    for held in unreadable {
        defaults.set(held, forKey: "layouts.held:/jaw/a")
        #expect(throws: (any Error).self) { try here.held() }
        here.hold(work, v: 5)
        here.markSaid()
        here.release(through: 9)
        #expect(defaults.data(forKey: "layouts.held:/jaw/a") == held, "what's held left as it is")
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawUITests`
Expected: exit 1, with the build failing on `cannot find 'LayoutPreferences' in scope`.

- [ ] **Step 3: What this Mac keeps**

Create `mac/JawUI/Sources/Panes/LayoutPreferences.swift`:

```swift
import Foundation

/// What this Mac keeps of the layouts for a jaw folder, in its preferences, each under a key with
/// the folder's path after it: the book an earlier Jaw kept (`layouts:`), or what an earlier build
/// kept (`grid:`, the grid alone, or `panes:`, the tree), read as the unnamed layout, under the path
/// as it was typed, as earlier Jaws wrote them, which moves to jawd once and is written again only
/// while the preferences keep the layouts for a launch, as before P10; and what this Jaw holds
/// (`layouts.held:`, under the path standardized, so any spelling of it finds the same value), one
/// value written whole in one write. None of it is deleted, and a value here this Jaw can't read, a
/// newer Jaw's or a damaged one, is never written over.
public struct LayoutPreferences {
    public let defaults: UserDefaults
    public let home: String

    public init(_ defaults: UserDefaults, home: String) {
        self.defaults = defaults
        self.home = home
    }

    /// The book kept here, or what an earlier build kept as the unnamed layout; nil when nothing
    /// is. A book that can't be read throws.
    public func book() throws -> LayoutBook? {
        if let data = defaults.data(forKey: "layouts:" + home) {
            return try JSONDecoder().decode(LayoutBook.self, from: data)
        }
        var book = LayoutBook()
        if let data = defaults.data(forKey: "grid:" + home), let kept = try? JSONDecoder().decode(Kept.self, from: data) {
            book.update(kept.grid)
            book.focus = kept.focus
        } else if let data = defaults.data(forKey: "panes:" + home), let tree = try? JSONDecoder().decode(KeptTree.self, from: data) {
            let g = PaneGrid.from(tree.layout)
            book.update(g)
            book.focus = tree.focused.flatMap { g.slot(showing: $0)?.id }
        }
        return book.isEmpty ? nil : book
    }

    /// Keeps `book` here, as before P10, while the preferences keep the layouts; never over a book
    /// that can't be read.
    public func keep(_ book: LayoutBook) {
        if let old = defaults.data(forKey: "layouts:" + home), (try? JSONDecoder().decode(LayoutBook.self, from: old)) == nil {
            return
        }
        guard let data = try? JSONEncoder().encode(book) else { return }
        defaults.set(data, forKey: "layouts:" + home)
    }

    /// What this Jaw holds here, nothing when nothing is; a value that can't be read throws.
    public func held() throws -> LayoutHeld {
        guard let data = defaults.data(forKey: heldKey) else { return LayoutHeld() }
        return try JSONDecoder().decode(LayoutHeld.self, from: data)
    }

    /// Holds `book`, which jawd didn't confirm keeping, at version `v`, in place of what's held.
    public func hold(_ book: LayoutBook, v: UInt64) {
        change { held in
            held.book = book
            held.v = v
        }
    }

    /// Lets the book held here go, once jawd keeps it or a newer one, at version `v`.
    public func release(through v: UInt64) {
        change { held in
            guard held.book != nil, held.v <= v else { return }
            held.book = nil
            held.v = 0
        }
    }

    /// The alert has named the layouts of the book here that didn't move to jawd, or there were
    /// none.
    public func markSaid() {
        change { $0.said = true }
    }

    var heldKey: String {
        "layouts.held:" + URL(fileURLWithPath: home).standardizedFileURL.path
    }

    /// Changes what's held here and writes it whole, in one write, when it changed; never over a
    /// value that can't be read.
    func change(_ edit: (inout LayoutHeld) -> Void) {
        guard let was = try? held() else { return }
        var now = was
        edit(&now)
        guard now != was, let data = try? JSONEncoder().encode(now) else { return }
        defaults.set(data, forKey: heldKey)
    }

    /// The panes mac-m1 kept on a grid, before the named layouts.
    struct Kept: Codable {
        let grid: PaneGrid
        let focus: Int?
    }

    /// The tree mac-m1 kept before the grid.
    struct KeptTree: Codable {
        let layout: PaneLayout
        let focused: String?
    }
}
```

`Kept` and `KeptTree` are `AppModel`'s, moved here with the older keys they read; `AppModel`'s go in Step 5, with `keptKey`. Each write reads what's there first, so a value this Jaw can't read is left as it is whatever read it earlier in the launch.

- [ ] **Step 4: The alert**

In `mac/JawUI/Sources/Window/Prompts.swift`, the enum's doc comment becomes:

```swift
/// The window's alerts about its layouts, shown as sheets: a name to save one as or rename it to,
/// whether to delete one, and what jawd keeps of them.
```

and after `deleteLayout(_:)`, add:

```swift
    /// An alert saying something of the layouts, with its title and its words, and OK.
    public static func layouts(_ title: String, _ words: String) -> NSAlert {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = words
        alert.addButton(withTitle: "OK")
        return alert
    }
```

In `mac/JawApp/Sources/MainWindowController.swift`'s Layouts extension, after `askLayoutDelete()`, add:

```swift
    /// Says something of the layouts in an alert over the window; false with no window to say it in.
    @discardableResult
    func tellLayouts(_ title: String, _ words: String) -> Bool {
        guard let window else { return false }
        Prompts.layouts(title, words).beginSheetModal(for: window)
        return true
    }
```

- [ ] **Step 5: The model keeps its layouts where the keeper says**

In `mac/JawApp/Sources/AppModel.swift`, make eight changes.

`var keepKept = false` and its doc comment go: `LayoutPreferences` leaves a book it can't read as it is, whatever read it first.

After Part A's `var unopened`, add:

```swift
    /// Where the layouts are kept, and when they go to jawd.
    var keeper = LayoutKeeper()
    /// The book to open once the chat list has the chats it shows.
    var opening: LayoutBook?
    /// When the keeper is next woken, so only one wait at a time counts.
    var keeperWakes: Date?
    /// Whether `stop()` has run: it runs once.
    var stopped = false
    /// What the last alert over the window said of the layouts, for the snapshot's panes line.
    var alerted: String?
```

In `start()`, before `jawd.start()`, add the line below, so the launch's keeper knows what this Mac holds:

```swift
        keeper = LayoutKeeper(held: heldHere())
```

`stop()` becomes the version below. What the keeper answers while the app quits is dropped: from `quitting` on it takes nothing from jawd and asks nothing, and only `quit` says what to do.

```swift
    /// Lets each pane go, keeps what's on screen where the layouts are kept, waiting a second at
    /// most for jawd's answer and holding it on this Mac when jawd hasn't confirmed it, and closes
    /// the connection, once.
    func stop() {
        guard !stopped else { return }
        stopped = true
        for c in panes.values {
            c.leave()
        }
        var held: [LayoutKeeper.Step] = []
        if restored {
            book.focus = focusedSlot
            run(keeper.quitting(book, now: Date()))
            for update in jawd?.waitForKeeping(seconds: 1) ?? [] {
                switch update {
                case let .book(kept): _ = keeper.heard(kept, now: Date()) { nil }
                case let .bookPushed(kept): _ = keeper.pushed(kept, now: Date()) { nil }
                case let .bookRefused(why, keeping): _ = keeper.refused(why, keeping: keeping, now: Date()) { nil }
                default: break
                }
            }
            held = keeper.quit()
            run(held)
        }
        if launch.snapshot != nil { Bench.quitting(held, model: self) }
        jawd?.close()
        agent.stop()
    }
```

In `apply(_:)`, the `.hello` case ends with asking for the book, so after `window?.chatList.root = w.root` add:

```swift
            if launch.bench == nil { run(keeper.hello()) }
```

and the switch's last cases, from Part A's `.looked` on, become:

```swift
        case let .looked(chat, found):
            lookedUp(chat, found)
        case let .book(kept):
            hear { $0.heard(kept, now: Date(), preferences: $1) }
        case let .bookPushed(kept):
            if launch.bench == nil { hear { $0.pushed(kept, now: Date(), preferences: $1) } }
        case let .bookRefused(why, keeping):
            hear { $0.refused(why, keeping: keeping, now: Date(), preferences: $1) }
        case .push, .chats, .layout:
            break
```

After Part A's `lookedUp(_:_:)`, add the function below. It changes the keeper on a copy, so nothing the closure that reads the preferences' book reaches can meet `keeper` in the middle of a change, since Swift's exclusive access to a property being changed lasts the whole call. The keeper says what it reads there that can't be read, so reading says nothing itself.

```swift
    /// Tells the keeper what jawd said, on a copy while it may read the preferences' book, and does
    /// what it answers.
    func hear(_ said: (inout LayoutKeeper, () throws -> LayoutBook?) -> [LayoutKeeper.Step]) {
        var told = keeper
        let steps = said(&told) { try keptHere?.book() }
        keeper = told
        run(steps)
    }
```

`moved(_:to:done:)`, with its doc comment, becomes the version below, so the keeper hears every move of a line and keeps it two seconds after the last, and jawd's answer to an earlier keep is never taken over a line still being dragged, while the preferences, with a jawd that keeps no books, are written once the drag ends, as main writes them:

```swift
    /// A gutter dragged: the line moves live, and each move goes to the keeper, which keeps the line
    /// once it has stayed where it is for two seconds, or in the preferences once the drag ends.
    func moved(_ line: PaneGrid.Line, to position: Double, done: Bool) {
        grid = grid?.moving(line, to: position, size: gridSize)
        savePanes(done: done)
    }
```

In "Keeping the panes", from `struct Kept` to the end of `restorePanes()`, the section becomes the code below: `Kept`, `KeptTree` and `keptKey` give way to `LayoutPreferences`, which reads the same keys.

```swift
    /// What this Mac keeps of the layouts for the jaw folder.
    lazy var keptHere: LayoutPreferences? = Self.kept(launch)

    /// Where this Mac keeps the layouts: in the app's preferences; nowhere for a snapshot, whose
    /// pictures don't depend on what an earlier run left.
    static func kept(_ launch: Launch) -> LayoutPreferences? {
        launch.snapshot == nil ? LayoutPreferences(.standard, home: launch.home) : nil
    }

    /// What this Mac holds of the layouts, for the launch's keeper; what can't be read is said, and
    /// left as it is.
    func heldHere() -> LayoutHeld {
        do {
            return try keptHere?.held() ?? LayoutHeld()
        } catch {
            say("what this Mac holds of the layouts couldn't be read: \((error as? LayoutBook.Problem)?.said ?? "\(error)"); it's left as it is, and nothing more is held there",
                warn: true)
            return LayoutHeld()
        }
    }

    /// Keeps the book on screen where the layouts are kept: by jawd once it has stayed as it is for
    /// two seconds, or in the preferences at once, as before P10, a dragged line once it's `done`.
    func savePanes(done: Bool = true) {
        guard restored else { return }
        book.focus = focusedSlot
        run(keeper.changed(book, now: Date(), done: done))
    }

    /// Does what the keeper says, then wakes it when it next has something to do.
    func run(_ steps: [LayoutKeeper.Step]) {
        for step in steps {
            switch step {
            case let .open(new) where restored:
                take(new)
            case let .open(new):
                opening = new
                restorePanes()
            case let .follow(answered) where restored:
                book.focus = focusedSlot
                let followed = book.following(answered)
                if followed != book { take(followed) }
            case .follow:
                break
            case .ask:
                jawd?.book()
            case let .keep(kept, v):
                jawd?.keep(kept.forJawd, v: v)
            case .writePreferences:
                book.focus = focusedSlot
                keptHere?.keep(book)
            case let .hold(held, v):
                keptHere?.hold(held, v: v)
            case let .release(v):
                keptHere?.release(through: v)
            case let .leftBehind(past, long):
                tellLeftBehind(past: past, long: long)
            case let .say(text, warn):
                say(text, warn: warn)
            case let .tell(title, words):
                alerted = title
                window?.tellLayouts(title, words)
            }
        }
        wakeKeeper()
    }

    /// Names the preferences' layouts that didn't move to jawd in an alert, and keeps that it has
    /// once the alert is up, or at once when there are none to name.
    func tellLeftBehind(past: [String], long: [String]) {
        if !(past + long).isEmpty {
            guard window?.tellLayouts(LayoutKeeper.leftTitle, LayoutKeeper.left(past: past, long: long)) == true else { return }
            alerted = LayoutKeeper.listed(past + long)
        }
        keptHere?.markSaid()
    }

    /// Wakes the keeper when it next has something to do, a change to keep or jawd's book to ask
    /// for again: one wait at a time, for the soonest.
    func wakeKeeper() {
        guard let due = keeper.due, keeperWakes.map({ due < $0 }) ?? true else { return }
        keeperWakes = due
        DispatchQueue.main.asyncAfter(deadline: .now() + max(due.timeIntervalSinceNow, 0)) { [weak self] in
            guard let self, keeperWakes == due else { return }
            keeperWakes = nil
            run(keeper.fire(now: Date()))
        }
    }

    /// Opens the book where the layouts are kept, once jawd has said which and the chat list has the
    /// chats it shows; the keeper counts changes from here.
    func restorePanes() {
        guard !restored, let opening, !(window?.chatList.all.isEmpty ?? true) else { return }
        restored = true
        self.opening = nil
        keeper.opened()
        take(opening)
    }

    /// A book in place of what's on screen, as jawd keeps it or the preferences kept it, its panes
    /// reconciled as switching does.
    func take(_ new: LayoutBook) {
        conversation?.leave()
        book = new
        if let grid, !grid.holds {
            window?.layoutsChanged()
            return
        }
        reconcile(focus: book.focus)
    }
```

What this keeps from before: `restorePanes()` is still called at the chat list's first rows, and now also once the keeper says what to open, whichever comes second; the preferences' three keys are read as they were, by `LayoutPreferences` now, a book there that can't be read said as before, by the keeper now; and `savePanes()` keeps its callers, which are every change the parity plan names: a chat put in a pane (`open`, `drop`), a shape edited (`split`, `close(slot:)`, `swap`, and `moved(_:to:done:)`, now at every move, the preferences still written once the drag ends), saved or reverted (`saveLayout`, and `revertLayout` through `reconcile`), a layout made, renamed or deleted (`saveLayoutAs`, `applyEditor`, `renameLayout`, `deleteLayout`), and another switched to (`switchLayout` and Part A's `terminalPanes`, through `reconcile`). What changes: the restore waits for jawd's answer, a snapshot restores from jawd too, `--bench`, which never asks, never restores, and the preferences' book is written only in the keeper's `preferences` place. `keepKept` left a book this Jaw can't read as it was only once a read that failed had set it; `LayoutPreferences` checks before each write instead, so no way of writing can miss it.

- [ ] **Step 6: A snapshot runs only on a folder of its own, waits for the layouts, and says what it held**

In `mac/JawApp/Sources/Launch.swift`, after `socket`, add:

```swift
    /// Why the app won't run as asked, if it won't: a snapshot keeps the layouts it makes in jawd,
    /// so it runs only on a folder of its own, not jaw's own or one holding an engine's session, a
    /// real account's, as `jawd demo` refuses for its folder.
    var refusal: String? {
        guard snapshot != nil else { return nil }
        let path = { (folder: String) in URL(fileURLWithPath: folder).resolvingSymlinksInPath().path }
        let account = path(home) == path(Self.platformHome)
            || FileManager.default.fileExists(atPath: (path(home) as NSString).appendingPathComponent("session.db"))
        guard account else { return nil }
        return "a snapshot keeps the layouts it makes in jawd, so it needs a folder of its own, not \(home), which holds an account; "
            + "give it one with --home, or --demo"
    }
```

`mac/JawApp/Sources/main.swift` becomes:

```swift
import AppKit

let launch = Launch.parse(CommandLine.arguments)
if let refusal = launch.refusal {
    FileHandle.standardError.write(Data("Jaw: \(refusal)\n".utf8))
    exit(1)
}
let app = NSApplication.shared
let delegate = AppDelegate(launch: launch)
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
```

In `mac/JawApp/Sources/Bench.swift`, `settled`'s doc comment and its first test become, so a snapshot's chat opens after the book jawd keeps, rather than before it and under it:

```swift
    /// Runs `then` once the chat list's rows have stayed as they are for half a second and the
    /// layouts are open, or after five seconds whatever they do: a fresh demo's messages and its
    /// group's name come over its first moments, so a picture taken at the first rows would catch
    /// some of them and not others.
```

```swift
        if (quiet >= 5 && model.restored) || looked >= 50 { return then() }
```

and after `settled`, add:

```swift
    /// Says, as the snapshot quits, where the layouts are kept and what this Mac holds of them; a
    /// snapshot with no preferences to hold in holds nothing, and says so.
    static func quitting(_ held: [LayoutKeeper.Step], model: AppModel) {
        var said = "nothing held"
        if case let .hold(_, v)? = held.first {
            let why = model.keeper.unanswered ? "didn't answer" : "refused it"
            said = model.keptHere == nil
                ? "jawd \(why), and this snapshot has no preferences to hold it in, so what's on screen at v\(v) isn't kept"
                : "jawd \(why), so this Mac holds what's on screen at v\(v)"
        }
        FileHandle.standardError.write(Data("quit: layouts \(model.keeper.place.rawValue), \(said)\n".utf8))
    }
```

- [ ] **Step 7: Build and test**

Run: `mac/scripts/build-mac build`, then `mac/scripts/build-mac test JawUITests`, `mac/scripts/build-mac test JawHarnessTests` and `mac/scripts/build-mac test JawKit`.
Expected: each exits 0, and JawUITests passes with three more tests than after Task 3, the three of `LayoutPreferencesTests`.

- [ ] **Step 8: A snapshot still opens its chat, and one on jaw's own folder is refused**

With jawd copied (see the Global Constraints), run:

```sh
mkdir -p mac/build/book
mac/scripts/build-mac snapshot mac/build/book/app.png --keys cmd+d
```

Expected: `cmd+d: 1:<six>@0-1x0-1 2:<six>@1-2x0-1 · focus 2 · layout unsaved · panes …×… …×…`, each `<six>` the first six characters of a chat's JID and each `…×…` a pane's size, then `{"panes":2.0,"snapshot":1.0}` and `quit: layouts jawd, nothing held`. Look at `mac/build/book/app.png` with the Read tool: two panes, each with its chat's name in its header and its messages. The demo's folder is new, so jawd kept nothing for the app and the snapshot's first chat opened in a pane of its own, as before; at quitting the app kept what was on screen, and jawd kept it within its second.

Then run the app on the build Mac's own jaw folder, which it refuses before it starts anything:

```sh
ssh -o BatchMode=yes build-mac 'cd ~/src/build-mac/jaw/jaw-panes/mac && build/DerivedData/Build/Products/Debug/Jaw.app/Contents/MacOS/Jaw --home "$HOME/Library/Application Support/jaw" --snapshot /dev/null; echo "exit $?"'
```

Expected: `Jaw: a snapshot keeps the layouts it makes in jawd, so it needs a folder of its own, not /Users/…/Library/Application Support/jaw, which holds an account; give it one with --home, or --demo`, then `exit 1`.

- [ ] **Step 9: Commit**

```bash
git add mac/JawUI/Sources/Panes/LayoutPreferences.swift mac/Tests/JawUITests/LayoutPreferencesTests.swift \
    mac/JawUI/Sources/Window/Prompts.swift mac/JawApp/Sources/MainWindowController.swift mac/JawApp/Sources/AppModel.swift \
    mac/JawApp/Sources/Bench.swift mac/JawApp/Sources/Launch.swift mac/JawApp/Sources/main.swift
git commit -F- <<'EOF'
The Mac app keeps its layouts in jawd, as LayoutKeeper says: it asks for its book at each hello and opens it once the chat list has its chats, moves its preferences' book there the first time jawd keeps nothing and names once, in an alert, the layouts that stay behind, keeps each change with layout.set once it has stayed for two seconds, a dragged line's every move counted, and at once when it quits, waiting a second at most for jawd's answer and holding on this Mac what jawd didn't confirm, as it holds a change made while jawd is away, follows a newer book another of its connections keeps, and with a jawd from before P10 keeps the layouts in the preferences as before, a dragged line written once its drag ends; LayoutPreferences, in JawUI, reads the preferences' book and what an earlier build kept, and holds one value per jaw folder written whole in format 1, under the folder's standardized path, the book jawd didn't confirm, its version and whether the alert has said, never writing over what it can't read, a value in another format or with a key it doesn't know included; a snapshot opens its chat once the layouts are open, says as it quits what it held, and is refused on jaw's own folder or one with an engine's session, since it keeps what it makes in jawd; tests of those preferences, each in preferences of its own
EOF
```

---

### Task 5: The snapshot says where the layouts are kept, and the live check on jawd demo

**Files:**
- Modify: `mac/JawApp/Sources/Bench.swift` (`press`); `mac/JawApp/Sources/Launch.swift` (`Launch` and `parse(_:environment:)`); `mac/JawApp/Sources/AppModel.swift` (`kept(_:)`)
- Scratch: `mac/build/book/try-layout-book.sh`

**Interfaces:**
- Consumes: Task 4's `AppModel.keeper`, `alerted`, `kept(_:)` and `Bench.quitting(_:model:)`; `LayoutKeeper.place`, `unanswered` and `asked` (Task 3).
- Produces: the snapshot's `wait:<seconds>` step; its panes lines' `alert "…"`, after the status line's `said`, and their last field, `layouts <place>`, with `, unanswered` while a keep waits for jawd's answer and ` at v<version>` while that keep is on its way; and `--layouts-in <name>`, `Launch.layoutsIn`, taken only with `--snapshot` and for a name starting `Launch.checkPreferences`, `dev.badcoffee.jaw.check.`.

- [ ] **Step 1: See the step missing**

Run:

```sh
mac/scripts/build-mac snapshot mac/build/book/before.png --keys wait:3
```

Expected: a line `wait:3: 1:<six>@0-1x0-1 · focus 1 · layout unsaved · panes …×…`, with no `layouts` field: the step isn't one yet, so its name was typed as keys.

- [ ] **Step 2: The step and the fields**

In `mac/JawApp/Sources/Bench.swift`, in `press(_:model:then:)`, make four changes.

`step` becomes:

```swift
            let step = key.hasPrefix("save:") || key == "revert" || key == "settings" || key.hasPrefix("size:") || key.hasPrefix("drop:")
                || key.hasPrefix("menu:") || key == "terminal-panes" || key.hasPrefix("wait:")
```

The wait after each key becomes the step's own for `wait:`, so `DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {` becomes:

```swift
            DispatchQueue.main.asyncAfter(deadline: .now() + (key.hasPrefix("wait:") ? Double(key.dropFirst(5)) ?? 0.3 : 0.3)) {
```

The panes line says an alert the app showed since the line before, once, so the line that makes `said` becomes:

```swift
                    let said = palette + glance + (model.notice.map { " · said \"\($0.0)\"" } ?? "") + (model.alerted.map { " · alert \"\($0)\"" } ?? "")
                        + " · panes " + model.panes.values.map { "\(Int($0.view.frame.width))×\(Int($0.view.frame.height))" }.joined(separator: " ")
                    model.alerted = nil
```

And it ends with where the layouts are kept, whether a keep waits for jawd's answer, and the version a keep on its way was asked at, so the line that writes it becomes:

```swift
                    let waits = (model.keeper.unanswered ? ", unanswered" : "") + (model.keeper.asked.map { " at v\($0)" } ?? "")
                    FileHandle.standardError.write(Data("\(key): \(grid) · focus \(model.focusedSlot ?? 0) · layout \(model.book.current ?? "unsaved")\(model.book.isEdited ? " •" : "")\(said) · layouts \(model.keeper.place.rawValue)\(waits)\n".utf8))
```

- [ ] **Step 3: Preferences of the check's own**

In `mac/JawApp/Sources/Launch.swift`, add these after `snapshot`:

```swift
    /// `--layouts-in <name>`, with `--snapshot` and a name starting `checkPreferences`: the
    /// preferences the snapshot reads and holds the layouts in, as the live check seeds them; nil
    /// otherwise, so a snapshot touches no preferences.
    let layoutsIn: String?

    /// What the names of the live checks' own preferences start with.
    static let checkPreferences = "dev.badcoffee.jaw.check."
```

In `parse(_:environment:)`, declare `var layoutsIn: String?` after `var snapshot: String?`, add this case after `--snapshot`'s:

```swift
            case "--layouts-in" where i + 1 < arguments.count:
                layoutsIn = arguments[i + 1]
                i += 1
```

and make its last line two:

```swift
        if snapshot == nil || !(layoutsIn?.hasPrefix(checkPreferences) ?? false) { layoutsIn = nil }
        return Launch(home: home ?? platformHome, demo: demo, bench: bench, snapshot: snapshot, layoutsIn: layoutsIn)
```

In `mac/JawApp/Sources/AppModel.swift`, `kept(_:)`, with its doc comment, becomes:

```swift
    /// Where this Mac keeps the layouts: in the app's preferences; for a snapshot, whose pictures
    /// don't depend on what an earlier run left, in the check's own preferences that `--layouts-in`
    /// names, or nowhere.
    static func kept(_ launch: Launch) -> LayoutPreferences? {
        if let name = launch.layoutsIn, let defaults = UserDefaults(suiteName: name) {
            return LayoutPreferences(defaults, home: launch.home)
        }
        return launch.snapshot == nil ? LayoutPreferences(.standard, home: launch.home) : nil
    }
```

- [ ] **Step 4: Build**

Run: `mac/scripts/build-mac build`, then `mac/scripts/build-mac test JawUITests`.
Expected: both exit 0, JawUITests with as many tests as after Task 4.

- [ ] **Step 5: The live check**

This check runs on the build Mac against `jawd demo` on a folder of its own, with preferences of the check's own seeded with a book an earlier Jaw kept: twelve layouts, each one pane on the demo's chat 1, a name of 69 bytes ninth, and L11 on screen. The list's first row, the chat a snapshot opens before its keys, is chat 0, the demo's newest, which also takes a live message every 100 ms while its history goes in (`LIVE_EVERY` in `crates/jawd/src/demo/mod.rs`); the demo pins nothing without `--first-run`, which the check doesn't pass, and the seeded layouts don't show chat 0. The first run moves the book to jawd. The second is the relaunch, step 1.1's "Done when" on a made-up account: it opens what jawd kept and edits L1, which the script sees jawd keep before it stops jawd; then it splits again and drops the list's first row into the new pane, a change whose keep jawd never answers, since jawd is stopped and then killed before it can read it, so quitting holds it. The third, on a jawd started again on that folder, moves the held book to jawd and changes nothing else, since the chat the snapshot opens first is already in the pane it focuses. The script prints jawd's version, the held value's and that of the keep jawd never answered, and checks each claim, printing `ok:` or `FAIL:`; since a panes line shows only the first six characters of each chat's JID, which most of the demo's share, the last claim compares the book jawd ends with to the one this Mac held, whole.

Write `mac/build/book/try-layout-book.sh`:

```sh
#!/bin/sh
# The live check of the layouts kept by jawd, on the build Mac: jawd demo on a folder of its own and
# preferences of the check's own seeded with a book an earlier Jaw kept, each claim printed as ok or
# FAIL. Three runs of the app: the move to jawd and its alert; the relaunch, an edit jawd keeps, and
# a change it never answers, since it's stopped and then killed before it reads it, held at quitting;
# and the next launch, on a jawd started again on that folder, moving the held book to jawd and
# changing nothing else. One scratch folder and one set of preferences, removed on the way out.
set -eu
command -v python3 > /dev/null || { echo "the build Mac has no python3" >&2; exit 1; }
cd ~/src/build-mac/jaw/jaw-panes/mac
scratch=$(mktemp -d "${TMPDIR:?}/jaw-book.XXXXXX")
home="$scratch/jaw"
prefs="dev.badcoffee.jaw.check.${scratch##*.}"
app=build/DerivedData/Build/Products/Debug/Jaw.app/Contents/MacOS/Jaw
long="Lunch on Sunday with the whole family round the kitchen table at home"
demo=""
run=""
failed=0

# Stops the jawd demo the script started and waits for it to go: with -KILL at once, paused or not,
# so it reads nothing more; otherwise as Ctrl-C would.
unserve() {
    if [ "${1:-}" = -KILL ]; then
        kill -KILL "$demo" 2> /dev/null || true
    else
        kill -CONT "$demo" 2> /dev/null || true
        kill "$demo" 2> /dev/null || true
    fi
    wait "$demo" 2> /dev/null || true
    demo=""
}
# Writes nothing: it may run once the ssh session has gone, when a write would raise PIPE.
cleanup() {
    if [ -n "$run" ]; then
        kill "$run" 2> /dev/null || true
        wait "$run" 2> /dev/null || true
    fi
    if [ -n "$demo" ]; then unserve -KILL; fi
    defaults delete "$prefs" > /dev/null 2>&1 || true
    rm -rf "$scratch" > /dev/null 2>&1 || true
}
trap cleanup EXIT
# `ssh ... 'sh -s'` gives the script no terminal, so a dropped session sends no HUP: the script
# finds out at its next write back, which raises PIPE.
trap 'exit 1' HUP INT TERM PIPE

# jawd demo on the folder, the same made-up account each time it starts there, once it serves.
serve() {
    ../target/debug/jawd demo --folder "$home" --chats 40 --people 60 --messages 800 --blob 200 > "$scratch/demo.log" 2>&1 &
    demo=$!
    tries=0
    until grep -q 'serving until Ctrl-C' "$scratch/demo.log"; do
        tries=$((tries + 1))
        if [ "$tries" -gt 600 ] || ! kill -0 "$demo" 2> /dev/null; then
            tail -5 "$scratch/demo.log" >&2
            exit 1
        fi
        sleep 0.2
    done
}

# Prints "ok: $1" when the command after it succeeds, and "FAIL: $1" otherwise.
claim() {
    what=$1
    shift
    if "$@" > /dev/null 2>&1; then echo "ok: $what"; else echo "FAIL: $what"; failed=1; fi
}
absent() {
    ! grep -q "$@"
}
# The version after "$1 v=" in file $2.
version() {
    sed -n "s/^$1 v=\([0-9]*\).*/\1/p" "$2"
}

# What jawd keeps for the app, asked on a connection of its own: its version, and the book, which
# also goes as JSON to the file given, if one is.
asked() {
    python3 - "$home/jaw.sock" "${1:-/dev/null}" << 'PY'
import json, socket, sys

connection = socket.socket(socket.AF_UNIX)
connection.connect(sys.argv[1])
lines = connection.makefile("rw")

def ask(id, method, params):
    lines.write(json.dumps({"id": id, "method": method, "params": params}) + "\n")
    lines.flush()
    while True:
        line = json.loads(lines.readline())
        if line.get("id") == id:
            if "error" in line:
                sys.exit(f"{method}: {line['error']['message']}")
            return line["result"]

def shown(book):
    on = next((l for l in book.get("layouts", []) if l["name"] == book.get("current")), None)
    if on is None:
        return f"the unsaved panes, {len(book['unnamed']['slots']) if book.get('unnamed') else 0} panes"
    grid = on.get("edited") or on["grid"]
    return f"{on['name']}{', edited' if 'edited' in on else ''}, {len(grid['slots'])} panes"

ask(1, "hello", {"protocol": 1, "client": "jaw-probe", "version": "0.1.0", "focus": False})
kept = ask(2, "layout.get", {"interface": "jaw-mac"})
book = kept.get("layout")
with open(sys.argv[2], "w") as f:
    json.dump(book, f)
if book is None:
    print(f"jawd keeps v={kept['v']} for jaw-mac, and no layout")
else:
    named = ", ".join(l["name"] for l in book.get("layouts", [])) or "no named layouts"
    print(f"jawd keeps v={kept['v']} for jaw-mac, format {book['format']}: {named}; on screen {shown(book)}")
PY
}

# What the check's preferences keep for the folder: the book an earlier Jaw kept, under the path as
# typed, as seeded or not; and what this Jaw holds, under the path standardized, as the app keys it,
# with its version and whether the alert has said, its book also going as JSON to the file given, if
# one is. With TMPDIR ending in a slash, as macOS's does, the path has a doubled one, so the two keys
# differ and the held value is found only if the app standardized its key.
here() {
    python3 - "$prefs" "$home" "$scratch/seed.json" "${1:-/dev/null}" << 'PY'
import json, os, plistlib, subprocess, sys

prefs, home, seed, saved = sys.argv[1:]
out = subprocess.run(["defaults", "export", prefs, "-"], capture_output=True)
kept = plistlib.loads(out.stdout) if out.returncode == 0 else {}
book = kept.get(f"layouts:{home}")
with open(seed, "rb") as f:
    seeded = f.read()
if book is None:
    print("the preferences' book: none")
else:
    print(f"the preferences' book: {len(json.loads(book)['layouts'])} layouts, {'as seeded' if book == seeded else 'changed'}")
key = "layouts.held:" + os.path.normpath(home)
held = json.loads(kept[key]) if key in kept else {}
with open(saved, "w") as f:
    json.dump(held.get("book"), f)
if held.get("book"):
    on = held["book"]
    grid = on.get("unnamed") if on.get("current") is None else next(l.get("edited") or l["grid"] for l in on["layouts"] if l["name"] == on["current"])
    print(f"held: v={held['v']}, {len(on['layouts'])} layouts, {len(grid['slots'])} panes on screen")
else:
    print("held: nothing")
print(f"said: {'yes' if held.get('said') else 'no'}")
PY
}

# Whether the book jawd keeps, as `asked` saved it in $1, is the one this Mac held, as `here` saved
# it in $2: the same JSON, but for jawd's `format` where the app has its `v`.
same() {
    python3 - "$1" "$2" << 'PY'
import json, sys

def read(path):
    with open(path) as f:
        return json.load(f)

jawd, held = read(sys.argv[1]), read(sys.argv[2])
jawd.pop("format")
held.pop("v")
sys.exit(jawd != held)
PY
}

python3 - "$prefs" "$home" "$scratch/seed.json" "$long" << 'PY'
import json, plistlib, subprocess, sys

prefs, home, seed, long = sys.argv[1:]
# One pane on jawd demo's chat 1, a person's, in every layout: not chat 0, the list's first row,
# which the relaunch drops into L1's third pane.
grid = {"cols": [0, 1], "rows": [0, 1], "slots": [{"id": 1, "c0": 0, "c1": 1, "r0": 0, "r1": 1, "chat": "351900000001@s.whatsapp.net"}]}
names = [f"L{i}" for i in range(1, 9)] + [long, "L10", "L11", "L12"]
book = json.dumps({"v": 1, "current": "L11", "unnamed": grid, "unnamedFocus": 1,
                   "layouts": [{"name": n, "grid": grid, "focus": 1} for n in names]}).encode()
with open(seed, "wb") as f:
    f.write(book)
with open(seed + ".plist", "wb") as f:
    plistlib.dump({f"layouts:{home}": book}, f)
subprocess.run(["defaults", "import", prefs, seed + ".plist"], check=True)
PY
serve

echo "== the move"
"$app" --home "$home" --layouts-in "$prefs" --snapshot build/book-1.png --keys wait:3 < /dev/null > build/book-1.log 2>&1 || true
grep -E '^(wait:3|quit):|^\{' build/book-1.log
asked | tee "$scratch/1"
here | tee -a "$scratch/1"
claim "jawd keeps the first nine whose names fit, with the unsaved panes on screen in place of L11" \
    grep -q '^jawd keeps v=[0-9]* for jaw-mac, format 2: L1, L2, L3, L4, L5, L6, L7, L8, L10; on screen the unsaved panes, 1 panes$' "$scratch/1"
claim "the alert named the three that stayed" grep -q "^wait:3: .* · alert \"L11, L12 and $long\" · panes .* · layouts jawd\$" build/book-1.log
claim "and this Mac keeps that it has said them" grep -q '^said: yes$' "$scratch/1"
claim "the preferences' book is as it was seeded" grep -q "^the preferences' book: 12 layouts, as seeded$" "$scratch/1"

echo "== the relaunch, an edit jawd keeps, and a change it never answers"
"$app" --home "$home" --layouts-in "$prefs" --snapshot build/book-2.png \
    --keys wait:2,ctrl+cmd+1,cmd+d,wait:3,wait:2,cmd+d,drop:0:middle,wait:4 < /dev/null > build/book-2.log 2>&1 &
run=$!
# jawd is stopped half a second after it's seen keeping L1's edit, so its answer has gone to the app,
# and long before the app asks it to keep the next change.
tries=0
until asked > "$scratch/kept" 2>&1 && grep -q 'on screen L1, edited, 2 panes$' "$scratch/kept"; do
    tries=$((tries + 1))
    if [ "$tries" -gt 50 ] || ! kill -0 "$run" 2> /dev/null; then break; fi
    sleep 0.2
done
sleep 0.5
kill -STOP "$demo"
wait "$run" || true
run=""
unserve -KILL
grep -E '^(wait:[0-9]|ctrl\+cmd\+1|cmd\+d|drop:0:middle|quit):|^\{' build/book-2.log
sed 's/^/before it was stopped: /' "$scratch/kept"
serve
asked | tee "$scratch/2"
here "$scratch/2.held" | tee -a "$scratch/2"
v1=$(version "jawd keeps" "$scratch/kept")
v2=$(sed -n 's/^wait:4: .* unanswered at v\([0-9]*\)$/\1/p' build/book-2.log)
claim "the relaunch opened the book jawd kept, L1 in it to switch to" \
    grep -q '^ctrl+cmd+1: 1:[0-9]*@0-1x0-1 · focus 1 · layout L1 · panes [0-9]*×[0-9]* · layouts jawd$' build/book-2.log
claim "with no alert, since it had said" absent ' · alert ' build/book-2.log
claim "jawd kept L1's edit, two panes, before it was stopped" grep -q 'on screen L1, edited, 2 panes$' "$scratch/kept"
claim "and the app had its answer" \
    grep -q '^wait:3: 1:[0-9]*@0-1x0-1 2:[0-9]*@1-2x0-1 · focus 2 · layout L1 • · .* · layouts jawd$' build/book-2.log
claim "the next change's keep waited for an answer jawd never gave" \
    grep -q '^wait:4: 1:[0-9]*@0-1x0-1 2:[0-9]*@1-2x0-1 3:[0-9]*@2-3x0-1 · focus 3 · layout L1 • · .* · layouts jawd, unanswered at v[0-9]*$' build/book-2.log
claim "quitting held what was on screen at that keep's version, saying jawd didn't answer" \
    grep -q "^quit: layouts jawd, jawd didn't answer, so this Mac holds what's on screen at v$v2\$" build/book-2.log
claim "jawd, started again, keeps L1's edit as it kept it" grep -q "^jawd keeps v=$v1 for jaw-mac, format 2: .*; on screen L1, edited, 2 panes\$" "$scratch/2"
claim "which is older than the book held" [ "$v1" -lt "$v2" ]
claim "this Mac holds the book with L1's three panes, at that keep's version" grep -q "^held: v=$v2, 9 layouts, 3 panes on screen\$" "$scratch/2"

echo "== the next launch"
"$app" --home "$home" --layouts-in "$prefs" --snapshot build/book-3.png --keys size:1440x900 < /dev/null > build/book-3.log 2>&1 || true
grep -E '^(size:1440x900|quit):|^\{' build/book-3.log
asked "$scratch/3.book" | tee "$scratch/3"
here | tee -a "$scratch/3"
v3=$(version "jawd keeps" "$scratch/3")
claim "the next launch opened the held book, L1 with its three panes" \
    grep -q '^size:1440x900: 1:[0-9]*@0-1x0-1 2:[0-9]*@1-2x0-1 3:[0-9]*@2-3x0-1 · focus 3 · layout L1 • · .* · layouts jawd$' build/book-3.log
claim "with no alert" absent ' · alert ' build/book-3.log
claim "jawd keeps it now, past the version it was held at" [ "$v3" -gt "$v2" ]
claim "with its three panes" grep -q 'on screen L1, edited, 3 panes$' "$scratch/3"
claim "and it's the book this Mac held, every layout and chat as it was" same "$scratch/3.book" "$scratch/2.held"
claim "this Mac holds nothing now" grep -q '^held: nothing$' "$scratch/3"
claim "and the preferences' book is still as it was seeded" grep -q "^the preferences' book: 12 layouts, as seeded$" "$scratch/3"
exit "$failed"
```

Run it on the build Mac after Step 4's build, which copied the checkout and built the app there, then copy its pictures back:

```sh
ssh -o BatchMode=yes build-mac 'sh -s' < mac/build/book/try-layout-book.sh
for p in book-1 book-3; do
    scp -q -o BatchMode=yes "build-mac:src/build-mac/jaw/jaw-panes/mac/build/$p.png" "mac/build/book/$p.png"
done
```

Expected, with P10's jawd, exit 0 and the lines below, each part's followed by an `ok:` line for each of its claims and no `FAIL:`. Each `<six>` is the first six characters of a chat's JID, each `…×…` a pane's size, `<long>` the 69-byte name, and `<v0>` to `<v3>` versions, `<v1>` less than `<v2>` less than `<v3>`:

```
== the move
wait:3: 1:<six>@0-1x0-1 · focus 1 · layout unsaved · alert "L11, L12 and <long>" · panes …×… · layouts jawd
{"panes":1.0,"snapshot":1.0}
quit: layouts jawd, nothing held
jawd keeps v=<v0> for jaw-mac, format 2: L1, L2, L3, L4, L5, L6, L7, L8, L10; on screen the unsaved panes, 1 panes
the preferences' book: 12 layouts, as seeded
held: nothing
said: yes
== the relaunch, an edit jawd keeps, and a change it never answers
wait:2: 1:<six>@0-1x0-1 · focus 1 · layout unsaved · panes …×… · layouts jawd
ctrl+cmd+1: 1:<six>@0-1x0-1 · focus 1 · layout L1 · panes …×… · layouts jawd
cmd+d: 1:<six>@0-1x0-1 2:<six>@1-2x0-1 · focus 2 · layout L1 • · panes …×… …×… · layouts jawd
wait:3: 1:<six>@0-1x0-1 2:<six>@1-2x0-1 · focus 2 · layout L1 • · panes …×… …×… · layouts jawd
wait:2: 1:<six>@0-1x0-1 2:<six>@1-2x0-1 · focus 2 · layout L1 • · panes …×… …×… · layouts jawd
cmd+d: 1:<six>@0-1x0-1 2:<six>@1-2x0-1 3:<six>@2-3x0-1 · focus 3 · layout L1 • · panes …×… …×… …×… · layouts jawd
drop:0:middle: 1:<six>@0-1x0-1 2:<six>@1-2x0-1 3:<six>@2-3x0-1 · focus 3 · layout L1 • · panes …×… …×… …×… · layouts jawd
wait:4: 1:<six>@0-1x0-1 2:<six>@1-2x0-1 3:<six>@2-3x0-1 · focus 3 · layout L1 • · panes …×… …×… …×… · layouts jawd, unanswered at v<v2>
{"panes":3.0,"snapshot":1.0}
quit: layouts jawd, jawd didn't answer, so this Mac holds what's on screen at v<v2>
before it was stopped: jawd keeps v=<v1> for jaw-mac, format 2: L1, L2, L3, L4, L5, L6, L7, L8, L10; on screen L1, edited, 2 panes
jawd keeps v=<v1> for jaw-mac, format 2: L1, L2, L3, L4, L5, L6, L7, L8, L10; on screen L1, edited, 2 panes
the preferences' book: 12 layouts, as seeded
held: v=<v2>, 9 layouts, 3 panes on screen
said: yes
== the next launch
size:1440x900: 1:<six>@0-1x0-1 2:<six>@1-2x0-1 3:<six>@2-3x0-1 · focus 3 · layout L1 • · panes …×… …×… …×… · layouts jawd
{"panes":3.0,"snapshot":1.0}
quit: layouts jawd, nothing held
jawd keeps v=<v3> for jaw-mac, format 2: L1, L2, L3, L4, L5, L6, L7, L8, L10; on screen L1, edited, 3 panes
the preferences' book: 12 layouts, as seeded
held: nothing
said: yes
```

How to read it:

- **The move.** jawd kept nothing for the app, so the seeded book moved: L1 to L8 and L10, the first nine whose names fit, the 69-byte name staying with L11 and L12, past the nine. L11 was on screen, so the unsaved panes are, the snapshot's first chat in their pane. The alert named the three, as its field on the first line shows, the alert itself being a sheet the picture doesn't draw; the held value says it has, and the preferences' book is byte for byte as seeded. The check's folder path has a doubled slash, from TMPDIR's trailing one, and the probe reads the book under the path as typed and the held value under the path standardized, as the app keys them, so `said: yes` also shows the app standardized the held value's key.
- **The relaunch.** It opened the book jawd kept, as the first run left it, and said nothing: the alert had said. Switching to L1, which an empty book doesn't hold, shows the book was jawd's. L1, split, was kept by jawd two seconds later: the script saw jawd keep it, two panes, and stopped jawd half a second after, by when jawd's answer had reached the app, as `layouts jawd` on the `wait:3` line shows. The next split, and the list's first row dropped into its pane, went to jawd two seconds after the drop, and that keep went unanswered, as `layouts jawd, unanswered at v<v2>` shows; the app quit holding what was on screen at that keep's version, saying jawd didn't answer. jawd was killed while still stopped, so it never read that keep, and started again on the same folder: it keeps L1 as it kept it, two panes, at the version the script saw before it stopped jawd, older than the held book's three.
- **The next launch.** It found jawd's book older than the held one, so it moved the held book to jawd and opened it, L1 with its three panes, and let it go once jawd had kept it, at a version past the one it was held at. The chat a snapshot opens before its keys, the list's first row, was already in L1's third pane, where the relaunch dropped it, so opening it only focused that pane, and the run changed nothing else: jawd's newer version and its three panes can only have come from the move, and the book jawd keeps is the one held, every layout and chat as it was.

A FAIL line, and exit 1, say a claim wasn't so. The check leaves a loaded build Mac some room, and a FAIL that load explains shows it: jawd is stopped half a second after the script sees it keep L1's edit, about six and a half seconds before the app asks it to keep the next change, so `the next change's keep waited for an answer jawd never gave` failing with the `wait:4` line ending `layouts jawd` means the probe took longer than that; the script looks for that edit for about fifteen seconds, so `jawd kept L1's edit, two panes, before it was stopped` failing with the `wait:3` line ending `, unanswered at v…` means jawd took longer than that to keep it, and `and the app had its answer` failing alone means jawd's answer hadn't reached the app in the half second; the relaunch takes about 23 of the snapshot's 30 seconds, past which its `{"panes":…}` line is `{"timed_out_s":30.0}`; and a snapshot waits five seconds at most after the chat list's first rows for the layouts to open (`settled`), its first line ending `layouts asking` when they hadn't. For any of those, run the check again. Any other FAIL is the app's: stop and say so in the report, with the lines. If the first run's line ends `layouts preferences`, or the probe says `jawd keeps v=0 for jaw-mac, and no layout`, the jawd copied is from before P10: stop and ask the controller. Look at the pictures with the Read tool: `book-1.png` shows one pane with its chat's name in its header, and `book-3.png` L1's three. If the build Mac has no `python3`, the script says so; stop and say so in the report rather than running anything here. The script removes its folder and its preferences, and stops its jawd demo and any run of the app it started, on its way out. `ssh … 'sh -s'` gives it no terminal on the build Mac, so a dropped session sends it no HUP: it finds out at its next write back, which raises PIPE in the shell, trapped as HUP, INT and TERM are, or fails the command that wrote, which `set -e` ends the script on. Either way `cleanup` runs, writing nothing, so a dropped session leaves nothing running or kept, a run of the app the script was waiting for first ending on its own.

- [ ] **Step 6: Commit**

```bash
git add mac/JawApp/Sources/Bench.swift mac/JawApp/Sources/Launch.swift mac/JawApp/Sources/AppModel.swift
git commit -F- <<'EOF'
A snapshot's panes lines end with where the layouts are kept, whether a keep waits for jawd's answer and the version of one on its way, an alert the app showed comes before them, wait:<seconds> is a step, and --layouts-in reads and holds the layouts in preferences of the check's own, only with --snapshot and for names starting dev.badcoffee.jaw.check.; on jawd demo, a book of twelve an earlier Jaw kept moved its first nine whose names fit with an alert naming the three left, the preferences left as seeded, the relaunch opened what jawd kept and an edit was seen kept by jawd before it was stopped, which is step 1.1's relaunch on a made-up account, a change jawd never answered, stopped and killed before it read it, was held at quitting at that keep's version under the folder's standardized path, and the next launch, on a jawd started again and changing nothing else, moved it to jawd as it was held
EOF
```

---

### Task 6: The step's second half in the docs

**Files:**
- Modify: `mac/README.md`, `docs/plans/2026-10-27-jaw-mac-parity-plan.md`, `docs/plans/2026-10-26-jaw-mac-grid-layouts-design.md`

**Interfaces:**
- Consumes: everything above.
- Produces: the README, the parity plan and the grid layouts design saying how the layouts are kept, jawd the one store, that P10 is on main, and that step 1.1 is done but for the owner's trial.

- [ ] **Step 1: The README**

In `mac/README.md`, make two changes.

In the paragraph on the app's options, make four. `--snapshot`'s words, `` which draws the window once the chat list has stopped changing and a chat is open (or the sheet over it) ``, become `` which draws the window once the chat list has stopped changing, the layouts are open and a chat is open (or the sheet over it), and won't run on jaw's own folder or one with an engine's session in it, since it keeps the layouts it makes in jawd ``. After `` `build-mac snapshot` runs it on a demo folder of its own each time, removed after. `` add:

```markdown
With `--snapshot`, `--layouts-in <name>`, for a name starting `dev.badcoffee.jaw.check.`, reads and holds the layouts in the preferences of that name, as the layout book's live check does with a book it seeds there; a snapshot touches no other preferences.
```

Add `` `wait:<seconds>`, which waits that long before it prints, `` to the `--keys` steps, after `` `terminal-panes` (Open the Terminal's Panes), ``. And after `greyed ones in brackets.` add:

```markdown
Each panes line ends with where the layouts are kept: `layouts jawd`; `layouts trying`, until the first keep says whether jawd keeps books; `layouts asking`, until jawd gives its book; `layouts preferences`, with a jawd that keeps none; or `layouts nowhere`, when jawd keeps a newer Jaw's or the layouts can't be kept anywhere; with `, unanswered` while a keep waits for jawd's answer, and ` at v<version>`, the version it was asked to keep at, while that keep is on its way. An alert the app showed since the line before comes before the panes, as `alert "…"`, and as the snapshot quits it prints a `quit:` line saying what it held on this Mac, if anything.
```

At the end of the "Panes and layouts" bullet, after Part A's paragraph, add:

```markdown
jawd keeps the book, the one store, as it keeps the terminal's panes (the owner, 29 October): `LayoutBook` as P10's format 2 (`forJawd`), asked for at each hello in the format it was kept in and kept by `LayoutKeeper` with `layout.set` once a change has stayed for two seconds and at once on quitting, the newest book jawd tells of taken, an answer or the `layout` push. The first time jawd keeps nothing for the app, the preferences' book (`layouts:` and the jaw folder) moves into jawd, its first nine named layouts whose names fit jawd's 64 bytes, and isn't written again; an alert names the rest once, which stay there for an earlier Jaw, and should jawd keep none after the move, as with a new or restored jaw.db, the next launch brings them back and says so. What's on screen at quitting that jawd didn't confirm keeping within a second, or a change made while jawd is away, is held in one value (`layouts.held:`, in format 1, `LayoutPreferences`) and moved into jawd at the next launch. With a jawd that keeps no books, the preferences keep the layouts as before P10. A book jawd keeps that this Jaw can't read is left as it is, and that launch keeps nothing; a value in the preferences this Jaw can't read is never written over. A slot jawd empties, its chat gone or shown by a slot before it, waits for a chat, since jawd doesn't say which of the two it was. A snapshot keeps its layouts in jawd, so it runs only on a folder of its own, and reads and holds nothing in the preferences but those `--layouts-in` names; `--bench` keeps nothing.
```

- [ ] **Step 2: The parity plan**

In `docs/plans/2026-10-27-jaw-mac-parity-plan.md`, step 1.1, replace the bullet that begins `- **With P10** (below):` with:

```markdown
- **With P10, built** ([its plan](2026-10-28-jaw-mac-layout-book-kept-by-jawd-plan.md)): jawd keeps the Mac's whole book of layouts, the one store, as it keeps the terminal's tree (Owner, 29 Oct): `layout.set` in P10's format 2 once a change has stayed for two seconds and at once on quitting, `layout.get` at each hello in the format it was kept in, and the `layout` push from another of the app's connections, the newest by version kept whichever comes first. The preferences' book is read once into jawd, the first time jawd keeps nothing for the app, and isn't written again: of a book from before, the first nine named layouts whose names fit jawd's 64 bytes move, ten layouts with the unsaved panes, the ones ⌃⌘0 to ⌃⌘9 reach (Owner, 28 Oct), and an alert names the rest once, which stay in the preferences for an earlier Jaw; should jawd keep none after the move, the next launch brings them back from the preferences and says so (Owner, 29 Oct). A change is kept nowhere for its first two seconds, as in the terminal; what jawd didn't confirm keeping when the app quit, or a change made while jawd is away, is held in one small value and moved into jawd at the next launch. A jawd that keeps no books, as one from before P10 doesn't, leaves the layouts in the preferences as before. Built against P10's protocol text and fixtures, on main since 817bc02, and `FakeJawd` answering as P10's jawd and as one from before it.
```

and the bullet that begins `- **Done when**` with:

```markdown
- **Done when** a relaunch opens the layouts the app was left with, kept by jawd, and the terminal's panes open in the Mac. Both halves are built, `make check-parity` lists nothing for 1.1, and on `jawd demo` a book from before moved into jawd from preferences of the check's own, the relaunch opened what jawd kept, and a quit jawd never answered was held and moved in at the next launch (the layout book plan's Task 5); what's left is the owner's trial on his account, once his jawd has P10.
```

Then the section `## P10, a layout book`, which still reads as a proposal, says it landed: its paragraph becomes:

```markdown
jawd's part of 1.1, proposed here for jawd's builders and on main since 817bc02, its audit approved (`docs/audits/2026-10-28-layout-book.md`). `layout.set` and `layout.get` take `format` 2: a book of named layouts, each slots on a grid's lines (its columns and rows as fractions, each slot its lines and its chat), its edited shape beside its saved one, which is focused, and the unnamed layout, the Mac's `LayoutBook` key for key, as `protocol/README.md`'s "Layouts" has it. Format 1 is still taken and read, each format read as the other, and a round trip between them comes back close rather than exact. jawd follows merged and aliased JIDs in every slot of every layout, as it does in a tree's panes, and the limits are sized for a book: 12 slots a shape, the ten layouts ⌃⌘0 to ⌃⌘9 reach, names of 1 to 64 bytes, and 64 KiB, its bytes measured against a bound `status` reports. It changed `crates/jawd/src/layout.rs` and the store's `interface_state`, following "Data integrity" in jaw's design.
```

- [ ] **Step 3: The grid layouts design**

In `docs/plans/2026-10-26-jaw-mac-grid-layouts-design.md`, make six changes.

In "Layouts", the "Switching" bullet's last sentence, `A chat that's gone (left, deleted, or not found by jawd) leaves its slot empty, saying so in the slot.`, becomes:

```markdown
A chat that's gone (left, deleted, or not found by jawd) leaves its slot empty, saying so in the slot; once jawd has emptied the slot itself, though, at launch or when it keeps the layouts, the slot waits for a chat like any empty slot, and the status line says how many panes jawd emptied, since jawd doesn't say whether each one's chat went or a slot before it shows that chat (pending the owner's answer to question 2 of [the layout book plan](2026-10-28-jaw-mac-layout-book-kept-by-jawd-plan.md)).
```

and the "Bounded" bullet becomes:

```markdown
- **Bounded:** at most 12 slots in a layout, and only the layout on screen holds windows, so it's at most 12 of jawd's 32 windows a connection, as now. There are ten layouts at most, the unsaved panes and nine named, the ones ⌃⌘0 to ⌃⌘9 reach (Owner, 28 Oct), as jawd keeps them (P10), each name 64 bytes at most, all of them together a few KB.
```

In "Storage", the paragraph that begins `Layouts are kept in the app's preferences until jawd keeps an interface's state (P1)`, the JSON under it, and the paragraph after it become:

```markdown
jawd keeps the layouts (P10), the one store, as it keeps the terminal's panes (Owner, 29 Oct): `LayoutBook`'s own JSON, key for key, as format 2 of `layout.set`, with `format` where the app's version is, as `protocol/README.md`'s "Layouts" draws it. Earlier Jaws kept them in the app's preferences as version 1 of the same JSON, under `layouts:` and the jaw folder's path. The first time jawd keeps nothing for the app, that book, or the panes mac-m1 kept (`grid:` and `panes:`) as the unnamed layout, moves into jawd and isn't written again: of a book with more than nine named layouts, or a name longer than 64 bytes, the first nine whose names fit move, and an alert names the rest once, which stay there for an earlier Jaw to open. Should jawd keep none after the move, as with a new jaw.db or one restored from before it, the next launch moves that book in again and says the layouts came back (Owner, 29 Oct). A change goes to jawd once it has stayed for two seconds, so a crash within them loses it, as in the terminal, and the app keeps what's on screen at once when it quits; what jawd didn't confirm keeping within a second then, or a change made while jawd is away, is held on this Mac in one value (`layouts.held:` and the folder's standardized path, in format 1, with whether the alert has said) and moved into jawd at the next launch. With a jawd that keeps no books, the preferences keep the layouts as before. This is the app's own state, not jaw's data, so the Mac's half doesn't touch the data layer. P10 is the data-layer change, audited on jawd's side.
```

In "When things go wrong", the bullet `- **A chat that's gone:** its slot stays, empty, saying the chat isn't there any more, and the layout isn't marked edited.` becomes:

```markdown
- **A chat that's gone:** its slot stays, empty, saying the chat isn't there any more, and the layout isn't marked edited. Once jawd has emptied the slot itself, at launch or when it keeps the layouts, the slot waits for a chat instead, and the status line says how many panes jawd emptied (pending the owner's answer to question 2 of [the layout book plan](2026-10-28-jaw-mac-layout-book-kept-by-jawd-plan.md)).
```

and the bullet that begins `- **A layout that doesn't decode**` becomes:

```markdown
- **Layouts that don't decode** (a newer Jaw's, or a damaged preference): they're said and left as they are, so a newer app can read them. A book jawd keeps that this Jaw can't read is said in an alert, the panes open empty, and that launch keeps nothing, so nothing is written over it; so is a book in the preferences this Jaw can't read when jawd keeps no books, since the preferences then can't keep the layouts either. The value this Mac holds of them has a format, 1, so one a newer Jaw writes in another reads as one that doesn't decode.
```

In "Build order", item 6 becomes:

```markdown
6. **With P10:** layouts kept by jawd, and opening the terminal's layout ([the parity plan](2026-10-27-jaw-mac-parity-plan.md)'s step 1.1).
```

- [ ] **Step 4: Run the check**

Run: `make check-parity`, then `python3 scripts/check-parity.py --table | grep -E '^r layout|^p layout '`.
Expected: the first exits 0, its list holding nothing for step 1.1, as since Task 1. The table has three lines: `layout.get` and `layout.set` used by the terminal and the Mac and served by jawd, and the `layout` push used by both, none with a step.

- [ ] **Step 5: Commit**

```bash
git add mac/README.md docs/plans/2026-10-27-jaw-mac-parity-plan.md docs/plans/2026-10-26-jaw-mac-grid-layouts-design.md
git commit -F- <<'EOF'
Parity step 1.1 is done but for the owner's trial: the parity plan says how jawd keeps the Mac's book of layouts as P10's format 2, the one store as the owner chose on 29 October, the preferences' book moved in once and never written again, moved in again only should jawd keep none after the move, a change kept once it settles and what jawd didn't confirm at quitting, or a change made while jawd is away, held in one value and moved in at the next launch, ten layouts and an alert naming those of a book from before that didn't move, a jawd that keeps none leaving them in the preferences, that the move, the relaunch and a held quit were tried on jawd demo, and in its P10 section that P10 landed; the grid layouts design keeps ten layouts, the unsaved panes and nine named, as the owner chose on 28 October, in place of 30, says how jawd keeps them and what a book that doesn't decode does, and that a slot jawd emptied waits rather than saying its chat went, since jawd doesn't say which it was, pending the owner's answer; the Mac README says how the layouts are kept, that a snapshot waits for them and runs only on a folder of its own, --layouts-in, the snapshot's wait step, its panes lines' alert and layouts fields and its quit line; make check-parity lists nothing for 1.1
EOF
```

- [ ] **Step 6: Clean up and report**

Read `git log --oneline 817bc02..origin/main -- protocol/README.md protocol/fixtures/` again (see the Global Constraints). Run `rm -rf mac/build/book`, then `make prune-builds`. Report each task's runs, the pictures looked at (`app.png`, `before.png`, `book-1.png` and `book-3.png`), the live check's lines, and what stays: the branch's copy on the build Mac (`~/src/build-mac/jaw/jaw-panes`, removed once the branch has landed) and `target/debug/jawd` in the worktree, which goes with it.

---

## After this plan

P10 is on main, so this plan tries the move of a book from before, the relaunch and a quit jawd never answered end to end on `jawd demo` itself (Task 5's live check). What's left is the owner's trial on his account, once his jawd is from main with P10 (817bc02 or later): his first launch of this Jaw moves his preferences' layouts into jawd, with the alert if some stay behind; a relaunch opens them; and the terminal's panes open in the Mac. Then the parity plan's step 1.1 says it was tried, and the board shows it done.
