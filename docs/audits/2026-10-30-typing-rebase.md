# Audit: typing in chats rebased onto folders (`presence-typing`, round two)

Range: `5230b9f..b3b1df8e` in `presence-typing` (worktree `~/src/jaw-presence`), 30 October 2026. Auditor: database-expert. `5230b9f` was `origin/main` when the lander rebased: folders' M6 in jawd. Line numbers are at `b3b1df8e`, and paths are under `crates/` unless they say otherwise.

Round one (`docs/audits/2026-10-29-typing.md`, committed as `b3b1df8e`) approved `7262a15..8721bc7`. This round checks only what the rebase onto folders added beyond main's context and keeping both sides of each conflict: the lander's six changes (its report, `reports/typing-landing.md`), five of them in data-layer paths. Round one stands for the rest.

Everything ran in one clone of the worktree under `/tmp/jaw-audits/typing-2/`, with one build folder, `CARGO_INCREMENTAL=0`, `nice`, `TMPDIR` in the same folder, revisions and switches changed with `git checkout` there, and each switch turned on by `JAW_MUT` in one build. Only the end-to-end test's own `jawd --engine demo` ran; no Go engine was built or started, nothing under `~/Library/Application Support/jaw` was opened, and I stopped no process. Nothing was edited in the worktree but this note.

## In short

Nothing is marked data loss or inconsistency. Change 1 is two match arms that give the folder tree and rosters no chats for typing: their answers and pushes come out byte for byte as on main, typing never reaches them, chat lists of every folder and conversations still get it, and nothing is written. Changes 2 to 6 are a lint's allow, two tests' arguments, a test's hello and a bench's hello, each as the lander says. Folders' `Typing` arm is still empty, and nothing of folders reads typing. Everything in jawd's tests and fixtures says protocol 2.

Two things are worth taking with this landing, both test-only and printed at the end, verified: a test for change 1, which no test fails without (T4), and a version of round one's O5 test that waits for its answers and for each push rather than a fixed 300 ms. O5's test failed again here, in 1 of 3 runs of jawd's whole suite, as it did for the lander; I'd not leave it as a known flake (O5, below). Main has moved meanwhile, to the keepalive fix (`abfa15a4`): typing merges onto it cleanly, builds, and its tests and the end-to-end test pass there.

## What was asked, checked

### The range-diff

