# Audit: the layout book (P10)

# Round one: `a03782d`

Range: `fa9265c..a03782d` in `layout-book-2` (worktree `~/src/jaw-layout-book-2`), 28 October 2026. Auditor: database-expert. `fa9265c` is `origin/main`. Line numbers are at `a03782d`, and paths are under `crates/` unless they say otherwise.

The range is six commits, and nothing is uncommitted but this note:
- `ae01146`, the first builder's work in progress, carried over from `layout-book`;
- `05db671`, the first whole cut: format 2 kept beside format 1, each read as the other, and migration 17;
- `1d1812b`, the books' fixtures, `float_roundtrip`, and tests tying the fixtures to jawd's answers;
- `8dddcd2`, the protocol's Layouts in both formats, and P10 marked built;
- `25dd178`, a book jaw.db holds that jawd wouldn't keep reads as no tree;
- `a03782d`, the builder's mutants' findings: a pure overlap, the rebuild's test finding its migration by its text, and read-time bounds.

I read the range whole against `fa9265c`, with the code around it. `jawd/src/sock.rs` is untouched, as the builder says, and the writer's half (`jawd/src/store/write/layout.rs`: the version rule, the room for 16 interfaces, the quarantine) is P1's, unchanged but for its comments and tests.

What I did, in one clone at `a03782d` with one build folder, built with `CARGO_INCREMENTAL=0` and switched between revisions with `git checkout`:
- ran the branch's layout, schema, plan, upgrade and fixture tests, then the workspace's tests;
- had main's own jawd (`fa9265c`) keep 16 interfaces' layouts in a version-16 jaw.db, and upgraded that file with the branch's jawd;
- killed the branch's jawd at each statement of migration 17 and just before its commit, and 40 times at random during its start;
- killed a real jawd of the branch 300 times around a `layout.set` of a random book, over its socket;
- built 54 switches, each turning off one rule the range adds, and ran each against the tests around it;
- wrote probes: 20,000 random trees and 20,000 random grids through both conversions, 300 trees kept through the API and read across interfaces, lines at the edges of what a double writes, two slots that come to one chat read in each format, slots through a chain of merges here and on `merge-chains`, and what a book at every limit costs;
- moved migration 17 onto `chat-actions`, where it becomes 18;
- measured `float_roundtrip` with and without it.

Everything ran with `--engine demo` or in the tests' own folders, under `/tmp/jaw-audits/layout-book`, deleted once this note was written. No Go engine was built, so no test could start one. I stopped only the jawds I'd started, by their PIDs. My switches, probes, scripts and logs are kept in this auditor's store, `~/Library/Application Support/Cursor/AgentStores/cursor_agent_stores/1ed70e92-18c6-4a50-9de6-c3c498461792/files/layout-book-round1/`; `switches-and-probes.diff` applies to `a03782d`.

## In short

Approved. Nothing in this range loses data or leaves jaw.db inconsistent. One rule is tested along one axis only (T1): which of two slots keeps a chat they both come to, which decides what a reader is shown, not what jaw.db keeps. Its test is printed below, and it's cheap to add with the trailers. The rest are follow-ups.

