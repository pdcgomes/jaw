# Audit: presence landing on main after the library (`presence-land-trial` onto `77d44257`)

Range: `77d44257..154979a2` in `presence-land-trial` (worktree `~/src/jaw-presence-2`): the eight commits approved in round one of `docs/audits/2026-10-31-presence.md` as `e12d8d34..a637b061` in `presence`, rebased onto `origin/main` at `77d44257` once the library landed, 1 November 2026. Auditor: database-expert.

Only the landing is in scope: what the rebase onto the library does beyond keeping both sides. Everything else stands as the presence note approved it. Paths are under `crates/jawd/src/` unless they say otherwise, and line numbers are at `154979a2`.

## In short

Approved. Nothing is marked data loss or inconsistency.

- **The range-diff shows the lander's three changes and nothing else but the library's context.** Mine (`git range-diff e12d8d34..a637b061 77d44257..154979a2`) is byte for byte the lander's, 329 lines.
  - Commits 4, 6, 7 and 8 are identical.
  - Commits 1 and 5 differ only in the library's context: `Update`'s `counts, note`, the README's `watch.move` row, and the demo's `paging` and `shown`.
  - Commit 2 adds only the six `""`, and commit 3 only the rename, the two `Spec::Library(_)` arms and the library's context.
  - One hunk of presence's goes with nothing in its place, rightly: `presence: _` in `push()`. The library's `push()` already ends its pattern with `..` (`feed/client.rs:780`), which leaves presence's field out just as presence's own line did.
- **The rename is right.** The two fields named `chats` have different types, so the compiler holds every reader to its own. The only places a swap could compile are `found()` and `settle()`, and switching off any of their four lines fails a test.
- **Each kind of window gets exactly the typing and presence it should** (the table below). Presence changes never re-read a library window.
- **The six `""` are inert, and stand on their own.** The places rule doesn't cover them; the conditions under which they stand are below. Without them, the typing commit fails with exactly six E0061 errors, at those six calls.
- **Both branches' rules still fail their tests when switched off.** I ran 41 switches in one build, twice each:
  - every one of the library's at the meeting points, and every one of presence's (its round one's, the lander's three, and those at the meeting points), fails a test;
  - so do main's two typing rules in the code presence moved into `shown()`;
  - two pass: `C_presence_kept`, as in round one (its O6), and a probe of my own (T1).
- **The full suite passes, but for three tests on the landers' list,** and each of them passes alone 3 of 3 (O2). Clippy with `-D warnings` and rustfmt pass, the typing commit and the presence-in-chats commit each build, and the latter's plan tests pass.
- **One test gap, the library's rather than this landing's (T1).** Nothing tests that a library window shows nobody's presence. The rule holds in main's code, which this landing doesn't change, and a test for it is in the store.

## The three changes

### The two fields named `chats`

- `Found` (`feed/client.rs:255`, `:260`) and `State` (`:545`, `:549`) now hold two fields:
  - the library's `chats: HashSet<i64>`, the chats a library window's rows are in, by n;
  - presence's `roster_chats: HashSet<String>`, a roster's people's one-to-one chats, by JID.
- `found()` fills each from its own read: `chats` from the library's (`:473`), and `roster_chats` from a roster's rows (`:451-454`). Each is empty for every other kind of window, and the window's bytes count both (`:457`, `:459`).
- `settle()` copies both into `State` (`:742-743`).
- Only the library's arm of `touched_by()` reads `chats` (`:987`), and only `shown()` and `Window::shows()` read `roster_chats` (`:670`, `:1015`).

### A library window, a conversation, a roster and a chat list

| Window | Typing (answer and pushes) | Presence (answer and pushes) | Whose presence it shows, so jawd follows |
| --- | --- | --- | --- |
| Library | none (`shown()` `:671`, `Window::shows()` `:1016`) | none (`Window::presence()` `:1026-1031`) | nobody (T1) |
| Conversation | its chat | a one-to-one chat's person's (`feed/window.rs:179-182`); a group's none | its person, first |
| Roster | its people's one-to-one chats | none of its own; its rows carry each person's, as on main | its people |
| Chat list | its rows' chats, as on main | none | nobody |