`git range-diff 7262a15..8721bc7 5230b9f..69b7823b`, and the same with each series squashed into one commit, so that the whole of typing as audited is compared with the whole of it as rebased:
- commit 3 (`d0cb9189`, end to end) is identical; commits 5 to 7 (`20481151`, `f90e0cfd`, `69b7823b`) differ only by the trailer;
- commit 2 (`fe5cb27a`, the terminal) differs by main's context only: folders' `children_unread` and `shown_unread()` beside its lines, and its test placed after folders' two;
- commit 4 (`3686699b`) by the trailer, the design's "Where it comes from" keeping both sides' clauses, and change 4;
- commit 1 (`fb1b2bea`) by the trailer, the conflicts kept from both sides (imports united; `run`'s calls, `main.rs`, the demo world's fields and tasks, `status`, the README and `status.result.json` with both sides' fields), and changes 1, 2, 3, 5 and 6.

File by file, typing's lines on main grow from round one's only in `feed/client.rs` (+5: the two arms, and `result`'s messages arm split over three lines), `sync/mod.rs` (+1, the allow), `sock.rs` (+3 −1, change 5) and `scripts/folders-bench.py` (change 6, new to the range). They shrink in `jaw-proto/tests/fixtures.rs`, `main.rs` and the tests' `run` calls, which main had already split over lines for folders' own additions, so typing adds a name or an argument where it added a line. Nothing else differs. `scripts/check-data-audit.py 5230b9f b3b1df8e` passes, and `cargo clippy -p jawd --all-targets -- -D warnings` passes at `fb1b2bea` and at `3686699b`, the two commits holding the six changes.

### Change 1: the folder tree and rosters

Typing reaches a connection's windows only through `Window::shows` and `Window::chats` (`feed/client.rs:886-901`): `Client::shows` (`:954`), each answer's typing (`typing_in(window.chats())`, `:1265` and `:1280`), what a connection forgets (`forget_unshown`, `:978`), what a lagging connection looks at again (`:1319`) and `flush_typing` (`:986`) all go through them, and the push path's chats coming in and leaving are worked out for chat lists only (`let list = matches!(window.spec, Spec::Chats(_))`, `:1459-1462`). The new arms give the tree and rosters no chats. So:
- **Their answers are main's.** `folders.watch` answers `FoldersWatched`, which has no `typing`. `roster.watch` answers `ChatsWatched` (`:679`, as on main), whose `typing` is `skip_serializing_if = "BTreeMap::is_empty"` and always empty for a roster, so its bytes are main's.
- **Their pushes are main's.** `push` (`:692-720`) is untouched: the `roster` and `folders` pushes are `ChatsPush` and `FoldersPush`, which have no `typing`.
- **They never get typing.** No chat is owed for them, no answer of theirs carries any, and their pushes neither bring a chat into typing's reckoning nor take one out.
- **Chat lists of every folder and conversations still get it.** Folders' chat lists, smart, yours and built in, are all `Spec::Chats`, whose rows are `chats` rows by JID (`feed/window.rs:133-156`, communities' groups included), so they go through the arm that was there.
- **Nothing is written.** The arms read nothing and call nothing.

Person and folder ids (`person:12`, `folder:3`, `all`) never equal a chat's JID, so arms that gave the tree and rosters their own row ids instead would change nothing either; my switches agree (below). The probe I wrote for this (T4) checks all of the above: with Ana typing in her chat, whose person the roster shows, and Rui recording in the group, the tree's and the roster's answers equal those with nobody typing and have only their own fields; a connection watching only them is told no typing while more comes, and their pushes, made by renaming Ana and a new message of hers, have only their own fields; then a chat list of the `groups` folder beside them is told the group's typing in its answer and by a push; and the feed's seq moves only for the two changes written.

### Changes 2 to 6

- **2, the allow on `sync::run`** (`sync/mod.rs:286`). Main's `run` already took seven arguments, clippy's limit, and typing adds the eighth; `store/lists.rs:740` has the same allow.
- **3, folders' sync test passes `crate::feed::Typing::default()`** (`sync/tests.rs:679`), an unstarted `Typing`. That's right while the arm drops typing, and the test still guards the arm: with the arm writing (`arm_writes`, typing taken as presence) it fails ("typing changed rows: … Ana …"). It stops being enough once the arm is wired (O10).
- **4, typing's sync-loop test passes `Default::default()` for `following`** (`sync/tests.rs:2631`), an unstarted `Following`, as folders' other sync tests do; it sends no presence.
- **5, folders' folder-methods test says hello with `PROTOCOL`** (`sock.rs:1225-1226`): diffed against main's test, the hello is the only difference.
- **6, `scripts/folders-bench.py:70` says protocol 2**, as a literal (O12).

### Folders' `Typing` arm

`sync/mod.rs:387` is `Some(pb::event::Kind::Typing(_)) => {}`, byte for byte main's, its comment untouched, so typing from WhatsApp is still dropped there until the presence half wires it. Nothing in `presence.rs`, `organise.rs`, `store/lists.rs` or `send/presence.rs` reads `feed::Typing`; its only holders are `main.rs`'s wiring, sync's Message arm (`:366`, round one's), the feed, the demo engine and `status`. Folders' schema also refuses typing or recording as anyone's presence (`store::write::folders::tests::jaw_db_refuses_typing_or_recording_as_anyones_presence_whoever_writes_it`, passing).

### The protocol is 2