- **Migration 17 is a correct schema step.** On a version-16 jaw.db that main's jawd filled with 16 interfaces' layouts, one of exactly 16,384 bytes, the branch's jawd kept every row byte for byte, and the rebuilt table takes 65,536 bytes and refuses 65,537. Killed at each of its four statements and just before its commit, the file was wholly at 16 with every row; started again, it went to 17 with every row. It costs a synced commit, 16 to 54 ms on this Mac, whatever the account's size.
- **Its number holds.** Main has 16 migrations, so it's 17 today. Moved onto `chat-actions`, whose own 17 is `app_state_awaiting`, it becomes 18 by its lock line and its comment, with the same text and hash (`30b59add…`), and the schema, layout, plan, upgrade and fixture tests pass. `folders` (a data step), `m2-deleted` and `m3-media-in` also add a 17th; none of them touches `interface_state`.
- **Books are kept durably and idempotently.** `layout.set` answers after a synced commit, and only a newer version replaces what's kept, whatever either's format. In 300 kills around a set, the book kept after each restart was never older than the newest answered as kept, never newer than the newest asked for, and always the very book asked for at its version, every line bit for bit. The integrity check was ok and nothing was quarantined.
- **Each format reads as the other, and loses nothing.** Of 20,000 random trees, every pane became a slot or was named in `dropped`, each slot's id was its pane's place, and the focus stayed on its chat; read back, each was a tree of the same chats with the focus on the same one. Of 20,000 random grids that are trees, every slot came back with its chat and the focus. Read back and forth, panes move by a few thousandths of the window (O2), and a tree of 13 to 16 panes reads as a grid jawd won't keep as a book (O3).
- **Merged and aliased JIDs** are followed in every slot through `chat_named`, as panes are: with that switched off, three of the builder's tests fail. Through a chain of merges, which main can still make (P1's X1, outside this range), a slot lands on a chat being merged away, as a pane does. On `merge-chains` no chain forms, and every JID of the person reads as the chat that stays.
- **The limits** hold at their exact edges: ten layouts, the unnamed one and nine named, 12 slots in each shape, edited ones too, and 64 KiB. One past each is error 5, refused before the writer, with nothing quarantined.
- **No new scan or temporary b-tree.** The one changed statement is `status`'s, which scans `interface_state`'s 16 rows at most, as before. With 16 books of 64 KiB it takes 0.26 ms at p50 and 0.68 ms at p99, more than the builder's 0.22 ms "worst" (O4), on a connection of its own.
- **`float_roundtrip` is right, and right workspace-wide.** It costs about 10 ns for each 16- or 17-digit double parsed, and nothing for JSON without one. On jawd's hot paths only a location's two coordinates and a book's lines are such numbers, and no value jaw.db keeps or compares changes except the books it's there for. The judgement is below.
- **Switches:** 53 of my 54 fail a builder's test; the survivor is T1's.
- **The gate's tests** pass at the head but for ten timing tests in the full run, at load 55 to 72 beside other agents' builds, each of which passed alone 3 of 3.

## What was asked, checked

### The migration

- **Its shape** (`store/schema.rs:938`): a new `STRICT`, `WITHOUT ROWID` table with migration 12's columns and `CHECK`s but the JSON's raised to 65,536 bytes, every row copied with every column, the old table dropped and the new one renamed.
  - Nothing refers to `interface_state`, and it has no foreign key, no index beside its primary key and no trigger. Main's version-16 file has only the table in `sqlite_schema` for it. So there's nothing to recreate, and the foreign-key check a data step runs would find nothing.
  - The lock makes a rebuild a data step only for a table others refer to, and migration 7 rebuilt the outbox as a schema step the same way. The table holds 16 rows at most, so the step costs the same however big jaw.db is.
  - Its class is declared in its text: jaw's own, as migration 12 made it, never dropped, copied as it is by a rebuild.
- **One transaction.** It runs with every pending schema step in one `IMMEDIATE` transaction with `user_version` and the fingerprint's check (`schema.rs:1140`). The builder's fault at every statement of every migration passes over it, so a failure anywhere leaves the file as it was.
- **On main's own file.** Main's jawd kept 16 interfaces' layouts: names from 3 to 64 bytes, with accents and an emoji, versions from 1 to `i64::MAX`, a tree of 16 panes and one of exactly 16,384 bytes. It refused a 17th interface with error 5. The branch's jawd then started on that file:
  - `user_version` 17; the 16 rows identical, `interface`, `key`, `value`, `v` and `updated_at`; no `interface_state_rebuilt` left; the table's text as migration 17 writes it.
  - Each layout read the same as a tree as before, and as a book too, each with its `dropped`.
  - Over the socket, a book of exactly 65,536 bytes was kept, and one of 65,537 was error 5.
  - Outside jawd, on a copy: 65,536 bytes taken; 65,537 bytes, 65,538 in two-byte letters, text that isn't JSON, a version of 0, a version that's text, and names of 0 and 65 bytes refused; a name of 64 bytes taken.
- **Kills.** A switched build stops for a minute at a statement named in its environment, so a SIGKILL lands there. At each of the four statements and just before the commit, the killed file was at 16, every row as it was, the old 16,384-byte limit, `integrity_check` ok. The branch's jawd then opened it at 17 with every row and stopped cleanly. 40 kills at random in the first 0.4 s of a start left 6 files at 16 and 34 at 17, all whole, and each then opened at 17.
- **Its number.** The text names no number but migration 12's, so its hash doesn't change when it moves. On `chat-actions` (`73cdaf3`) the range applied with three conflicts: `migrations.lock` and `schema.rs`, resolved by putting this migration after `chat-actions`' as 18 (its lock line and its `// 18,` comment), and two rows of `protocol/README.md`, which the tests don't read. There, 61 tests of `layout::`, `store::write::layout`, `store::schema` (with the fault test) and `store::plans` passed, and so did the upgrade test and jaw-proto's fixtures.
- **Its tests.** The builder's `rebuilt_to_take_a_book_the_table_keeps_every_row_it_had_as_it_was` (`jawd/src/store/write/layout.rs:202`) finds the migration by its text, so it survives renumbering. It fails with the copy removed, or `v` or `updated_at` not copied (my switches). The upgrade test from version 1 keeps its layouts after the upgrade, so its `interface_state` is empty when 17 runs: the rebuild test and my upgrade of main's file are what exercise the copy.