- **A library window.**
  - `LibraryWatched` has neither field, so no answer of a library window can carry typing or presence. The library's test pins its answers' keys, and its test `Watcher` panics at any typing or presence push for a chat no window shows (`feed/tests.rs:268-292`).
  - Its read leaves `people` and `presence` empty (`feed/window.rs:251-261`). So `keep()`'s `Showing`, main's line (`:1036`), shows nobody's presence for it.
- **Presence changes re-read no library window.** A presence event marks only `Changes::presence` (`feed/mod.rs:261-263`), which the library's arms of `touched_by()` don't read (`:981-988`). Presence's one addition to `touched_by()` is in the conversation's arm (`:976`).
- **The library note's re-checks hold:**
  - `Kind::of`, `result()`, `push()`, `touched_by()`, `shows()` and `shown()` each keep the library's arm.
  - The three `matches!` that pick out conversations are main's lines (`:995`, `:1036`, `:1654`).
  - The fourth place, the chats a push brings in for typing, is now presence's `shown()`-based `came` and `left` (`:1721-1724`). It gives a library window none, as main's `list` did.

### The six `""`

- **Where:** `library_windows_are_told_no_typing_while_a_conversation_beside_them_is` (`feed/tests.rs:5282`, `:5283`, `:5310`, `:5311`, `:5314`, `:5338`). They're folded into the typing commit, so every commit builds.
- **What `""` does.** It's what jaw-engine sends when WhatsApp gives no other JID (`feed/typing.rs:133`).
  - `heard()` watches an other JID only when it isn't empty (`:145`), and `jids()` and `mine()` drop an empty one (`:310-317`). So `name()`, `said()`, `chat()` and the task's `waiting` (`:579-593`) see one sender, as main's three-argument `heard()` did.
  - For this test's inputs (Ana typing in her own chat by its JID, and two people in a group), each takes main's path: the same chat found, the same typist, the same JIDs watched.
- **The places rule** (`docs/audits/2026-10-31-places-landing.md`) **doesn't cover it.**
  - Its condition 1 needs a parameter main added; this one is the branch's.
  - Its condition 4 leaves out a test that uses typing, and this is a typing test.
- **It stands on its own,** on these grounds:
  - **It's already approved elsewhere.** The parameter, and `""` at all 31 of main's `heard()` calls in `feed/tests.rs`, are in presence's typing commit, which the presence note approved. These six are one more such call, written on main after presence's base.
  - **It's test code only,** nothing else changes, and the test names no typist by another JID.
  - **The compiler shows it's complete.** Without the six, `cargo check -p jawd --all-targets` at `6f48f5bf` fails with exactly six E0061 errors, at those calls. With them it passes, and the test passes at that commit and at the head.
  - **The test still guards its rule:** switching off either of the library's typing rules fails it, as on main.
- **For later landings,** the same holds when all of these do:
  - a branch's own new parameter is passed, at a call in a test that main added after the branch's base, the value the branch passes at main's other such calls;
  - that value means "none" in production;
  - the test doesn't use what the parameter adds.

  Anything beyond that comes back to the database expert.

## Findings

### Data loss

None.

### Inconsistency

None.

### Test gap