`jaw_proto::PROTOCOL` is 2, and jawd refuses any other with error 6 (`sock.rs:600-622`). Every hello in jawd's and the clients' tests says `PROTOCOL`, except `hello_comes_first_and_must_match`'s, which asks for `PROTOCOL + 1` to be refused; the fixtures (`hello.request.json`, `hello.result.json`, `status.result.json`) say 2, as do `scripts/folders-bench.py` and JawKit (`c95fb047`). The only `"protocol": 1` left in Rust is `jaw-proto/src/settings.rs:124` and `:131`, a test that parses a `Welcome` and a `Status` and compares their version with nothing (O12). The engine link's `protocol: 1` (`engine/frame.rs:48`) is the link's own.

### The merged result's tests

At `b3b1df8e`, at a load of 55 to 66 on 20 cores:
- jawd's feed, sock, sync, presence, folders and typing tests: 217 passed and 3 failed of 220, in 199 s. The three pass alone, 3 of 3 each: `no_window_starves_while_the_others_keep_falling_due` and `a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push`, on the lander's list, and `sync::tests::an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean`, main's (below, Outside this range).
- All 20 typing tests and 37 folders tests among them passed.
- The end-to-end `crates/jaw/tests/typing.rs` passed, in 149 s, most of it rebuilding jawd.

### Switches

Twelve switches in one build, each turned on by `JAW_MUT`, run against the typing, folders, roster and tree tests (66 with the probe and O5's fixed version) unless they say otherwise:

| Switch | What it does | Fails |
| --- | --- | --- |
| `tree_shows` | the tree and rosters show every chat | only the probe: "typing pushed for 120363000000000000@g.us, which no window shows" |
| `tree_held_shows` | they show what their rows' ids name | none (equivalent: no id is a chat's) |
| `tree_chats_held` | their chats are their rows' ids | none (equivalent) |
| `tree_list` | the push path treats them as chat lists | none (equivalent) |
| `roster_typing_dropped` | the roster's answer leaves typing out | none (equivalent: it's always empty) |
| `chats_not_shown` | chat lists show no chat | 6, round one's and both new tests |
| `messages_no_chats` | conversations give no chat | 1, watching again from since |
| `arm_writes` | folders' arm takes typing as presence | folders' sync test |
| `arm_heard` | folders' arm calls `typing.heard` | none (O10) |
| `typing_writes` | typing's task writes a person each round | O5's test, both versions, on "the writer's transactions, synced commits and the feed's seq" |
| `slow_watch` | O5's interface watches 600 ms late | O5's original, 3 of 3, seeing 3 pushes; the fixed version passes 3 of 3 |
| `slow_connection` | a connection takes 40 ms over each round of typing | O5's original, 3 of 3, seeing 2 pushes; the fixed version passes 3 of 3 |

### Main has moved: the keepalive fix

`origin/main` moved during this audit, to `abfa15a4` (the keepalive fix, eleven commits). It meets typing in `sync/mod.rs`, `sync/tests.rs`, `send/tests.rs`, `demo/mod.rs` and the README, but adds no `sync::run` call, no hello and no change to the protocol's version. `git merge-tree` of `abfa15a4` and the branch (with the tests printed below) is clean; built, its typing, folders, roster, tree, sync and keepalive tests pass, 154 of 155, the one failure the same engine-stop test as above, passing alone 3 of 3; and the end-to-end typing test passes on it (67 s). Its one change of behaviour that meets typing: a keepalive timeout makes `connection` say `connecting` with the connection still open. That makes typing's `hearing` false, ending everyone's typing, and once `connection` says online again presence forgets what WhatsApp was told and tells it you're online (`send/presence.rs:60-67`), after which typing is heard again: typing ends for as long as WhatsApp can't be reached, as the protocol says.

## Findings

### Data loss

None. Nothing in the six changes reaches the writer, the inbox, an ack, the spool or a file.

### Inconsistency

None. The tree's and rosters' answers and pushes are main's, and typing moves no `v` or `seq` with them watched, as without.