### Books

- **Durable before answered.** `keep` writes through `apply_durable` (`jawd/src/layout.rs:151`), as for trees. The book's own test checks that `synced_commits` moves by one, and fails, with P1's, when the commit isn't synced.
- **Idempotent, the newer version winning whatever the formats.** The builder's `only_a_newer_layout_replaces_a_book_whatever_its_format_so_asking_again_or_late_changes_nothing` covers asking again, a late set, another of the same version, a tree of the same version, a newer tree replacing the book and pushed as a tree, and a late book answered with that tree read as a book.
- **A crash between the commit and the answer.** The builder's two scenes for books, killed once the answer is made and while it waits for the lock, pass. In my 300 kills, 223 sets were unanswered when killed, and 172 of those were there after the restart; neither kind was acknowledged, so both are allowed.
- **Restored after a restart.** Every restart in the kills read the book back as a book, against what was asked at its version, and as a tree.
- **Unknown keys.** `Book`, `Named`, `Grid` and `Slot` deny unknown fields, so a book with a key jawd doesn't know is refused with -32602, not kept without it; the builder tests `colour` and a `v` inside the book.
- **The lines.** A book's lines are kept and answered as the numbers they were given (see `float_roundtrip`). `-0.0`, `5e-324` and `1e-7` are taken and come back as they were.

### Each format read as the other

- **A tree as a grid** (`layout/book.rs:183`). In 20,000 random trees of 1 to 16 panes, with shares often meeting in one line: every grid holds, every pane is a slot or named in `dropped`, once and in the tree's order, the slot ids are the panes' places from 1, and the focus is on the focused pane's slot, or the slot before it, or the first. No pane rounded to nothing in these; the builder's test makes one that does.
- **A grid as a tree** (`layout/book.rs:254`). Each of those grids read back as a tree of the same chats, the keys on the same chat. In 20,000 random grids that are trees, of 1 to 12 slots on lines of 17 digits, each read as a tree of every slot's chat with the keys on the focused slot's, and back as a grid of the same slots.
- **Through the API, across interfaces.** 300 random trees kept as `jaw`, each read by `jaw-mac` as format 2 and kept as its own book, then read back as format 2 over JSON and as format 1 by `jaw`. Every book kept came back key for key with its lines bit for bit, and every tree came back with the same chats and the keys on the same chat. 84 couldn't be kept, having more than 12 panes (O3).
- **What's lost in an answer, by design:** a book read as a tree is its layout on screen alone, and a pinwheel is no tree. Each interface keeps its own row, and a read writes nothing, so nothing kept depends on a conversion.

### Merged and aliased JIDs in slots