- **T1 (follow-up, the library's): nothing tests that a library window shows nobody's presence.**
  - **Where:** the library's read (`feed/window.rs:251-261`), which leaves `people` empty, and `keep()`'s `Showing` (`feed/client.rs:1036`). Both are main's lines.
  - **What happens:** a probe that gives a library window its senders as its `people`, as a roster has its people, fails none of the eight tests run against it. Among them are the library's typing test, `whose_presence_the_windows_show_is_an_open_chats_person_first_then_a_rosters_people` and `presence_reads_again_only_the_rosters_and_the_persons_own_conversation`. With it, jawd would spend its 20 a minute and 200 a connection on people whose presence nothing shows.
  - **Why it doesn't block:** the rule holds at the trial, in code this landing doesn't change, and it's untested on main too. It decides only whom jawd asks WhatsApp to follow, never what's written.
  - **Fix:** `integration/presence-landing-t1.diff` in the agent store, 24 lines for `feed/tests.rs`.
    - Library windows of a chat, of a person and across chats leave `showing()` empty. A conversation beside them shows its person alone, and once it's closed, nobody's shown again.
    - It passes 3 of 3 at `154979a2`, and fails 3 of 3 with the probe on.
    - It belongs with the library's follow-ups. It could also land now, as a tests-only commit after this note's, carrying this note's trailer.

### Unpredictable performance

None.
- The landing adds no statement and moves no write.
- `plans.txt` changes only in the presence-in-chats commit, by presence's five lines for `chat_presence`, two key lookups. The plan tests pass there and at the head.

### Other

- **O1. `make check-audit` can't see the integration.** The two commits it changed keep the presence note's trailer, so the check passes them as it would the approved ones. **Fix:** this note's trailer beside the presence note's on those two (Landing, step 2).
- **O2. The flakes list.**
  - My full run failed three tests, each on the landers' list, and each passed alone 3 of 3:
    - the long-request lane, `feed::tests::a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most`;
    - the restart test, `send::tests::a_restart_gives_a_message_waiting_for_a_receipt_only_the_time_it_had_left`;
    - the socket's connection-full test, `sock::tests::a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push`.
  - `a_watch_result_waiting_for_room_comes_before_anything_its_window_sends` passed.
  - The lander's run failed `a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends`. It has been watched since its first sighting on 1 November at 08:42, so this was its second, and by that note's own rule it goes on the list. It passed in my run.

## Measurements

**Method:**
- **Scratch:** one folder, `/tmp/jaw-audits/presence-landing/`, deleted once this note was written.
  - It held one clone of `~/src/jaw-presence-2` at `154979a2`, with `presence` at `a637b061`.
  - Its `target` was the only build folder, built with `CARGO_INCREMENTAL=0`, with `TMPDIR` and Go's build cache inside the folder.
  - Revisions were switched with `git checkout`, and the tree was checked out clean after each change of mine.
- **Main:** GitHub's main was `77d44257` at 13:07 UTC (`git ls-remote`).
- **Bounds:** every build and test ran under `nice -n 10`, the tests under `gtimeout --kill-after`.
- **Switches:** applied by script and turned on by `JAW_MUT`, all in one build. The runner took a lock for each switch and round, and each of the 84 runs ran once.
- **Machine:** Apple M1 Ultra, 20 cores. Other agents kept the load at 33 to 65.
- **What I didn't touch:**
  - I opened nothing under `~/Library/Application Support/jaw` and copied no `session.db`.
  - The only Go engine that ran was the one a jawd test starts unlinked in a temporary folder, where it says Unlinked and connects nowhere (`engine/internal/wa/engine.go:147-151`). The Go tests' two link attempts fail before connecting.
  - Nothing was sent to WhatsApp, and I signalled no process.
  - In `~/src/jaw-presence-2` I only read.

**Runs:**

| Step | Result | Took |
| --- | --- | --- |
| `go test -race ./...`, after building the engine as `make test` does | 3 packages ok | 88 s |
| `cargo test --workspace --no-fail-fast` | Every binary passes but jawd's unit tests: 908 passed, 3 failed (O2), 16 ignored. Among those that pass: the terminal's 335, composing 3, failures 10, settings 3 and typing 2; jaw-client 26 and 3; jaw-proto 16 and 3 fixtures; jawd's installed 1, starting 1, stopping 4 and upgrading 1 | 41 min |
| The three failures alone, 3 runs each | 9 of 9 pass | 2 to 10 s each |
| Switches, 2 rounds each | the baseline passes 23 of 23 twice; 39 of 41 fail their tests 2 of 2 (below) | 17 min |
| At `6f48f5bf` without the six `""`: `cargo check -p jawd --all-targets` | exit 101: six E0061 errors, at the six calls, nothing else | 30 s |
| At `6f48f5bf` as it is: the check, then the library's typing test and main's, 3 runs | 0; 2 of 2 pass each run | |
| At `5a24f044`: the check, the three plan tests, then five meeting-point tests, 2 runs | 0; 3 of 3 pass; 5 of 5 pass each run | |
| T1's test at `154979a2`, without and with the probe, 3 runs each | passes 3 of 3; fails 3 of 3 | |
| `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` | 0, 0 | |
| `scripts/check-data-audit.py 77d44257 154979a2` | 0 | |

**The switches:**

| Switches | Rule switched off | What fails |
| --- | --- | --- |
| `L_shows`, `L_chats` | a library window shows no chat for typing | the library's typing test |
| `L_found_chats`, `L_state_chats`, `L_touched_all_chats` | the library's `chats`, carried through `found()` and `settle()` and read in `touched_by()` | T5's renamed-chat test |
| `L_kind_of`, `L_push_as_messages`, `L_result_counts` | the library's own kind, push and answer | 1 or 2 of the library's tests |
| `L_state_counts`, `L_push_counts_always` | counts are kept, and sent only when they change | T4's test |
| `L_flush_library_first` | conversations are flushed before the library | T7's test |
| `P_found_roster_chats`, `P_settle_roster_chats`, `P_roster_shown`, `P_roster_shows` | a roster shows its people's chats for typing | the tree and roster typing test |
| `P_settle_presence`, `P_result_presence` | a conversation's presence is kept and answered | 2 or 3 of presence's tests |
| `P_came_none` | a chat a push brings in is told who's typing | main's test of it |
| `M_chats_shown`, `M_conv_shown` | main's chat list and conversation typing, in `shown()` | main's typing test, and the roster's or the library's |
| round one's `A_*` (3), `B_*` (10), `C_presence_not_marked`, `C_answer_untold` and `E_arm_no_alt`, and the lander's `X_*` (3) | presence's rules, as round one switched them | the tests round one found |
| `C_presence_kept` | closed conversations' presence is forgotten | none, as in round one (its O6: memory only) |
| `L_lib_people`, a probe | a library window shows nobody's presence | none (T1) |

## Verdict

Nothing is marked data loss or inconsistency. The rebase keeps both sides except for the three changes the lander proposed, and each is right:
- the rename keeps each field's reads its own;
- the library window's arms give it no typing and no presence, as the library has it;
- the six `""` make the library's test run as it did on main.

Every rule this landing touches fails a test with its rule switched off, at the meeting points and in both branches. The full suite passes under the flakes rule. T1 is a follow-up of the library's, with its test ready; O1 is the landing's trailer, and O2 is for the flakes list.

## Landing

In `~/src/jaw-presence-2`:

1. **Move `presence` to the trial:** `git switch presence && git reset --hard presence-land-trial`. HEAD should be `154979a2`, and `git status` clean.
2. **The trailers.**
   - The typing commit `6f48f5bf` and the presence-in-chats commit `5a24f044` each take `Data-Audit: docs/audits/2026-11-01-presence-landing.md`, beside `Data-Audit: docs/audits/2026-10-31-presence.md`. Each trailer appears once, and `Co-authored-by` once.
   - Run `git rebase -i origin/main` with `edit` on those two picks. At each stop, run `git -c trailer.ifexists=addIfDifferent commit --amend --no-edit --trailer "Data-Audit: docs/audits/2026-11-01-presence-landing.md"`.
   - No other message or tree changes.
3. **The notes.**
   - The presence note is committed already, in the eighth commit.
   - Commit this note after it as `docs/audits/2026-11-01-presence-landing.md`, byte for byte from the store's `integration/2026-11-01-presence-landing.md`, docs only.
   - If T1's test is taken now, it's a tests-only commit after that one, from `integration/presence-landing-t1.diff`, with this note's trailer.
4. **Check before the gate.**
   - `git range-diff 77d44257..154979a2 origin/main..HEAD` shows only the two trailers, and the new commit or commits.
   - Each rebased commit's tree is the trial's.
   - `make check-audit` passes.
5. **The full gate:** `make check-generated`, `make lint`, `make build`, `make test` and `make check-audit`, under the flakes rule.
   - The three tests in O2 may fail, if each then passes alone 3 of 3.
   - Any other failure stops the push.
6. **The push:** from this worktree, through the pre-push hook, never with `--no-verify`.
7. **If main moves first:** rebase again, and do the presence note's re-checks (its Landing, step 4). Anything beyond keeping both sides in a data-layer path comes back to the database expert.
8. **After landing:** the cleanup the lander's report lists, T1 to the library's follow-ups unless it was taken, and O2 to the flakes list.

Verdict: approved