### Tests (missing)

- **T4, test gap. Change 1's rule has no test.** With the tree and rosters taken to show every chat (`tree_shows`), all of the branch's and folders' tests pass, and a connection watching only them is then pushed others' typing for chats none of its windows shows. It doesn't block, as round one's T1 to T3 didn't: it's a path of a tested rule (typing goes only to the chats a connection's windows show, which every typing test's model checks), and it decides only what an interface is told of typing, which jaw.db never holds. **Fix:** adopt `the_tree_and_a_roster_are_told_no_typing_while_a_folders_chat_list_beside_them_is` (printed below). It fails 1 of 1 with the switch, passes 3 of 3 alone, 10 of 10 beside O5's two versions, 5 of 5 as printed, and in each of three runs of jawd's whole suite.

### Unpredictable performance

None. The new arms are constant work, and a connection's tree and rosters add nothing to what typing's rounds cost it.

### Other

- **O5, other: a racing test, round one's, now measured, with its fix. It shouldn't stand as a known flake.** `typing_writes_nothing_to_jaw_db_and_moves_no_seq_while_interfaces_are_told_it` (`feed/tests.rs:4994`) gives its interface a fixed 300 ms to watch (`:5022`), then wants at least four `typing` pushes (`:5037`). It rests on three timings: the watches answered within 300 ms; the connection sending each change before the next comes 20 ms (a pace) later, since two changes of one chat 20 ms apart merge into one push; and no gap of 3 s between pushes (`came(ms(3_000))`). Measured: it failed in 1 of 3 runs of jawd's whole suite here (load 52 to 76), seeing 3 pushes, as it did in the lander's one run; 3 of 3 with the watches 600 ms late (`slow_watch`, 3 pushes, the lander's failure exactly); and 3 of 3 with the connection 40 ms slow to take each round (`slow_connection`, 2 pushes), with the watches on time, so waiting only for the answers, round one's fix, isn't enough. **Fix:** the version printed below waits, `TYPED` (15 s) at most each, for both answers and then for each push in turn before the next change, so nothing merges and no fixed time is left; its interface hands on each line as it comes, and the test closes the connection at its end. It passes 3 of 3 under each of the two switches, 10 of 10 alone, and in 3 of 3 whole suites; it still fails when typing writes (`typing_writes`), on the same assertion; and it takes 1.8 s where the original took 6. **My judgement: take it with this landing.** It's the branch's own test, and the one test that typing reaching an interface writes nothing; when it fails it fails before its nothing-written checks, so on a loaded run, the gate's, the rule goes unchecked; and each failure teaches whoever lands next to run the data-integrity suite again until it passes. It can't pass when typing writes, so it doesn't block the approval, and the fix is test-only and verified here, so it needs no other round.
- **O10, other, for the presence half. Folders' sync test hands `run` an unstarted `Typing`** (change 3, `sync/tests.rs:679`). Once the presence half wires `typing.heard` into the arm, that test as it stands would pass without typing's task ever running (`arm_heard` passes; so would typing writing, since no task runs to write), so it would no longer check that typing heard from WhatsApp writes nothing. **Fix, then:** give it a started `Typing`, as `a_message_from_someone_ends_their_typing_in_its_chat_as_sync_takes_it_from_the_engine` does.
- **O11, other, for the presence half. No roster shows typing yet.** The protocol's roster row says "typing and recording show from the chat's typing", and the terminal draws a roster row's typing only from the row's own `presence.state` (`jaw/src/view/sidebar.rs:549-550`), which jawd never sets to typing. With change 1, a connection is told a chat's typing only while a chat list or a conversation shows the chat. So a roster row never shows typing, as on main, whose arm drops it. The presence half decides whether a roster's people's chats count as shown, or interfaces draw a row's typing from the chats' typing they hold.
- **O12, other (hygiene).** `jaw-proto/src/settings.rs:124` and `:131` still say `"protocol": 1`; nothing compares them with anything, but they could say 2. `scripts/folders-bench.py:70` says 2 as a literal, where the Rust tests say `PROTOCOL`, so it will need changing at 3. The three comments the lander lists as stale are the presence half's to rewrite: the arm's "Until it lands, it goes no further" (`sync/mod.rs:385-386`), `engine/mod.rs`'s "until the link carries … M6's `Typing` event", and the README's "presence comes once M6's folders are on main".

### Outside this range

- `sync::tests::an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean` failed once at the head and once on the trial merge, each in a run of many tests at a load of 60 to 70, and passes alone 3 of 3 each time. It times `stop` against `DISCONNECTING` and 500 ms; the branch changes neither it nor anything it calls (it doesn't run `sync::run`). It's main's, for the owner's list.
- In the three whole suites, the other failures were all on the lander's list: the long-request lane (3 of 3), connection-full (3 of 3), restart (1), starvation (1) and the WAL bound (1).
- JawKit's `FixtureTests.swift:78` will fail once this lands, as the lander reported; it's the Mac's typing step's, not the data layer's.

## Measurements

- **The head's tests:** above, 217 of 220 in 199 s at a load of 61 to 66, the three failures passing alone 3 of 3; the end-to-end test 149 s.
- **Whole suites:** jawd's 700 unit tests (the head's, the probe and O5's fixed version, switches compiled in and none on), three runs one after another, 906, 962 and 922 s, at a load of 52 to 76: 684, 683 and 684 passed, 13 ignored. O5's original failed in run 2; its fixed version and the probe passed in all three.
- **The printed tests,** on `b3b1df8e` with nothing else changed: `cargo fmt --check` and `cargo clippy -p jawd --all-targets -- -D warnings` pass; the two tests pass 5 of 5 alone, and the typing, folders, roster and tree tests 65 of 65 with them. O5's fixed version takes 1.79 and 1.89 s against the original's 6.15 and 5.75 s, alone.
- **The trial merge onto `abfa15a4`:** 154 of 155 tests in about 130 s, and the end-to-end test in 67 s. Commands sometimes run twice here: that one did, so two end-to-end runs went at once, each with its own jawd in its own folder, and both passed (41 and 67 s). Nothing of mine was left running.

## The tests to adopt

In `crates/jawd/src/feed/tests.rs`, as one commit, test-only: O5's test made to wait for its answers and each push, and T4's test. It applies to `b3b1df8e`, and to the branch rebased onto `abfa15a4`, which doesn't touch that file.

```diff
@@ -4990,6 +4990,24 @@ async fn a_connection_whose_interface_reads_nothing_holds_one_typing_for_each_ch
     assert!(stats.merged >= 100, "{stats:?}");
 }
 
+/// Waits until the interface whose lines come on `lines` is pushed `names` typing in `chat`,
+/// passing over what comes first, for `TYPED` at most.
+async fn pushed_typing(lines: &mut mpsc::UnboundedReceiver<Value>, chat: &str, names: &[&str]) {
+    let end = Instant::now() + TYPED;
+    let mut came = Vec::new();
+    loop {
+        let Ok(Some(line)) = tokio::time::timeout_at(end.into(), lines.recv()).await else {
+            panic!("the interface was never pushed {names:?} typing in {chat}, only {came:?}");
+        };
+        let who: Vec<&str> =
+            line["params"]["who"].as_array().into_iter().flatten().filter_map(|who| who["name"].as_str()).collect();
+        if line["method"] == "typing" && line["params"]["chat"] == chat && who == names {
+            return;
+        }
+        came.push(line);
+    }
+}
+
 #[tokio::test(flavor = "multi_thread")]
 async fn typing_writes_nothing_to_jaw_db_and_moves_no_seq_while_interfaces_are_told_it() {
     let (bed, typing, hearing) = typing_bed(ms(400)).await;
@@ -5011,33 +5029,128 @@ async fn typing_writes_nothing_to_jaw_db_and_moves_no_seq_while_interfaces_are_t
     let version = |db: &rusqlite::Connection| db.query_row("PRAGMA data_version", [], |r| r.get::<_, i64>(0)).unwrap();
     let (version_before, wal_before) = (version(&side), wal());
 
-    // An interface watches its chat list and the group while Ana types and stops, Rui records and
-    // Zé types there, a message of Rui's ends his, Zé's fades, and Ana's ends as you go away.
+    // An interface watches its chat list and the group, then hands on each line jawd sends it.
+    let (watched, answered) = tokio::sync::oneshot::channel();
+    let (came, mut lines) = mpsc::unbounded_channel();
     let watching = tokio::task::spawn_blocking(move || {
         let (mut line, _) = Line::open(&socket, None);
         line.call("chats.watch", json!({"around": "top", "after": 3}));
         line.call("messages.watch", json!({"chat": GROUP, "around": "newest", "before": 5}));
-        line.came(ms(3_000))
+        let _ = watched.send(line.stream.try_clone().unwrap());
+        for push in std::mem::take(&mut line.pushes) {
+            let _ = came.send(push);
+        }
+        line.stream.set_read_timeout(Some(TYPED)).unwrap();
+        let mut text = String::new();
+        while line.reader.read_line(&mut text).is_ok_and(|n| n > 0) {
+            let _ = came.send(serde_json::from_str::<Value>(&text).unwrap());
+            text.clear();
+        }
     });
-    tokio::time::sleep(ms(300)).await;
+    let stream = tokio::time::timeout(TYPED, answered).await.expect("its watches were answered").unwrap();
+
+    // Ana types and stops, Rui records and Zé types in the group, a message of Rui's ends his,
+    // Zé's fades, and Ana's ends as you go away, each once the interface has been told the last.
     typing.heard(OPEN, OPEN, Composing);
+    pushed_typing(&mut lines, OPEN, &["Ana"]).await;
     typing.heard(OPEN, OPEN, Paused);
+    pushed_typing(&mut lines, OPEN, &[]).await;
     typing.heard(GROUP, &person(9), Recording);
     typing.heard(GROUP, &person(10), Composing);
-    typed(&typing, GROUP, &["Rui", "Zé"]).await;
+    pushed_typing(&mut lines, GROUP, &["Rui", "Zé"]).await;
     typing.message(GROUP, &person(9), "");
-    typed(&typing, GROUP, &["Zé"]).await;
-    typed(&typing, GROUP, &[]).await;
+    pushed_typing(&mut lines, GROUP, &["Zé"]).await;
+    pushed_typing(&mut lines, GROUP, &[]).await;
     typing.heard(OPEN, OPEN, Recording);
-    typed(&typing, OPEN, &["Ana"]).await;
+    pushed_typing(&mut lines, OPEN, &["Ana"]).await;
     hearing.send_replace(false);
-    typed(&typing, OPEN, &[]).await;
-    let came = watching.await.unwrap();
-    let pushed = came.iter().filter(|line| line["method"] == "typing").count();
-    assert!(pushed >= 4, "the interface was told: {came:?}");
+    pushed_typing(&mut lines, OPEN, &[]).await;
+    stream.shutdown(std::net::Shutdown::Both).unwrap();
+    watching.await.unwrap();
 
     assert_eq!(writes(&bed), before, "the writer's transactions, synced commits and the feed's seq");
     assert_eq!(version(&side), version_before, "nothing committed to jaw.db");
     assert_eq!(wal(), wal_before, "nothing written to its WAL");
     assert_eq!(typing.stats().chats, 0);
 }
+
+/// The tree's and the roster's answers on a fresh connection, their watch ids left out.
+async fn tree_and_roster_answers(bed: &Bed) -> (Value, Value) {
+    let mut watcher = bed.watcher(64);
+    let mut tree = watcher.call("folders.watch", json!({})).await.unwrap();
+    let roster = json!({"folder": "all", "around": "top", "after": 20});
+    let mut roster = watcher.call("roster.watch", roster).await.unwrap();
+    for answer in [&mut tree, &mut roster] {
+        answer.as_object_mut().unwrap().remove("watch");
+    }
+    (tree, roster)
+}
+
+/// The folder tree and a roster show folders and people, not chats, so typing reaches neither:
+/// they answer and push as they do with nobody typing, while a folder's chat list beside them is
+/// told who's typing in its chats, and nothing is written.
+#[tokio::test(flavor = "multi_thread")]
+async fn the_tree_and_a_roster_are_told_no_typing_while_a_folders_chat_list_beside_them_is() {
+    let (bed, typing, _hearing) = typing_bed(NEVER).await;
+    let quiet = tree_and_roster_answers(&bed).await;
+    let committed = *bed.store.committed().borrow();
+    typing.heard(OPEN, OPEN, Composing);
+    typing.heard(GROUP, &person(9), Recording);
+    typed(&typing, OPEN, &["Ana"]).await;
+    typed(&typing, GROUP, &["Rui"]).await;
+
+    // Ana types in her chat, whose person the roster shows, and Rui records in the group: the
+    // tree and the roster answer as they did with nobody typing, with no typing of their own.
+    let typed_in = tree_and_roster_answers(&bed).await;
+    assert_eq!(typed_in, quiet, "the tree's and the roster's answers with someone typing");
+    let keys = |answer: &Value| {
+        let mut keys: Vec<String> = answer.as_object().unwrap().keys().cloned().collect();
+        keys.sort();
+        keys
+    };
+    assert_eq!(keys(&typed_in.0), ["favourites", "rows", "seq"], "{}", typed_in.0);
+    assert_eq!(keys(&typed_in.1), ["rows", "seq", "total"], "{}", typed_in.1);
+    let ana = typed_in.1["rows"].as_array().unwrap().iter().find(|row| row["chat"] == OPEN);
+    assert!(ana.is_some(), "the roster shows Ana, whose chat she types in: {}", typed_in.1);
+
+    // A connection watching only them is told none of the typing that follows, and their pushes
+    // say what they say with nobody typing.
+    let mut watcher = bed.watcher(64);
+    watcher.watch("folders.watch", json!({})).await;
+    watcher.watch("roster.watch", json!({"folder": "all", "around": "top", "after": 20})).await;
+    typing.heard(GROUP, &person(10), Composing);
+    typing.heard(OPEN, OPEN, Paused);
+    typed(&typing, GROUP, &["Rui", "Zé"]).await;
+    typed(&typing, OPEN, &[]).await;
+    typing.heard(OPEN, OPEN, Recording);
+    typed(&typing, OPEN, &["Ana"]).await;
+    let renamed = Change::Person(named(OPEN, "Ana Ribeiro"));
+    let written = Change::Message(Message { live: true, ..text(OPEN, "t-roster", 20_000, Some(OPEN), "olá") });
+    let seq = bed.apply(vec![renamed, written]).await;
+    watcher.settle(seq).await;
+    watcher.run_for(ms(300)).await;
+    assert!(watcher.told.is_empty(), "typing told to the tree and a roster: {:?}", watcher.told);
+    let methods: Vec<&str> = watcher.pushes.iter().map(|(method, _)| method.as_str()).collect();
+    assert!(methods.contains(&"folders") && methods.contains(&"roster"), "{:?}", watcher.pushes);
+    for (method, params) in &watcher.pushes {
+        let keys = keys(params);
+        let theirs: &[&str] = match method.as_str() {
+            "folders" => &["favourites", "remove", "seq", "upsert", "watch"],
+            "roster" => &["remove", "seq", "total", "upsert", "watch"],
+            other => panic!("a {other} push to the tree and a roster"),
+        };
+        assert!(keys.iter().all(|key| theirs.contains(&key.as_str())), "a {method} push: {params}");
+    }
+
+    // A chat list of the groups folder beside them is told who's typing in its chats: by its
+    // answer, then by a push as it changes.
+    let groups = json!({"folder": "groups", "around": "top", "after": 3});
+    let groups = watcher.call("chats.watch", groups).await.unwrap();
+    assert_eq!(answered_typing(&groups), want(&[(GROUP, &["Rui", "Zé"])]), "{groups}");
+    typing.heard(GROUP, &person(9), Paused);
+    watcher.until_typing(&[(GROUP, &["Zé"])]).await;
+    let told: Vec<&str> = watcher.told.iter().map(|(chat, _)| chat.as_str()).collect();
+    assert_eq!(told, [GROUP], "only the group's chat list is told, once");
+    assert_eq!(*bed.store.committed().borrow(), seq, "typing wrote nothing past the two changes");
+    assert!(committed < seq);
+}
```

## What's kept

This auditor's store holds, in `~/Library/Application Support/Cursor/AgentStores/cursor_agent_stores/1ed70e92-18c6-4a50-9de6-c3c498461792/files/typing-round2/` (64 KB): `adopt-tests.diff`, the patch above, which `git apply` takes on `b3b1df8e`; `switches-probes-and-tests.diff`, also on `b3b1df8e`, with the twelve switches, the probe and both versions of O5's test; the scripts that ran them; and the runs' results. `/tmp/jaw-audits/typing-2/` (the clone, its build folder and the logs) is deleted.

## What the lander does

1. Take the tests above as one commit, after `69b7823b` or at the branch's end, with the trailer `Data-Audit: docs/audits/2026-10-30-typing-rebase.md`. If the owner would rather land without them, T4 and O5 stay follow-ups; the approval doesn't depend on them.
2. Give `fb1b2bea` (changes 1, 2, 3, 5 and 6) and `3686699b` (change 4) the trailer `Data-Audit: docs/audits/2026-10-30-typing-rebase.md` too, beside round one's, which they keep. No other commit takes it: `20481151`, `f90e0cfd` and `69b7823b` differ from round one's only by their trailer, and `fe5cb27a`, `d0cb9189`, `c95fb047` and `b3b1df8e` touch no data-layer path.
3. Commit this note, and run `make check-audit`: it checks every `Data-Audit` trailer a commit names.
4. Main has moved: `origin/main` is `abfa15a4`, the keepalive fix. Rebase onto it. Then check that `git range-diff` against this range shows only main's context and its keep-both resolutions (my trial merge found no conflict); that `rg '"protocol"\s*:\s*1' crates scripts` finds only `jaw-proto/src/settings.rs`; that jawd's feed, sock, sync, presence, folders and typing tests, and the end-to-end typing test, pass; and the gate.
5. If `m3-media-in` lands before this: its three new `sync::run` calls (`media/bench.rs:71`, `media/tests.rs:83` and `:2514`) take `crate::feed::Typing::default()`, which the compiler will ask for; its new hello at `media/bench.rs:181` says `"protocol": 1` and must say `jaw_proto::PROTOCOL`, which nothing will ask for, since jawd refuses it with error 6 only when that bench runs; and the README's `status` row and `status.result.json` keep both sides' fields. Run the checks in 4 again. If this lands first, media-in's lander does the same.
6. Anything else the range-diff shows, beyond main's context and keeping both sides, comes back to the database expert before landing.

## Verdict

Approved. The six changes write nothing, keep the tree's and rosters' answers and pushes as main's, keep typing to the chat lists and conversations that show its chats, and leave folders' arm dropping typing; the protocol is 2 throughout; and the branch's and folders' tests pass on the merged result and on main's newer `abfa15a4`. Nothing is marked data loss or inconsistency. T4 is a path of a tested rule, about what an interface is told of typing, and its test is printed above, verified; O5's test should be replaced by the version printed with it; O10 and O11 are the presence half's.

Verdict: approved