- `book::resolved` (`layout/book.rs:146`) reads every slot's JID through `chat_named`, the same lookup as panes, with each JID looked for once an answer. With the lookup switched off, or a gone chat's JID left in its slot, three of the builder's tests fail; with no dedupe, two do.
- The builder's merge test runs a merge to its end and checks every slot names a chat row that exists, and that a slot naming Rui by a device JID, while his older LID chat is being merged away, shows his number's chat.
- **A chain of merges, here.** P1's X1 scene: the new number's chat the oldest, then the old number's, then the LID's; the LID joined to the old number, then found to be the new number's too. Main leaves a chain: the LID's chat merging into the old number's, and that one into the new number's. A book's slots on the LID then read as the old number's chat, which is being merged away; a slot on the old number's device JID reads as the new number's, through the person's other JIDs, which leave out a chat being merged away. Once the merges have run, the person is left with two chats, X1's bug, and every slot names one that exists. That's main's merge code, and panes do the same.
- **On `merge-chains`** (`c7ca163`, read only from `~/src/jaw-merge`'s history, built in my clone). The same scene makes no chain: both merges go into the new number's chat, and the LID, the old number, its device JID and the new number all read as the new number's chat, during the merges and after. A trial merge of the two branches stops in `store/write.rs`, where `merge-chains` and main have each changed the merge since `merge-chains`' base, so I checked `chat_named` there on its own: slots use nothing else.

### The limits

- The builder's `exactly_nine_named_layouts_12_slots_and_64_kib_are_kept_and_one_past_each_is_error_5_with_nothing_quarantined` keeps a book at every limit, and refuses ten named layouts, 13 slots and 65,537 bytes with error 5, counted in `refused`, with nothing quarantined. My switches at each edge, and with the edited shapes left out of the slot limit, each fail a builder's test.
- A book at every limit, its names 64 bytes and its JIDs 64 bytes, takes 30,572 bytes, as the builder says.
- `status` counts books and the biggest book apart from trees by `format`, and a value of no format beside the trees.

### Plans and costs

- The one changed statement is `status`'s (`store/mod.rs:742`), pinned in `store/plans.txt` and allowed its scan of a table of 16 rows at most (`store/plans.rs:71`). Its plan is `SCAN interface_state`, no temporary b-tree. The plan test passes.
- The upsert, the read by primary key and the count for room are P1's statements, unchanged.

### `float_roundtrip`

- **What it changes.** serde_json reads a number with a fraction or an exponent as the nearest double, where without it a fast reading is off by one unit in the last place for about one double in ten: 10,641 of 100,000 random doubles, written and read back, came back as another number without it, and none with it. Integers, strings, and the `RawValue`s jawd reads its stored spans, reply and detail into are read as before.
- **What it costs** (release build, five alternating runs a side; Measurements): about 10 ns more for each 16- or 17-digit double, about 3 ns, within the noise, for a decimal of 9 or 10 digits, and nothing for JSON without a fraction. A location's detail, as the window parses it, takes 288 ns rather than 253; a messages push of 100 text rows is unchanged; a `layout.set` of a book at every limit, with 266 lines, takes 119 µs rather than 106.
- **On jawd's hot paths.** Message rows carry such numbers only for locations, two each (`convert/kinds.rs:291`), which the window parses for every row it sends (`feed/window.rs:190`). The protocol's requests carry none but a book's lines. So a live message costs tens of nanoseconds more at most, against its 5 ms.
- **What's kept or compared.** jaw.db keeps a float read from JSON only in books, which are new. Coordinates are written from the protobuf's doubles by serde_json's writer, which the feature doesn't touch, and read back only to be shown (`feed/window.rs:190`, `store/read.rs:482` and `:496`), where they now go out as jaw.db has them, rather than one in ten off by a unit. Amendments, settings, `sync_state` and the outbox's JSON hold no such numbers. `answer`'s comparison is of two strings jawd wrote. So no value an earlier jawd kept reads or compares differently.
- **Limited to layouts?** It can't be in Cargo: a dependency's features are one set for the whole build. In code it would mean reading the lines from raw JSON and parsing them with Rust's own, exact, `str::parse::<f64>`, but `layout.set` reads its params as a `Value` first (`layout.rs:126`), so the lines are rounded before the book sees them. That's a restructuring for no measurable gain. I'd keep it workspace-wide, as it is, with the comment in `Cargo.toml` saying why.
- **Its tests.** Without the feature, the builder's `a_books_lines_come_back_as_the_numbers_they_were_written_as` fails, and so does my probe's bit-for-bit check through the API.

### The gate

- `cargo test --workspace --no-fail-fast` at `a03782d`, at load 55 to 72 beside other agents' builds:
  - passed: jaw-client's 19 and its 3 against a real jawd, jaw-media's 22 and 17 (1 ignored), jaw-proto's 12 and its fixtures, wa_proto's 3, jaw's settings tests, and jawd's starting, stopping and upgrading tests;
  - jawd's: 534 passed, 10 ignored, and 5 timing tests failed: `send::tests::reading_moves_the_read_position_and_marks_read_once_a_spell_however_fast_it_is_read` and four of the store's WAL and checkpointer tests;
  - jaw's: 316 passed and 4 bench tests failed, each a 16 ms keypress or 60-a-second bound;
  - jaw-voice's: 35 passed and `a_recordings_end_is_held_for_a_caller_that_falls_behind_and_never_dropped` failed.
- Each of the ten passed alone 3 of 3 at load 50 to 67. The range touches none of their code; the terminal's bench parses text messages, which have no fraction for `float_roundtrip` to change.
- `scripts/check-data-audit.py fa9265c a03782d` fails only for want of this note's trailer, on all six commits.
- Not run: `go vet`, `gofmt` and the engine's tests (the range touches neither `engine/` nor `proto/`), and `clippy`.

## Findings

### Data loss

None.

### Inconsistency

None in this range. Through a chain of merges a slot can land on a chat being merged away, as a pane can; that's P1's X1, in main's merge code, which `merge-chains` closes (Outside this range).

### Tests (missing)

- **T1, test gap. Which of two slots keeps a chat they both come to is tested along one axis only.**
  - **Where:** `reading` (`layout/book.rs:177`), which orders a shape's slots by `(r0, c0)`, top to bottom and then left to right, as the README's Layouts says.
  - **What happens:** with the order taken by `(c0, r0)` instead, every test of `layout::`, `store::write::layout` and `store::plans` passes. The builder's test has its two slots in one column, where both orders agree. In my probe, Ana by her number below the group on the left and by her LID down the whole right: reading order keeps the right-hand slot, which starts higher, and empties the lower left; column order does the opposite. A reader would be shown the chat in another slot, and told another JID in `dropped`. Nothing kept changes.
  - **Why it doesn't block:** the rule decides what an interface is shown after a merge or an alias, not what jaw.db keeps. The rule beside it, a chat in one slot of a shape at most, emptied in its place, fails two tests when switched off.
  - **Fix:** adopt the test printed below. It passes at `a03782d`, and fails with the order by columns, or in the slots' stored order.

### Unpredictable performance

None breaks a budget (Measurements):
- A book at every limit, 30,572 bytes, 228 slots on 120 chats that exist: `layout.set` 10.2 ms at p50, most of it the synced commit; `layout.get` 1.9 ms at p50 and 2.0 to 2.6 ms at p99, on an interactive reader; as a tree, 1.2 ms; with every chat gone, so each JID is looked for twice, 2.4 ms. A debug build, so these are high.
- The upsert of a 64 KiB book in the writer's transaction: 30 µs at p50 and 164 µs at p99, its `json_valid` included, beside 7 µs for a tree of 16 KiB.
- `status`'s statement with 16 books of 64 KiB: 0.26 ms at p50, 0.68 ms at p99 (O4).
- Migration 17 on main's file: 16 to 54 ms, median 21.5, its synced commit included; P1 measured a commit setting only `user_version` at 16 to 41 ms.

### Other (follow-ups; none blocks)

- **O1, other. A book read as a tree and read as a book can show a chat two slots come to in different places.**
  - **Where:** `answered`'s arm for a book read as format 1 (`jawd/src/layout.rs:361`) cuts the book's raw shape into a tree and leaves out duplicates in the tree's order; a book read as format 2 empties them in reading order.
  - **What happens:** in my probe, a@x top left, b@x below it and c@x down the right, b@x and c@x coming to one chat. As a book, the right-hand slot keeps it, the lower left is emptied, and `dropped` is `["b@x"]`. As a tree, the lower left keeps it, the right-hand pane is left out, and `dropped` is `["c@x"]`. The README gives each rule, but `answered`'s comment says a reader of either format leaves out what a reader of trees would, which is so for a tree kept, not a book.
  - **Fix:** in that arm, resolve the book with `book::resolved` first and cut its resolved shape, so both formats empty the same slot and name the same JID; or say in the README that a book read as a tree leaves out duplicates in the tree's order.
- **O2, other. The README's round trip is close, not exact.**
  - **Where:** `protocol/README.md:485`, "Read as the other format and back, a tree is itself, or the same panes in the same places when two of its splits meet in one line".
  - **What happens:** each share re-derived from a grid is rounded to thousandths of the part it splits, and the roundings add up through nesting. Of my 216 trees of up to 12 panes kept across interfaces, 81 came back as the very tree, and a pane's edge moved by up to 0.0030 of the window. Of the 20,000 random trees, up to 16 panes, edges moved by up to 0.0080, and by up to 0.0016 more on a second trip. Of the 20,000 random grids, 2,375 came back on the very lines, and an edge moved by up to 0.0038. Nothing kept changes.
  - **Fix:** "…the same panes in the same places, to within a few thousandths of the window".
- **O3, other. A tree of 13 to 16 panes reads as a grid jawd won't keep as a book.**
  - **What happens:** format 1 takes 16 panes and format 2 takes 12 slots, so the Mac app opening such a terminal layout can show it but not keep it as is: error 5. 84 of my 300 trees.
  - **Fix:** for the app's half of step 1.1, or the README: say so beside the limits.
- **O4, other. `status`'s statement costs more than the builder's "0.22 ms worst".**
  - **What happens:** with 16 books of 64 KiB, 0.26 ms at p50 and 0.68 ms at p99 with SQLite 3.54 at load 60; 0.28 ms and 0.37 ms in jawd's own debug build; `layouts_kept`, opening its connection, 1.15 ms and 1.5 ms. It reads on a connection of its own for `jawd status`, so nothing waits on it.
  - **Fix:** the number, in whatever cites it.
- **O5, other. P1's H11, H6 and H5 stand, and this range is H11's case.**
  - **H11:** migration 17 is the first since P1 to drop a table of jaw's own. It copies every row, and its own test fails without the copy, but there's still no test over `MIGRATIONS` that a later migration can't drop such a table without copying it, which the design's Migrations says there is ("A test checks no jaw-own table is ever dropped"). Fix as H11 said.
  - **H6:** a book jawd couldn't write is quarantined whole, up to 64 KiB now, still without its version.
  - **H5:** a version at the top of the range still wedges an interface's layout, a book's as a tree's.
- **O6, other. The workspace's `Cargo.toml` isn't on the data layer's list.**
  - **Where:** `scripts/check-data-audit.py:15`.
  - **What happens:** `1d1812b` turned `float_roundtrip` on, which changes how every crate reads a number with a fraction, and was flagged only because it also touched the book's tests. A change to `serde_json`'s or `rusqlite`'s features alone wouldn't be.
  - **Fix:** have the check flag a change to the `rusqlite` and `serde_json` lines of `Cargo.toml`, or say in the data-integrity rule that their features are the data layer's.

### Outside this range

- **X1 stands:** main can still make a chain of merges, and `chat_named` follows one merge, so a pane or a slot on the chain's first chat reads as the chat in the middle, which is being merged away, until the merges end (my probe above). `merge-chains` closes it for both. The README's "never a chat being merged away" holds once it lands.
- **A tree takes unknown keys and drops them:** `Layout` (format 1) doesn't deny unknown fields, as P1 left it, where a book refuses them.
- **The socket reads a line of any length** (`jawd/src/sock.rs:224`), so a book's 64 KiB is bounded only after it's parsed. It's the same user's own socket.
- **The ten timing tests** above, in the full run under load.

## Measurements

The machine is a Mac Studio (M1 Ultra, 20 cores, macOS 26.6.2), with rustc 1.99.1, shared with other builders and audits at load 32 to 72; the load is beside each number.
- **The clone:** `/tmp/jaw-audits/layout-book/jaw`, at `a03782d`, its one build folder built with `CARGO_INCREMENTAL=0`. Binaries copied out before any change: the head's jawd (`9d871fb2…`) and its tests, and main's jawd at `fa9265c`. Every jawd ran with `--engine demo`, `JAW_HOME` in my folder and `nice`.
- **What I touched:** nothing in `~/src/jaw`, the builder's worktree or `~/src/jaw-layout-book` but this note, and nothing under `~/Library/Application Support/jaw`.

**The branch's own tests, at the head** (load 63 to 70): `layout::`, `store::write::layout`, `store::schema` and `store::plans`, 60 passed and 2 ignored (the crash scenes, which the crash tests run), in 141 s; `upgrading` 1; jaw-proto's 12 and its fixtures 3.

**Main's file, upgraded** (`make_v16.py`, `upgrade_v16.py`, load about 60): as under The migration. Main's `status` before: 16 kept, the largest 16,384 bytes, 1 refused. The branch's after the book: 16 kept, the largest tree 16,384 bytes, 1 book of 65,536 bytes, 1 refused.

**Kills during the migration** (`kill_migration.py`, the switched jawd with `JAW_AUDIT_PAUSE_AT`, load about 60):

| Killed at | The file | Opened again by the head's jawd |
| --- | --- | --- |
| statement 0, the `CREATE` | 16, every row, limit 16,384, ok | 17, every row, limit 65,536, ok |
| statement 1, the copy | the same | the same |
| statement 2, the `DROP` | the same | the same |
| statement 3, the `RENAME` | the same | the same |
| before the commit | the same | the same |
| at random in 0 to 0.4 s, 40 times | 6 at 16 and 34 at 17, each whole | 17, every row |

**Migration 17's cost:** its SQL run with Python's SQLite 3.54 on seven copies of main's file, `BEGIN IMMEDIATE` to a `FULL`, `fullfsync` commit: 16.2 to 53.5 ms, median 21.5 (load 48 to 56).

**Kills around a set of a book** (`kill_books.py`): each run starts the head's jawd, reads the book kept and checks it, sends a `layout.set` of a new random book one version newer (up to nine named layouts, edited shapes, lines of 17 digits, slots on the demo's two chats, on chats it doesn't have, or empty), and SIGKILLs it at a random time after. A fresh folder each series.

| Kills | Seed | Killed after | Answered | Unanswered, there after the restart | Unanswered, not there | Last answered as kept, last asked, kept at the end | Load |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 150 | 7 | 0 to 25 ms | 67 | 71 | 11 | 150, 150, 150 | 60 to 68 |
| 150 | 8 | 0 to 10 ms | 8 | 101 | 40 | 122, 150, 149 | 55 to 71 |

Each series ended with a start that checked its last kill, then a SIGTERM: `integrity_check` ok, nothing quarantined, `user_version` 17.

**Switches.** All in one test binary, each on when `JAW_AUDIT_MUT` names it, run four at a time at load 60 to 68 against `layout::`, `store::write::layout` and `store::plans` (48 tests), and for migration 17's text against `store::schema` too, without its fault test (59). With none on, all passed.

| Area | Switches | Caught | Survived |
| --- | --- | --- | --- |
| The book's limits and names: ten layouts, 12 slots, edited shapes too, 64 KiB at and past it, bytes of a tree's limit, no byte check, names once, of 1 to 64 bytes, `current` named | 9 | 9 | |
| A shape: lines in order, from 0 to 1, no more than the slots' edges, ids, JIDs, a chat once, no holes, no overlaps, every line an edge | 9 | 9 | |
| Reading slots: no lookup, a gone chat kept, no dedupe, `dropped` with repeats, dedupe in stored order, dedupe by columns | 6 | 5 | by columns (T1) |
| Converting: a pane too small kept, ids from 0, the focus off by one, rows cut first, shares floored, no read-time bound, no read-time check | 7 | 7 | |
| Formats: a set answered in the kept format, a get's format ignored, an unknown format taken, a kept book unread, a tree laid out unresolved, a book cut raw, an empty pane in `dropped` | 7 | 7 | |
| The synced commit; `status`'s books and largest | 3 | 3 | |
| Migration 17's text: no copy, the limit at 65,537, 65,535 or 16,384, `updated_at` or `v` not copied, no `STRICT`, no `WITHOUT ROWID`, no `CHECK` on the key, the JSON, the version or the name, no swap | 13 | 13 | |

With any migration switch on, `a_jaw_db_from_before_the_application_id_is_given_it` and `a_data_step_waits_for_a_whole_synced_backup_at_the_old_version_and_a_database_up_to_date_needs_none` fail whatever the switch, since they build their databases from the text without it, so I didn't count them. Every migration switch fails at least one other test: the rebuild's, the table's `CHECK`s at their edges, the plan test (`WITHOUT ROWID`) or the book limits'.

**Probes** (in `switches-and-probes.diff`), at the head, load 32 to 68:

| Probe | Result |
| --- | --- |
| 20,000 random trees of 1 to 16 panes, to a grid and back (an unbounded `tree_of` past 12) | every pane a slot or dropped, once; ids and focus right; 5,071 past 12 slots; 14,197 back as another tree; furthest edge 0.0080, then 0.0016 on a second trip |
| 20,000 random grids that are trees, 1 to 12 slots, lines of 17 digits | every slot and the focus back; 2,375 on the very lines; furthest edge 0.0038 |
| 300 trees kept as `jaw`, read and kept as `jaw-mac`'s book, read back both ways | 84 refused past 12 slots; every book back key for key, bit for bit; 81 trees exact; furthest edge 0.0030 |
| Lines `-0.0`, `5e-324`, `1e-7`, `0.1 + 0.2`, `1 - ε/2` | each kept and read back exactly |
| Two slots that come to one chat, read in each format | as a book, the right keeps it (`dropped` b@x); as a tree, the lower left does (`dropped` c@x): O1 |
| A chain of merges, here | during: the LID's slots on the old number's chat, being merged away; after: every slot on a chat that exists, the person with two chats (X1) |
| The same on `merge-chains` | both merges into the new number's chat; every JID of the person reads as it, during and after |

**Costs** (the probe `audit_what_a_book_at_every_limit_and_sixteen_books_of_64_kib_cost`, a debug build, three runs, load 32 to 52; and Python's SQLite 3.54 on copies of the upgraded file, load 57 to 65):

| What | p50 | p99 |
| --- | --- | --- |
| `layout.set` of a book at every limit (30,572 bytes) | 10.1 to 10.4 ms | 15.1 to 17.9 ms |
| `layout.get` of it, 228 slots on 120 chats | 1.86 to 1.93 ms | 2.00 to 2.60 ms |
| the same, as a tree | 1.17 to 1.24 ms | 1.29 to 1.77 ms |
| the same book with every chat gone | 2.35 to 2.44 ms | 2.60 to 3.16 ms |
| `layouts_kept`, 16 books of 64 KiB, its connection included | 1.15 to 1.18 ms | 1.53 to 1.56 ms |
| `status`'s statement alone, the same, jawd's SQLite in a debug build | 280 to 282 µs | 336 to 371 µs |
| `status`'s statement alone, the same, SQLite 3.54 | 259 µs | 685 µs |
| the same, 16 small trees | 6.6 µs | 10.3 µs |
| the upsert, uncommitted, of 200 bytes, 16 KiB, 30 KiB, 64 KiB | 4.0, 7.4, 12.7, 30.5 µs | 73, 21, 25, 164 µs |

**`float_roundtrip`** (`crates/jaw-proto/examples/audit_float_bench.rs`, release builds with the feature and without, run alternately five times each at load 58 to 61; each number the median of the five medians of 31 samples of `serde_json::from_str::<Value>`):

| Payload | Bytes | Fractions | Without, ns | With, ns | With / without |
| --- | --- | --- | --- | --- | --- |
| a request (`messages.watch`) | 122 | 0 | 558 | 561 | 1.01 |
| `status`'s answer | 641 | 0 | 2,868 | 2,882 | 1.00 |
| a location's detail, as the window parses it | 64 | 2 | 253 | 288 | 1.14 |
| a text message's row | 492 | 0 | 2,698 | 2,765 | 1.02 |
| a location message's row | 372 | 2 | 1,783 | 1,965 | 1.10 |
| a messages push of 100 text rows | 49,378 | 0 | 295,152 | 278,346 | 0.94 |
| the same, 90 text rows and 10 locations | 48,188 | 20 | 262,884 | 272,338 | 1.04 |
| `layout.set` of a book at every limit | 28,243 | 266 | 106,064 | 119,254 | 1.12 |
| 1,000 doubles of 16 to 17 digits | 19,263 | 1,000 | 27,432 | 37,968 | 1.38 |
| 1,000 decimals of 9 to 10 digits | 11,001 | 1,000 | 28,427 | 31,289 | 1.10 |
| 1,000 integers of 13 digits | 14,001 | 0 | 17,538 | 19,206 | 1.10 |

Integers parse by the same code either way, so their 1.10, and the 100 text rows' 0.94, are the noise at this load: about a tenth either way. Only the 17-digit doubles stand out of it, at about 10 ns each.

## The test to adopt

T1's, for `jawd/src/layout/book/tests.rs`, with its helpers there:

```rust
#[test]
fn of_two_slots_that_come_to_one_chat_the_one_higher_up_keeps_it_whatever_their_columns() {
    let known = knowing(&[(ANA, ANA), (ANA_LID, ANA), (CLIMBING, CLIMBING)]);
    // The group at the top left, Ana by her number below it, and Ana by her LID down the right,
    // whose slot starts higher than the one below the group.
    let shape = grid(
        &[0.0, 0.5, 1.0],
        &[0.0, 0.5, 1.0],
        vec![slot(1, [0, 1, 0, 1], Some(CLIMBING)), slot(2, [0, 1, 1, 2], Some(ANA)), slot(3, [1, 2, 0, 2], Some(ANA_LID))],
    );
    let (read, dropped) = resolved(unnamed(shape), known).unwrap();
    let chats: Vec<Option<&str>> = read.unnamed.as_ref().unwrap().slots.iter().map(|s| s.chat.as_deref()).collect();
    assert_eq!(chats, [Some(CLIMBING), None, Some(ANA)], "top to bottom first: the slot on the right starts higher");
    assert_eq!(dropped, [ANA]);
}
```

I ran it in my probes' module with a local `knowing` and `unnamed`: it passes at `a03782d`, and fails with the order by columns and with the slots' stored order.

## Verdict

Nothing in this range loses data or leaves jaw.db inconsistent: migration 17 keeps every row through every kill, books are answered only once a synced commit has them and never go back, and each format reads as the other with nothing kept depending on it. T1 is a test gap in which slot is shown a merged chat, not in what's kept; adopt its test with the trailers. O1 to O6 are follow-ups. Once this note is committed, every commit of the range touches the data layer and takes the trailer `Data-Audit: docs/audits/2026-10-28-layout-book.md`; when the branch lands after `chat-actions`, migration 17 becomes 18 by its lock line and its comment, as checked above.

Verdict: approved
