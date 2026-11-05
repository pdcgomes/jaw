# Starting jawd when nothing answers (parity step 1.2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Mac release starts the jawd inside it when nothing answers, as the terminal does, and Jaw › jawd at Login… goes: [the parity plan](2026-10-27-jaw-mac-parity-plan.md)'s step 1.2 as the owner chose to build it on 30 October, by his decisions of 29 October in the jawd lifetime design ("Now, by your 29 Oct decisions: start the bundled jawd when nothing answers, say Starting jawd, and remove jawd at Login… and the takeover; --for-apps and the app flag follow once jawd has them"). A release starts `Contents/Helpers/jawd` when nothing answers on the folder's `jaw.sock`, in a process group of its own so it isn't stopped with the app, its output in `jawd.log` beside the socket, and draws at once, saying "Starting jawd" until jawd answers. On an account's folder, jaw's own or one holding a `session.db`, only a Jaw built from main or a release's tag on it starts one; on jaw's own folder such a Jaw then asks `SMAppService` to unregister the agent an earlier Jaw registered, without waiting on the answer, and the terminal's agent is named with the command that removes it. Jaw › jawd at Login…, its prompts, the takeover and the bundle's LaunchAgent go. Debug builds keep `--demo` and `JAW_HOME`. `--for-apps` and the hello's `app` follow once jawd has them (the last section).

**Architecture:** JawKit gets two small values and a policy. `Spawn` starts a program as the terminal's `start_jawd` does: nothing on its standard input and output, its standard error appended to a log, none of the app's other descriptors, its signals at their defaults and unblocked, and a process group of its own when asked; it tells the main queue when the program exits, and says its wait status in the terminal's words. `Build` reads where the bundle says it was built from, which `mac/scripts/built-from` works out from git and `make mac` records in the Info.plist. `Starter` decides the rest, as `LayoutKeeper` does for the layouts in the layout book plan: told what the app knows at launch and what happens after (the jawd it started stopping with its log's last words, the wait it asked for ending, jaw-client failing to reach jawd, jawd's hello, `SMAppService`'s answer), it answers with steps (start this jawd, then ask `SMAppService` to unregister the earlier agent, wait a second before starting it again, say this) and with what the window says (waiting, starting, didn't start and why, or out of reach). The app's `Agent` carries the steps out and `AppModel` shows the answer through the reach JawUI already draws, so the app keeps only the wiring: `SMAppService`, the bundle's paths, and `JAW_TRACE`'s lines, which the check on the build Mac reads.

**Tech Stack:** Swift 6 (AppKit, ServiceManagement, Swift Testing), `posix_spawn`, Tuist, xcodebuild on the build Mac through `mac/scripts/build-mac`, POSIX `sh` and git for `mac/scripts/built-from`; on the build Mac, `open`, `plutil`, `codesign`, `defaults` and Python 3 for the check.

## Global Constraints

- The spec is the parity plan's step 1.2 (`docs/plans/2026-10-27-jaw-mac-parity-plan.md`) as the owner chose to build it on 30 October, quoted in the Goal; the "The Mac app" paragraph of the jawd lifetime design, which is on its own branch, its Defaults awaiting the owner's review, its decisions marked "Owner, 29 Oct" his (`git -C /Users/pedrogomes/src/jaw-lifetime show jawd-lifetime:docs/plans/2026-10-29-jaw-jawd-lifetime-design.md`); the terminal's `start_jawd`, `Wire::start` and `unreachable` in `crates/jaw/src/wire.rs`; and the Mac design's "Who starts jawd" and "Starting jawd" state (`docs/plans/2026-10-26-jaw-mac-design.md`). Where this plan and they differ, they win, except where the Decisions say this plan departs from them on purpose: stop and say so. `--for-apps` and the hello's `app` aren't built here: jawd doesn't have them yet.
- The branch is `mac-start-jawd`, in its own worktree, `/Users/pedrogomes/src/jaw-mac-start`. The controller made both from main at 9b3fbdf4 when it committed this plan there, which is c50a2ba3 with 1.1's first half (8e0a63d0, which changes AppModel) and M3's media in, neither touching what this plan changes but AppModel's lines; work only there. Before Task 1, and again before the final report, run `git fetch origin && git log --oneline HEAD..origin/main`; if main has moved, rebase (`git rebase origin/main`) and resolve as the overlaps below say. Check that main has what this plan replaces: `grep -n "func ensure" mac/JawApp/Sources/Agent.swift` and `grep -n "case starting(failed: String?)" mac/JawUI/Sources/Window/Situation.swift` must both find it; if either doesn't, stop and say so.
- Never push, never pass `--no-verify`, never rewrite commits that aren't yours. Never edit, build in, check out in, or run a git command that changes anything in another agent's worktree or in the owner's checkout, `/Users/pedrogomes/src/jaw`; only read them. Read the `mac-panes` plans only from its branch (`git -C /Users/pedrogomes/src/jaw-panes show mac-panes:<path>`), never from its working tree.
- Build and test on the build Mac with `mac/scripts/build-mac`: `test JawKit`, `test JawUITests`, `test JawHarnessTests`, `build` (the `jaw` app), `snapshot`, and, for the check, `build jaw -configuration Release CODE_SIGN_IDENTITY=- ENABLE_HARDENED_RUNTIME=NO`. `mac/scripts/check` exits 1 when a build or the tests fail, so both a run's exit status and its printed `** BUILD SUCCEEDED **` / `** TEST SUCCEEDED **` and `Test run with …` lines count. For the whole log: `ssh -o BatchMode=yes build-mac 'tail -150 ~/src/build-mac/jaw/jaw-mac-start/mac/build/check.log'`. Before each run, check that the build Mac answers: `ssh -o BatchMode=yes -o ConnectTimeout=5 build-mac true`. If it doesn't, or `build-mac` prints `doesn't answer; running here` and falls back to building on this Mac, stop (interrupt the run you started) and ask the controller. Never build or test on this Mac.
- JawKit's live tests, `--demo`, the snapshot and the check need `target/debug/jawd` in this worktree, which `build-mac` copies to the build Mac. Copy the Mac builder's, as it is: `mkdir -p target/debug && cp /Users/pedrogomes/src/jaw-mac-m1/target/debug/jawd target/debug/jawd` (about 50 MB, ignored by git). Never build Rust or Go, check anything out or change anything in that checkout. If its jawd is missing or older than that checkout's HEAD commit, or that HEAD isn't on origin/main, stop and ask the controller, who keeps it current; main moving on past that HEAD is fine. This prints `current` when none of those is so:

  ```sh
  m1=/Users/pedrogomes/src/jaw-mac-m1
  test -x "$m1/target/debug/jawd" && GIT_OPTIONAL_LOCKS=0 git -C "$m1" merge-base --is-ancestor HEAD origin/main \
      && [ "$(stat -f %m "$m1/target/debug/jawd")" -ge "$(git -C "$m1" log -1 --format=%ct HEAD)" ] && echo current
  ```

- `make mac` (`mac/scripts/bundle`) builds jawd, jaw and jaw-engine here with cargo and go, so it isn't run on this branch. Its changes are checked with `sh -n` and by running `mac/scripts/built-from` (Task 2); the check (Task 4) puts together a bundle of its own on the build Mac from the Release app and the copied jawd; and the owner tries what `make mac` makes from origin/main once this has landed (the last section).
- Never run jawd, the Go engine or the app on this Mac. On the build Mac, jawd runs only in JawKit's tests (`--engine demo`), in `--demo`'s snapshot and in the check, whose bundle has no engine anywhere jawd would look for one. The Go engine never runs anywhere. Never stop a process by name; stop only one whose PID you started, or one whose command line names your own scratch folder.
- The owner's folder: nothing here names `~/Library/Application Support/jaw` on this Mac in a command, or opens anything in it. On the build Mac, the check's fourth run uses the build user's jaw folder only when nobody has made it by the moment that run begins, when the check makes it with one `mkdir`, and removes it after.
- Swift 6 language mode, macOS 26, warnings are errors. `mac/scripts/check-components` refuses fonts, radii and line widths set by hand outside `JawUI/Sources/Components`. `mac/scripts/check-harness` refuses a view or a view controller in the app target, and `Jawd` or `Client` named in the harness.
- The code goes in JawKit's `Client/Spawn.swift`, `Client/Build.swift` and `Client/Starter.swift`; the app's `Agent.swift` (rewritten), `AppModel` (`traced`, `traceShown()` and `sinceStart`, `start()`'s first lines and its end, `drain()`'s trace line and its end, and `apply(_:)`'s `.hello` and `.unreachable` cases), `AppDelegate` (`jawdAtLogin(_:)` goes) and `Commands.swift` (the `agent` command goes); `mac/scripts/bundle`, the new `mac/scripts/built-from`, the new `mac/scripts/test-built-from`, which tries it, and the Makefile's `mac-test`, which runs that first; and `mac/JawApp/Helpers/dev.badcoffee.jaw.jawd.plist`, which is deleted. JawUI, the harness, `Launch.swift`, `main.swift`, `Bench.swift` (only called: `Bench.nextFrame`) and `MainWindowController` don't change. Line numbers are main's at c50a2ba3; once a branch below has landed, find each place by the symbol named.
- **Overlaps.** `mac-panes` (1.1) changes AppModel's layout code, `Commands.swift`'s layout commands, `Bench.press` and `panesShown` (Part A), and AppModel's `start()` (a line before `jawd.start()`), `stop()` (rewritten, still ending with `agent.stop()`), `apply(_:)`'s `.hello` case (a line at its end) and its last cases, `Launch.swift` (`Launch.refusal`, `--layouts-in`), `main.swift`, `JawUI/Sources/Window/Prompts.swift` and MainWindowController's Layouts extension (Part B). `mac-folders` adds cases to `AppModel.apply` and JawKit's `JawUpdate` and a line in `start()` after `jawd.watchChats`, which doesn't conflict with this plan's lines at the end of `start()`, two unchanged lines below it, and changes `mac/README.md`'s line 24. `mac-perf` (1.3) adds two properties right after AppModel's `trace`, a line at the top of `drain()` and one after its `window?.refresh()`, and the bench's hooks to `open`, `makePane` and `Bench.swift`; it adds a line after the README's line 17 and changes its line 24. This plan changes `start()`'s first lines and its end, adds a line at the top of `.hello`, replaces the body of `.unreachable`, adds `traced` right after `trace` and `traceShown()` after `drain()`'s `window?.refresh()`, the very places mac-perf adds its own, puts the time in `drain()`'s trace line, between mac-perf's two lines there but beside neither, and changes the README's lines 16, 22 and 24; it adds no case to either enum, and leaves `stop()`, `Bench.swift`, `Launch.swift` and `main.swift` alone. 1.2 will likely land first. Whichever lands second resolves its conflicts by keeping both sides: Part B's keeper beside this plan's starter in `start()` and `.hello`, `stop()` still calling `agent.stop()`, mac-perf's properties and its lines in `drain()` beside `traced` and `traceShown()`, both branches' README lines, and the other branches' cases beside these; and once Part B and this plan are both in, Part B's `Launch.refusal` knows jaw's own folder by `Starter.same`, whose test is the stronger. After that, the second branch's tests and `make check-parity` must pass again. If Part B has landed, `--snapshot` is refused on jaw's own folder and on a folder holding a `session.db`; the check uses `--snapshot` only on a folder of its own with neither.
- No audit: nothing here is on jaw's data paths (`scripts/check-data-audit.py` names none under `mac/`). The app never opens jaw.db or session.db, only looks whether a `session.db` is there; it starts jawd as the terminal does. The one jawd it can stop besides the demo's is an earlier Jaw's agent's, which `SMAppService` stops when it unregisters it, as launchd stops any agent's: with SIGTERM, jawd's clean stop, as `jaw agent uninstall` stops one.
- Scratch goes in four places: the worktree's ignored `mac/build/start/` (the check's script and the demo's picture); the build Mac's copy of the checkout; on the build Mac, the check's one `mktemp` folder under its `TMPDIR`, which holds the app's preferences as the check found them; and, while the fourth run uses it, the build user's jaw folder, which the check makes where jaw keeps it, `~/Library/Application Support/jaw`, not in its `mktemp` folder. The script puts the preferences back and removes those two folders, a dropped ssh session included. The tests' folders (`/tmp/jaws-…`) are their own, and the tests remove them. Before the final report, run `rm -rf mac/build/start` and `make prune-builds`. Once the branch has landed and its worktree is removed, its copy on the build Mac goes too: `ssh -o BatchMode=yes build-mac 'rm -rf ~/src/build-mac/jaw/jaw-mac-start'`.
- Every commit passes `make check-parity`: step 1.2 has no rows in `mac/parity.toml`, and nothing here changes what the Mac sends or reads.
- Commit messages are one long sentence in the project's voice (read `git log -5`), with no prefixes. Code matches its surroundings: doc comments in plain sentences, British spelling, no comments that narrate a change, 4-space indentation, and trailing commas in multi-line argument lists.

## Decisions

- **Only a Jaw built from main or a release's tag starts jawd on jaw's own folder, or on another account's.** Today only the owner's click registers a jawd, so a release made from a branch never runs its jawd on his folder by itself. After this step, one opened while his jawd is down would, and a branch's jawd can move jaw.db to a version main's refuses, as the comment in `Launch.parse`'s `#if DEBUG` says. Agents make release bundles on branches with `make mac`, some in worktrees on his Mac, and nothing would stop him or an agent opening one. So `make mac` records where the bundle was built from, as `mac/scripts/built-from` works it out from git, in two keys of its Info.plist, signed with the rest: `JawBuiltFrom` is `main` when HEAD is on origin/main and origin is the project's own repository, github.com/pdcgomes/jaw, `release` when HEAD is also at a tag like v0.1.0 (`scripts/release.sh`'s), `branch` otherwise, and `changed` when the tree has uncommitted changes to what jawd and jaw-engine are built from (`Cargo.toml`, `Cargo.lock`, `crates`, `engine`, `proto` and `protocol`, with Cargo's, rustup's and go's settings beside them, the makefiles, `scripts/version` and `VERSION`), counting every untracked file there but a `.DS_Store`, whatever git's settings or ignore files say, and every file git is told to assume unchanged or skip; `JawBuiltAt` says which commit, tag or branch, and, of a branch, that it's not on origin/main. `make mac` takes the record before it builds and again after, and writes nothing when git can't read the tree or the two differ, as they do when the checkout changed meanwhile, a change made and undone included (Task 2's amendment says how). A tag counts only on origin/main: every worktree shares the repository's tags, and `scripts/release.sh` has you tag whatever HEAD is, so a branch's commit tagged v0.1.0 would otherwise pass for a release. A change to the app's own sources, the docs or a README doesn't count, but the commit must be on origin/main. The owner's checkout wasn't, on 30 October: its local main was at cc471e2, a commit of its own 91 behind c50a2ba3 and further behind origin/main since, so a Jaw he made there recorded `branch`, said "this Jaw was built from main at cc471e2, not on origin/main" and started no jawd on his folder, while one made from origin/main did (the next decision). While his own commit is in it, a Jaw made there still records `branch`. A tree outside git records `branch` too. A release starts a jawd on an account's folder only from `main` or `release`. Any other, including a bundle that records nothing, as one Xcode or `build-mac release` makes, says so in the panes' place ("It didn't start: this Jaw was built from mac-start-jawd at 1a2b3c4, not on origin/main, so it doesn't start jawd on jaw's own folder or another account's … start one with jaw in a terminal, or open a Jaw built from origin/main") and connects to whatever jawd answers there, as now. It holds with nobody remembering anything: the record is made by the script that makes the bundle, and read by the app that starts jawd.
  - "Jaw's own folder" is `Launch.platformHome` however its path is written: symlinks, case, `..`, a symlink followed by `..`, a trailing slash, or a firmlink, by the file the kernel finds at the path as jawd will be given it, its volume and file number.
  - "Another account's" is any folder holding an engine's `session.db`, as Part B's snapshots and `jawd demo` already take it. On any other folder, as `--home` and `JAW_HOME` give, any release starts its own. That's how the check runs, and how a trial on a fresh folder does until it's linked.
  - The jawd a release starts is only ever its own `Contents/Helpers/jawd`, never `JAWD`'s, so the record speaks for the jawd that runs.
  - A Jaw that may not start jawd there doesn't unregister an agent there either, and a Jaw opened on another folder unregisters none. Both narrow the lifetime design's "the app unregisters any agent an earlier build registered", on purpose: unregistering stops the jawd the agent ran, which ran on jaw's own folder (its plist sets no `JAW_HOME`), and only a Jaw that starts its own there takes its place. A debug build, or a release from a branch, would leave the owner's folder with nothing answering and nothing that may start a jawd there, a build in progress stopping his jawd, which the parity plan's working rules never allow ("his folder and processes are never touched"); and a Jaw on another folder would stop his jawd while it serves a folder of its own.
  - Considered: letting a Mac branch whose jawd sources are main's (no change outside `mac/` since its merge base) start main's jawd there too. It would let a Mac step be tried with his jawd down, but its app is still a build in progress, which the parity plan keeps off his folder, and with his jawd running any build connects to it anyway.
- **A Jaw from origin/main, opened while the owner's jawd is down, starts its jawd on his folder.** While his jawd runs, a Jaw from origin/main connects to it and starts nothing, as now. But his jawd is a debug build of his checkout, which can be behind origin/main: on 30 October its migrations stopped at 17, and origin/main's added 18 and 20, schema steps, and 19, a data step (20 landed that evening, after c50a2ba3). So then, after a reboot, before he had run jaw, a Jaw from origin/main would start origin/main's jawd on his folder, which would upgrade jaw.db to 20, backing it up first beside it as `jaw.db.before-v17`, kept 30 days, and go on running after the app quit; his own jawd would then refuse jaw.db ("jaw.db is from a newer jawd (schema 20, this jawd knows 17)") until it was built from a checkout with origin/main's migrations. Nothing is lost, but before he opens a Jaw from origin/main with his jawd down, he brings his checkout up to origin/main (`git pull`) and rebuilds his jawd, or starts his own jawd first. A Jaw he makes in his checkout still records `branch` after the pull, while his own commit is in it, so the Jaws that start jawd on his folder are made in a checkout or worktree at origin/main. Nothing here stops it: an app starting the newest jawd is what he chose, and a jawd older than jaw.db is the latest jawd design's to handle (Owner, 27 Oct), which never replaces a running jawd with one that would refuse jaw.db. The last section and the final report say this to him, and the Mac plan's rules, as Task 6 rewrites them, say the same for agents: none opens a release Jaw on his Mac.
- **`--demo` never runs on an account's folder.** `--demo --home ~/Library/Application\ Support/jaw`, or `--home` on any folder holding a `session.db`, would put the demo engine's made-up account into a real jaw.db today; the starter refuses both. That's the one change to `--demo`, made on purpose, though the lifetime design keeps debug builds' `--demo` as it is: otherwise it stays a debug build's default, its jawd the app's child, stopped as the app quits.
- **The check runs on the build Mac's build user, with no engine in its bundle.** The build Mac (macOS 27.0.1) has one user besides the system's: `pedrogomes` (uid 502), the build user, logged in at the console since 13 October with a virtual screen and no auto-login. It has no `~/Library/Application Support/jaw`, no jaw plist in `~/Library/LaunchAgents`, nothing of jaw's in `launchctl print gui/502`, no `PATH`, `JAW_ENGINE` or `JAW_HOME` in the environment launchd gives the apps it opens, no Jaw.app installed, no jaw process running and no `jaw-engine` anywhere a jawd would look. So it's clean for jaw, and a macOS user made for the check isn't needed (question 1).
  - A release opened there would start its jawd, which would start the Go engine beside it, and the linking screen asks for a code as soon as it appears (`LinkingView` sends `link.start` from `onAppear`), so the engine would reach WhatsApp. That's allowed on a fresh folder, but the check doesn't need it. The Mac builder's engine is also from 27 October, older than the ten commits to main's engine and its protocol since, and this branch builds no Rust or Go.
  - So the check's bundle is the Release app with the Mac builder's current jawd inside it and no engine. That jawd serves the window unlinked from a fresh jaw.db and reaches nothing.
  - The Release app is built without the hardened runtime (`ENABLE_HARDENED_RUNTIME=NO`), as `mac-perf`'s `build-mac perf` builds the one it measures: signed ad hoc under the hardened runtime, library validation refuses the app its own frameworks. The build Mac's crash report of 29 October for such a Jaw says so ("Library not loaded: @rpath/JawKit.framework … mapping process and mapped file (non-platform) have different Team IDs"), and main's Release build there is signed `adhoc,runtime`. The hardened runtime and the Developer ID's signature come with `make mac`, in the owner's trial.
  - It opens the app as the Finder does, through LaunchServices in the build user's session (`open -n -g`), so the app's process group, `SMAppService` and the window server are a real launch's, and each run claims that the app's parent is launchd, as an app LaunchServices starts has. It finds the app and the jawds the app starts by their real paths, under `/private/var`, as LaunchServices runs them.
  - It shows the window drawn from the app's own trace: a line each time what the window says of jawd changes, or the window's visibility does, once a frame with it is on screen, and the time on each update the app drains, which, since jaw-client's failed tries are drained at the window's display frames, shows the window drawing while what it says stays the same.
  - It shares the build user with other agents' debug builds and harness runs. So it makes the jaw folder for its fourth run with one `mkdir` as that run begins, using the folder only if nobody else has made it by then, and it puts the app's preferences (`dev.badcoffee.jaw`) back as it found them, exported before its runs and imported after, a dropped session included.
  - What it leaves untried: an earlier Jaw's agent actually registered and then unregistered, which no Mac we know has (the starter's tests hold the decision, and the fourth run has `SMAppService` answer the call); the hardened runtime and the Developer ID's signature, the engine under a jawd the app started, and a bundle that `make mac` made, which come with the owner's trial of origin/main's once this has landed; and a user who never ran Jaw at all (the build user has run debug builds and the harness, whose preferences the check puts back and which don't bear on starting jawd).
- **A jawd that stopped because another held the folder is started again.** This departs from the terminal on purpose, where parity rule 1 would make its behaviour the reference: the terminal says "jawd didn't start: another jawd is running on …" until the other's jawd answers. Here the app starts its jawd while an earlier Jaw's agent's jawd may still be stopping, holding `jaw.lock` for the moment its stop takes, and two apps can start jawd at once. So the starter starts its jawd again a second after it stopped, while nothing answers and within the terminal's 15 s, and neither case ends saying it. The app waits that second on a timer of its own (`DispatchQueue.main.asyncAfter`), not for jaw-client's next try: the client's failed tries are drained only at the window's display frames, so with the window closed in that second, a restart waiting on one would wait until the window opened again. It knows the case by jawd's own words, which a test reads from `crates/jawd/src/main.rs`, so a change there fails it. Task 6 writes it into the Mac design.
- **The earlier agent, unregistered after the start and never waited on.** `SMAppService`'s header says `agent(plistName:)` names a plist in the app's `Contents/Library/LaunchAgents` ("The plistName must correspond to a plist in the calling app's Contents/Library/LaunchAgents directory"), which this step removes, so whether `status` and `unregister` still find an earlier build's registration isn't documented. On jaw's own folder, a Jaw that starts jawd there calls `unregister(completionHandler:)` at every launch, whatever `status` says, an error meaning there was nothing it could find, and calls it just after starting its jawd. So a launch with nothing answering never waits on `SMAppService`, which answers through the background-task daemon, and the window says "Starting jawd" from its first frame. The trace says when it asked and how many milliseconds the answer took.
  - If an earlier agent's jawd is still stopping as the app's starts, `jaw.lock` refuses the app's, and the restart above starts it again.
  - If an earlier agent's jawd was the one answering at launch, or was still starting then, refusing the app's by `jaw.lock`, and answered by the end of the restart's wait or held the lock past the app's 15 s, unregistering stops it (the header has the answer come "after the running process has been killed"), and the app starts its own once `SMAppService` says it removed the agent, unless another jawd answers by then; until then the window says what jaw-client says, as it does whenever a jawd goes, or why the app's jawd didn't start. Only a jawd of the app's on its way, running or to be started again, keeps that start from coming; one that failed isn't on its way, so one that couldn't be started at all is tried once more then. That's the one wait on `SMAppService`, and it has no cap, so the jawd the app starts then has its 15 s from that start, not from an earlier one's, which is over, and the window says Starting jawd for it, whatever an earlier one's failure said. It comes only where an earlier Jaw registered an agent, which no Mac we know has; nothing waits on it to draw; and starting a jawd before `SMAppService` has said the agent has gone would start one whenever a jawd that had answered stops, which the terminal never does. A Jaw whose answer never comes starts its jawd at once at its next launch.
  - The completion handler is marked `@Sendable`. The header has it called "on libdispatch's default target queue" and declares it without `NS_SWIFT_SENDABLE`; a compiler that took a closure written in the main-actor `Agent` for the main actor's would check that it runs there and stop the app the moment `SMAppService` answered. Xcode 26.6's Swift 6.3.3, on both Macs, already imports the handler as `@Sendable`, and makes the closure nonisolated with no such check, mark or not; the mark says so in the code and keeps it so for any compiler. The handler hops to the main queue itself.
  - Question 2 asks whether to keep the plist one more build instead.
- **The terminal's agent is named in the status line, and left alone.** For its few seconds, at each launch on jaw's own folder while `~/Library/LaunchAgents/dev.badcoffee.jaw.jawd.plist` is there: "jaw agent install's LaunchAgent still starts jawd at login: jaw agent uninstall removes it", with ", and Jaw starts jawd as it opens" from a Jaw that does. While it's there, the app asks `SMAppService` nothing, which narrows the lifetime design's "the app unregisters any agent an earlier build registered" on purpose: the label is that agent's as well, and no Jaw ever registered beside it (`registerAgent` refused while the plist was there, the takeover removed it first, and `jaw agent install` refuses while the app's agent is loaded), so nothing has shown what unregistering under that label would do to it, and no earlier Jaw's agent can be loaded beside it; the first launch after it has gone asks. The app still starts its own jawd when nothing answers, as the terminal does beside one.

## Questions for the owner

1. **A macOS user made for the check?** Lean: no, which the review shares. The build user is clean for jaw, as above, and the check leaves it so. What it shares with other agents' runs there it guards: it makes the jaw folder for its fourth run with one `mkdir` as that run begins, so it never uses one another run made in the minutes before, and it exports the app's preferences (`defaults export dev.badcoffee.jaw`) before its runs and imports them after, so the debug builds and the harness run there find them as they were. The one thing it can't guard is a write another run makes to them while the check runs, which putting them back undoes. A user of its own might not work at all: one behind fast user switching isn't on the display, so its window's display link, which drives the drains the check reads as drawing, may not tick. If you'd rather it ran on a user that has never run Jaw, on the build Mac run `sudo sysadminctl -addUser jawcheck -fullName "Jaw Check" -password -` (it asks for the password), log in as it once through Screen Sharing and leave it logged in with fast user switching, then run `sudo mkdir -p /Users/jawcheck/.ssh && sudo cp ~/.ssh/authorized_keys /Users/jawcheck/.ssh/ && sudo chown -R jawcheck:staff /Users/jawcheck/.ssh && sudo chmod 700 /Users/jawcheck/.ssh && sudo chmod 600 /Users/jawcheck/.ssh/authorized_keys`, and on this Mac add a `Host build-mac-check` entry beside `build-mac`'s in `~/.ssh/config`, with `User jawcheck`. The check would then copy its Release app and jawd there with `scp` rather than read them from the build user's copy, a change to its first lines.
2. **Keep the LaunchAgent's plist in the bundle one more build?** Lean: no, as you decided, which the review shares. Unregistering an earlier build's agent may need the plist in the bundle, which `SMAppService`'s header implies and doesn't say. But no Mac we know has such an agent: this Mac's `gui/501` has no `dev.badcoffee.jaw.jawd` service and no jaw plist in `~/Library/LaunchAgents`, and the build user has neither. Without the plist the call is outside the header's contract, so it most likely returns an error everywhere, which costs nothing now that the app starts jawd before asking and neither waits on the answer nor crashes on it. The check can't tell either way: with nothing registered, `SMAppService`'s error says nothing of whether it would find a registration with the plist gone. And an earlier agent whose plist has gone from the bundle can no longer be loaded at the next login. If you'd rather be sure, the plist stays in `Contents/Library/LaunchAgents`, registered by nothing, until the follow-up below removes it.

## File structure

New:

- `mac/JawKit/Sources/Client/Spawn.swift`: a program started as the terminal starts jawd, in a process group of its own when asked, its exit told; a wait status and a log's last words, said.
- `mac/JawKit/Sources/Client/Build.swift`: where the bundle says it was built from, and whether its jawd may start on an account's folder.
- `mac/JawKit/Sources/Client/Starter.swift`: who starts jawd and what the window says meanwhile; whether two paths are one folder.
- `mac/scripts/built-from`: what a checkout's Jaw is built from, as `make mac` records it.
- `mac/scripts/test-built-from`: `built-from` tried on throwaway repositories, one for each case it tells apart, which `make mac-test` runs first.
- `mac/Tests/JawKitTests/SpawnTests.swift`, `mac/Tests/JawKitTests/BuildTests.swift`, `mac/Tests/JawKitTests/StarterTests.swift`.

Changed:

- `mac/JawApp/Sources/Agent.swift`: rewritten. It carries out the starter's steps, and makes `Situation.Reach` from what the starter says.
- `mac/JawApp/Sources/AppModel.swift`: `traced` with `traceShown()` and `sinceStart`, `start()`, `drain()`, and `apply(_:)`'s `.hello` and `.unreachable`.
- `mac/JawApp/Sources/AppDelegate.swift`: `jawdAtLogin(_:)` goes.
- `mac/JawApp/Sources/Commands.swift`: the `agent` command goes.
- `mac/scripts/bundle`: the record (Task 2); no LaunchAgent and no `JawdVersion` (Task 5).
- `Makefile`: `mac-test` runs `mac/scripts/test-built-from` first (Task 2).
- `mac/README.md`, `docs/plans/2026-10-27-jaw-mac-parity-plan.md`, `docs/plans/2026-10-26-jaw-mac-design.md`, `docs/plans/2026-10-26-jaw-mac-plan.md` (Task 6).

Deleted: `mac/JawApp/Helpers/dev.badcoffee.jaw.jawd.plist`.

Scratch, never committed: `mac/build/start/try-start.sh`, the check (Task 4); `mac/build/start/demo.png`, the demo's picture (Task 5).

---

### Task 1: A program started in a process group of its own, in JawKit

**Files:**
- Create: `mac/JawKit/Sources/Client/Spawn.swift`
- Test: `mac/Tests/JawKitTests/SpawnTests.swift`

**Interfaces:**
- Produces, in JawKit:
  - `public struct Spawn: Equatable, Sendable`, with `program: String`, `arguments: [String]`, `environment: [String: String]`, `log: String`, `ownGroup: Bool` and `public init(program:arguments:environment:log:ownGroup:)`.
  - `public func run(exited: @escaping @MainActor @Sendable (pid_t, Int32) -> Void) throws -> pid_t`: starts it and returns its PID; `exited` gets the PID and its wait status on the main queue once it has gone. It throws when the log's folder can't be made or `posix_spawn` fails.
  - `public static func said(_ status: Int32) -> String`: the wait status in the terminal's words, which are Rust's `ExitStatus`'s: `"exit status: 1"`, or `"signal: 9 (SIGKILL)"` for one a signal stopped, with `" (core dumped)"` when it left a core.
  - `public static func lastWords(_ log: String) -> String`: the log's last line that says something, without jawd's `"jawd: "`, read from its last 4 KB; `""` with no log.

- [ ] **Step 1: Write the failing tests**

Create `mac/Tests/JawKitTests/SpawnTests.swift`. It uses `eventually`, from `FakeJawd.swift`:

```swift
import Darwin
import Foundation
@testable import JawKit
import Testing

/// The exits the main queue was told of, by PID.
@MainActor
final class Exits {
    var seen: [pid_t: Int32] = [:]
}

/// A folder of the test's own, which it removes.
func spawnFolder() -> String {
    "/tmp/jaws-\(UInt32.random(in: 0 ... .max))"
}

@MainActor @Test func `a spawn in a group of its own leads it, appends to its log in a folder made for it, and its exit is told`() async throws {
    let folder = spawnFolder()
    defer { try? FileManager.default.removeItem(atPath: folder) }
    let log = folder + "/home/jawd.log"
    let exits = Exits()
    func sh(_ script: String, ownGroup: Bool) -> Spawn {
        Spawn(program: "/bin/sh", arguments: ["-c", script], environment: ["PATH": "/usr/bin:/bin"], log: log, ownGroup: ownGroup)
    }
    let first = try sh("echo before >&2; exit 3", ownGroup: true).run { pid, status in exits.seen[pid] = status }
    #expect(await eventually { exits.seen[first] != nil })
    #expect(exits.seen[first].map(Spawn.said) == "exit status: 3")
    let mode = try FileManager.default.attributesOfItem(atPath: folder + "/home")[.posixPermissions] as? Int
    #expect(mode == 0o700, "the log's folder is made, readable only by you")
    let leader = try sh("echo after >&2; exec sleep 30", ownGroup: true).run { pid, status in exits.seen[pid] = status }
    #expect(await eventually { (try? String(contentsOfFile: log, encoding: .utf8)) == "before\nafter\n" }, "each start appends to the log")
    #expect(getpgid(leader) == leader && getpgid(leader) != getpgrp(), "it leads a process group of its own")
    kill(leader, SIGTERM)
    #expect(await eventually { exits.seen[leader] != nil })
    #expect(exits.seen[leader].map(Spawn.said) == "signal: 15 (SIGTERM)")
    let child = try sh("exec sleep 30", ownGroup: false).run { pid, status in exits.seen[pid] = status }
    #expect(getpgid(child) == getpgrp(), "without one, it's in the app's group")
    kill(child, SIGKILL)
    #expect(await eventually { exits.seen[child] != nil })
    #expect(exits.seen[child].map(Spawn.said) == "signal: 9 (SIGKILL)")
}

@MainActor @Test func `a spawn has none of the app's descriptors but the three it's given`() async throws {
    let folder = spawnFolder()
    defer { try? FileManager.default.removeItem(atPath: folder) }
    let log = folder + "/jawd.log"
    let held = Darwin.open("/dev/null", O_RDONLY)
    let high = fcntl(held, F_DUPFD, 40)
    defer {
        close(high)
        close(held)
    }
    #expect(high >= 40, "a descriptor of the app's numbered above the three, as another test's socket can be")
    let exits = Exits()
    let pid = try Spawn(program: "/bin/sh", arguments: ["-c", "ls /dev/fd >&2"], environment: ["PATH": "/usr/bin:/bin"], log: log,
                        ownGroup: true).run { pid, status in exits.seen[pid] = status }
    #expect(await eventually { exits.seen[pid] != nil })
    #expect(exits.seen[pid] == 0)
    let listed = try String(contentsOfFile: log, encoding: .utf8).split(separator: "\n").map(String.init)
    #expect(listed.contains("2") && !listed.contains(String(high)), "the descriptors it had: \(listed)")
}

@MainActor @Test func `a spawn's signals are at their defaults and unblocked, whatever the app ignores or blocks`() async throws {
    let folder = spawnFolder()
    defer { try? FileManager.default.removeItem(atPath: folder) }
    var blocked = sigset_t()
    sigemptyset(&blocked)
    sigaddset(&blocked, SIGUSR2)
    signal(SIGUSR1, SIG_IGN)
    pthread_sigmask(SIG_BLOCK, &blocked, nil)
    defer {
        pthread_sigmask(SIG_UNBLOCK, &blocked, nil)
        signal(SIGUSR1, SIG_DFL)
    }
    let exits = Exits()
    func sleeper() throws -> pid_t {
        try Spawn(program: "/bin/sleep", arguments: ["30"], environment: [:], log: folder + "/jawd.log", ownGroup: true)
            .run { pid, status in exits.seen[pid] = status }
    }
    let ignored = try sleeper()
    let masked = try sleeper()
    defer {
        for pid in [ignored, masked] where exits.seen[pid] == nil {
            kill(pid, SIGKILL)
        }
    }
    kill(ignored, SIGUSR1)
    kill(masked, SIGUSR2)
    #expect(await eventually { exits.seen[ignored] != nil && exits.seen[masked] != nil })
    #expect(exits.seen[ignored].map(Spawn.said) == "signal: 30 (SIGUSR1)", "a signal the app ignores is at its default")
    #expect(exits.seen[masked].map(Spawn.said) == "signal: 31 (SIGUSR2)", "a signal the app blocks is unblocked")
}

@Test func `a log's last words are its last line that says something, without jawd's name, from its last 4 KB, and a wait status is said in the terminal's words`() throws {
    let folder = spawnFolder()
    try FileManager.default.createDirectory(atPath: folder, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: folder) }
    let log = folder + "/jawd.log"
    #expect(Spawn.lastWords(log) == "", "no log, no words")
    try "jawd: stopped in 40 ms\njawd: another jawd is running on /x\n\n  \n".write(toFile: log, atomically: true, encoding: .utf8)
    #expect(Spawn.lastWords(log) == "another jawd is running on /x")
    try (String(repeating: "x", count: 10000) + "\nthe engine said nothing\n").write(toFile: log, atomically: true, encoding: .utf8)
    #expect(Spawn.lastWords(log) == "the engine said nothing")
    try ("x\n" + String(repeating: "y", count: 5000) + "\n").write(toFile: log, atomically: true, encoding: .utf8)
    #expect(Spawn.lastWords(log) == String(repeating: "y", count: 4095), "only the last 4 KB are read, however long the log")
    #expect(Spawn.said(1 << 8) == "exit status: 1" && Spawn.said(9) == "signal: 9 (SIGKILL)")
    #expect(Spawn.said(6 | 0x80) == "signal: 6 (SIGABRT) (core dumped)")
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawKit`
Expected: exit 1, with `error: cannot find 'Spawn' in scope` from `SpawnTests.swift`.

- [ ] **Step 3: Write `Spawn`**

Create `mac/JawKit/Sources/Client/Spawn.swift`:

```swift
import Darwin
import Foundation

/// A program to start as jawd is started, as the terminal's `start_jawd` starts one
/// (`crates/jaw/src/wire.rs`): its arguments and environment, the log its standard error is
/// appended to, and whether it runs in a process group of its own, so it isn't stopped with the app.
public struct Spawn: Equatable, Sendable {
    public var program: String
    public var arguments: [String]
    public var environment: [String: String]
    /// Where its standard error goes, appended; the folder is made if it isn't there, readable only by you.
    public var log: String
    public var ownGroup: Bool

    public init(program: String, arguments: [String], environment: [String: String], log: String, ownGroup: Bool) {
        self.program = program
        self.arguments = arguments
        self.environment = environment
        self.log = log
        self.ownGroup = ownGroup
    }

    /// Starts it with nothing on its standard input and output, its standard error appended to the
    /// log, none of the app's other descriptors, and every signal at its default and unblocked;
    /// `exited` is told its PID and wait status on the main queue once it has gone, however long after.
    public func run(exited: @escaping @MainActor @Sendable (pid_t, Int32) -> Void) throws -> pid_t {
        try FileManager.default.createDirectory(atPath: (log as NSString).deletingLastPathComponent,
                                                withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        var actions: posix_spawn_file_actions_t?
        posix_spawn_file_actions_init(&actions)
        defer { posix_spawn_file_actions_destroy(&actions) }
        posix_spawn_file_actions_addopen(&actions, 0, "/dev/null", O_RDONLY, 0)
        posix_spawn_file_actions_addopen(&actions, 1, "/dev/null", O_WRONLY, 0)
        posix_spawn_file_actions_addopen(&actions, 2, log, O_WRONLY | O_CREAT | O_APPEND, 0o600)
        var attributes: posix_spawnattr_t?
        posix_spawnattr_init(&attributes)
        defer { posix_spawnattr_destroy(&attributes) }
        var flags = POSIX_SPAWN_CLOEXEC_DEFAULT | POSIX_SPAWN_SETSIGDEF | POSIX_SPAWN_SETSIGMASK
        if ownGroup {
            flags |= POSIX_SPAWN_SETPGROUP
            posix_spawnattr_setpgroup(&attributes, 0)
        }
        posix_spawnattr_setflags(&attributes, Int16(flags))
        var unblocked = sigset_t()
        sigemptyset(&unblocked)
        posix_spawnattr_setsigmask(&attributes, &unblocked)
        var defaults = sigset_t()
        sigfillset(&defaults)
        sigdelset(&defaults, SIGKILL)
        sigdelset(&defaults, SIGSTOP)
        posix_spawnattr_setsigdefault(&attributes, &defaults)
        let argv = ([program] + arguments).map { strdup($0) } + [nil]
        let envp = environment.map { strdup("\($0.key)=\($0.value)") } + [nil]
        defer {
            for string in argv + envp {
                free(string)
            }
        }
        var pid: pid_t = 0
        let failed = posix_spawn(&pid, program, &actions, &attributes, argv, envp)
        guard failed == 0 else { throw POSIXError(POSIXErrorCode(rawValue: failed) ?? .EINVAL) }
        let started = pid
        Thread.detachNewThread {
            var status: Int32 = 0
            while waitpid(started, &status, 0) == -1, errno == EINTR {}
            let ended = status
            DispatchQueue.main.async { MainActor.assumeIsolated { exited(started, ended) } }
        }
        return started
    }

    /// A wait status in the terminal's words, which are Rust's `ExitStatus`'s: "exit status: 1", or
    /// "signal: 9 (SIGKILL)" for one a signal stopped, with " (core dumped)" when it left a core.
    public static func said(_ status: Int32) -> String {
        let signal = status & 0x7F
        guard signal != 0 else { return "exit status: \((status >> 8) & 0xFF)" }
        let names = ["HUP", "INT", "QUIT", "ILL", "TRAP", "ABRT", "EMT", "FPE", "KILL", "BUS", "SEGV", "SYS", "PIPE", "ALRM", "TERM", "URG",
                     "STOP", "TSTP", "CONT", "CHLD", "TTIN", "TTOU", "IO", "XCPU", "XFSZ", "VTALRM", "PROF", "WINCH", "INFO", "USR1", "USR2"]
        let name = Int(signal) <= names.count ? " (SIG\(names[Int(signal) - 1]))" : ""
        return "signal: \(signal)\(name)" + ((status & 0x80) != 0 ? " (core dumped)" : "")
    }

    /// A log's last line that says something, without jawd's "jawd: ", read from its last 4 KB: why a
    /// jawd that stopped as it started did, which jawd says on its last line.
    public static func lastWords(_ log: String) -> String {
        guard let file = FileHandle(forReadingAtPath: log) else { return "" }
        defer { try? file.close() }
        let end = (try? file.seekToEnd()) ?? 0
        try? file.seek(toOffset: end > 4096 ? end - 4096 : 0)
        let text = String(decoding: (try? file.readToEnd()) ?? Data(), as: UTF8.self)
        let last = text.split(separator: "\n").reversed().map { $0.trimmingCharacters(in: .whitespaces) }.first { !$0.isEmpty } ?? ""
        return last.hasPrefix("jawd: ") ? String(last.dropFirst(6)) : last
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `mac/scripts/build-mac test JawKit`
Expected: exit 0, `** TEST SUCCEEDED **`, and JawKit passes with four more tests than before.

- [ ] **Step 5: Commit**

```bash
git add mac/JawKit/Sources/Client/Spawn.swift mac/Tests/JawKitTests/SpawnTests.swift
git commit -F- <<'EOF'
JawKit starts a program as the terminal's start_jawd starts jawd, through posix_spawn: nothing on its standard input and output, its standard error appended to a log in a folder made for it if there isn't one, readable only by you, none of the app's other descriptors, every signal at its default and unblocked, and a process group of its own when asked, so a jawd started so isn't stopped with the app; the main queue is told its exit and wait status, said in the terminal's words as exit status: 1 or signal: 9 (SIGKILL), and a log's last words are its last line that says something, without jawd's name, from its last 4 KB; tests: a spawn leads a group of its own or joins the app's, appends to its log and is told of its exit, has none of the app's descriptors, one numbered above 40 included, has its signals at their defaults and unblocked whatever the app ignores or blocks, and a log's last words come from its end
EOF
```

---

### Task 2: Where a Jaw was built from

**Files:**
- Create: `mac/JawKit/Sources/Client/Build.swift`, `mac/scripts/built-from`
- Modify: `mac/scripts/bundle` (its header comment at 7-10, after line 16, after line 43, line 49 and the last line, 62), `Makefile` (`mac-test`)
- Test: `mac/Tests/JawKitTests/BuildTests.swift`, `mac/scripts/test-built-from`

**Interfaces:**
- Produces, in JawKit: `public struct Build: Equatable, Sendable`, with `from: String?` (`"main"`, `"release"`, `"branch"`, `"changed"` or nil), `at: String?`, `public init(from:at:)`, `public init(info: [String: Any])` (reading `JawBuiltFrom` and `JawBuiltAt`), `public var startsOnAccounts: Bool` and `public var refusal: String`.
- Produces: `mac/scripts/built-from [<checkout> [<since>]]`, which prints two lines, `from` then `at`: `main` and `<hash> on main`; `release` and `<tag>`, for a tag on a commit on origin/main; `branch` and `<branch> at <hash>, not on origin/main` (`<hash>` alone when HEAD is detached, here and below), `<branch> at <hash>, in a checkout whose origin isn't github.com/pdcgomes/jaw`, or `a tree outside git, which says nothing of where it came from`; or `changed` and `<branch> at <hash>, with uncommitted changes to what jawd is built from`, `<branch> at <hash>, with files jawd is built from whose changes git is told to ignore`, or, given `<since>`, a file, `<branch> at <hash>, with what jawd is built from written since <since>`. It fails, printing nothing, when git can't read the tree, when `<since>` isn't there, and when the folder's work tree doesn't hold it. It takes none of git's optional locks, so it changes nothing in the checkout it reads.
- Produces: the bundle's Info.plist keys `JawBuiltFrom` and `JawBuiltAt`, which `make mac` writes from what built-from said before the builds, once it says the same after them.
- Produces: `mac/scripts/test-built-from`, which `make mac-test` runs first: built-from tried on throwaway repositories, printing `ok:` or `FAIL:` for each case and exiting 1 if any failed.

- [ ] **Step 1: Write the failing test**

Create `mac/Tests/JawKitTests/BuildTests.swift`:

```swift
import Foundation
@testable import JawKit
import Testing

@Test func `only a Jaw built from main or a release's tag on it starts jawd on an account's folder, and the others say what they were built from`() {
    let main = Build(info: ["JawBuiltFrom": "main", "JawBuiltAt": "c50a2ba3 on main", "CFBundleName": "Jaw"])
    #expect(main == Build(from: "main", at: "c50a2ba3 on main") && main.startsOnAccounts)
    #expect(Build(from: "release", at: "v0.2.0").startsOnAccounts)
    for from in ["branch", "changed", "Main", "", nil] as [String?] {
        #expect(!Build(from: from, at: "mac-start-jawd at 1a2b3c4").startsOnAccounts, "from \(from ?? "nothing")")
    }
    #expect(Build(from: "branch", at: "main at cc471e2, not on origin/main").refusal
        == "this Jaw was built from main at cc471e2, not on origin/main, so it doesn't start jawd on jaw's own folder or another account's, "
        + "where a jawd from anything but origin/main or a release's tag could move jaw.db to a version origin/main's can't open: "
        + "start one with jaw in a terminal, or open a Jaw built from origin/main")
    #expect(Build(info: [:]) == Build(from: nil, at: nil))
    #expect(Build(info: ["JawBuiltFrom": true]).from == nil)
    #expect(Build(info: [:]).refusal.hasPrefix("this Jaw doesn't say what it was built from, as make mac's builds do, so it doesn't start jawd"))
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `mac/scripts/build-mac test JawKit`
Expected: exit 1, with `error: cannot find 'Build' in scope` from `BuildTests.swift`.

- [ ] **Step 3: Write `Build`**

Create `mac/JawKit/Sources/Client/Build.swift`:

```swift
import Foundation

/// Where this Jaw was built from, as `make mac` records it in the bundle's Info.plist (`JawBuiltFrom`
/// and `JawBuiltAt`, from `mac/scripts/built-from`): a commit on origin/main, a release's tag on one,
/// a branch, or a tree with uncommitted changes to what jawd and jaw-engine are built from. Only the
/// first two start a jawd on an account's folder, jaw's own or one holding a `session.db`, where a
/// branch's jawd could move jaw.db to a version main's can't open.
public struct Build: Equatable, Sendable {
    /// `main`, `release`, `branch` or `changed`; nil when the bundle doesn't say, as one `make mac`
    /// didn't make doesn't.
    public var from: String?
    /// The commit, tag or branch, for what the app says: "c50a2ba3 on main", "v0.2.0" or
    /// "mac-start-jawd at 1a2b3c4, not on origin/main".
    public var at: String?

    public init(from: String?, at: String?) {
        self.from = from
        self.at = at
    }

    /// The bundle's, from its Info.plist's keys.
    public init(info: [String: Any]) {
        self.init(from: info["JawBuiltFrom"] as? String, at: info["JawBuiltAt"] as? String)
    }

    /// Whether the jawd inside it may start on an account's folder.
    public var startsOnAccounts: Bool {
        from == "main" || from == "release"
    }

    /// Why the jawd inside it doesn't start on an account's folder, and how to start one there.
    public var refusal: String {
        let built = at.map { "this Jaw was built from \($0)" } ?? "this Jaw doesn't say what it was built from, as make mac's builds do"
        return "\(built), so it doesn't start jawd on jaw's own folder or another account's, where a jawd from anything but origin/main or a "
            + "release's tag could move jaw.db to a version origin/main's can't open: start one with jaw in a terminal, or open a Jaw built from origin/main"
    }
}
```

- [ ] **Step 4: Run the test**

Run: `mac/scripts/build-mac test JawKit`
Expected: exit 0, `** TEST SUCCEEDED **`, with one more test than after Task 1.

- [ ] **Step 5: Write `built-from`**

Create `mac/scripts/built-from`, then run `chmod +x mac/scripts/built-from`:

```sh
#!/bin/sh
# What a Jaw built from a checkout is built from, as `make mac` records it in the bundle's Info.plist
# (JawBuiltFrom and JawBuiltAt): on two lines, main, release, branch or changed, then the commit, tag
# or branch, for what the app says. Only a Jaw from origin/main or a release's tag on it starts a jawd
# on an account's folder, since a branch's jawd could move jaw.db to a version main's can't open. A
# tag counts only on origin/main, since every worktree shares the tags, and origin/main only where
# origin is the project's own repository, as Cargo.toml names it, since a clone of a local checkout
# has that checkout's main for it; "changed" is a tree with uncommitted changes to what jawd and
# jaw-engine are built from, Cargo's, rustup's and go's settings beside them, the makefiles and what
# gives the version included, which the app's own sources, the docs and the READMEs aren't; and a
# tree outside git says branch. It reads origin/main by its whole ref, which a branch or tag called
# origin/main can't stand in for; counts as a change every untracked file but a .DS_Store, whatever
# an ignore file or status.showUntrackedFiles says, since Cargo runs a new build.rs and go compiles a
# new .go file however git is told to ignore it, and every file git is told to assume unchanged or
# skip; has git look at every tracked file itself, whatever core.fsmonitor says; reads the checkout
# that holds the folder it's given, from its top, whatever GIT_DIR and GIT_WORK_TREE say, and
# refuses a folder whose work tree, as core.worktree can set it, doesn't hold it; takes no replace
# refs or grafts, which make one commit stand for another; and stops when git can't read the tree,
# so make mac records nothing. Given a file as well, it says changed when anything jawd is built
# from was written since that file was, so make mac sees a change made and undone while it built.
# What it can't see: a change a filter of git's hides, a file made at the checkout's top and removed
# again since the file given, a change undone with its files' times put back, and settings outside
# the checkout, Cargo's and go's own and the environment's. It only reads: git takes none of its
# optional locks.
#
#   mac/scripts/built-from [<checkout> [<since>]]   # this script's own checkout by default
set -eu
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES \
    GIT_NAMESPACE GIT_CONFIG GIT_CONFIG_GLOBAL GIT_CONFIG_SYSTEM GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT
# /var/empty is root's and always empty, so git finds no grafts there.
export GIT_OPTIONAL_LOCKS=0 GIT_NO_REPLACE_OBJECTS=1 GIT_GRAFT_FILE=/var/empty/grafts
host=github.com
name=pdcgomes/jaw
inputs="Cargo.toml Cargo.lock crates engine proto protocol .cargo rust-toolchain rust-toolchain.toml go.work"
inputs="$inputs Makefile GNUmakefile makefile scripts/version VERSION"
since=${2:-}
cd "${1:-$(dirname "$0")/../..}"
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    printf 'branch\na tree outside git, which says nothing of where it came from\n'
    exit 0
fi
here=$(pwd -P)
[ "$(git rev-parse --is-inside-work-tree)" = true ] || { echo "built-from: $here isn't inside its work tree" >&2; exit 1; }
top=$(git rev-parse --show-toplevel)
cd "$top"
commit=$(git rev-parse --short HEAD)
branch=$(git rev-parse --abbrev-ref HEAD)
place="$branch at $commit"
[ "$branch" = HEAD ] && place=$commit
changes=$(git -c core.fsmonitor=false status --porcelain --untracked-files=no --ignore-submodules=none -- $inputs)
others=$(git -c core.fsmonitor=false ls-files --others -- $inputs)
untracked=$(printf '%s\n' "$others" | sed '/\/\.DS_Store$/d')
tracked=$(git -c core.fsmonitor=false ls-files -v -- $inputs)
unwatched=$(printf '%s\n' "$tracked" | sed -n '/^[^H]/p')
written=
if [ -n "$since" ]; then
    for input in $inputs; do
        if [ -e "$input" ] || [ -L "$input" ]; then
            found=$(find "$input" ! -name .DS_Store -newer "$since")
            written="$written$found"
        fi
    done
fi
origin=$(git remote get-url origin 2> /dev/null) || origin=
case ${origin%.git} in
"https://$host/$name" | "git@$host:$name" | "ssh://git@$host/$name") ours=true ;;
*) ours=false ;;
esac
main=$(git show-ref --verify --hash refs/remotes/origin/main 2> /dev/null) || main=
if [ -n "$changes$untracked" ]; then
    printf 'changed\n%s, with uncommitted changes to what jawd is built from\n' "$place"
elif [ -n "$unwatched" ]; then
    printf 'changed\n%s, with files jawd is built from whose changes git is told to ignore\n' "$place"
elif [ -n "$written" ]; then
    printf 'changed\n%s, with what jawd is built from written since %s\n' "$place" "$since"
elif ! $ours; then
    printf "branch\n%s, in a checkout whose origin isn't %s\n" "$place" "$host/$name"
elif [ -z "$main" ] || ! git merge-base --is-ancestor HEAD "$main" 2> /dev/null; then
    printf 'branch\n%s, not on origin/main\n' "$place"
elif tag=$(git describe --tags --exact-match --match 'v[0-9]*' HEAD 2> /dev/null); then
    printf 'release\n%s\n' "$tag"
else
    printf 'main\n%s on main\n' "$commit"
fi
```

- [ ] **Step 6: Try it**

Run each, from the worktree:

```sh
mac/scripts/built-from
touch crates/built-from-check && mac/scripts/built-from; rm crates/built-from-check
touch mac/built-from-check && mac/scripts/built-from; rm mac/built-from-check
mac/scripts/built-from /Users/pedrogomes/src/jaw-mac-m1
mac/scripts/built-from /var/empty
git status --short
```

Expected, with each checkout's own short hash:

```
branch
mac-start-jawd at <this HEAD>, not on origin/main
changed
mac-start-jawd at <this HEAD>, with uncommitted changes to what jawd is built from
branch
mac-start-jawd at <this HEAD>, not on origin/main
main
<the Mac builder's HEAD> on main
branch
a tree outside git, which says nothing of where it came from
```

followed by `git status --short` listing only this task's three new files. A new file under `crates` makes the tree changed, and one under `mac` doesn't. The Mac builder's checkout records main, read without a lock. A folder outside git records branch. The lines that tried a throwaway repository here are gone: their cases, a local main with a commit of its own, as the owner's checkout has, recording branch even at a release's tag, and the same tag on origin/main recording release, are `mac/scripts/test-built-from`'s (the amendment below), on repositories of its own that none of its git commands can climb out of, where those lines, not chained, would have committed on this worktree's branch, and tagged and moved origin/main in its repository, every worktree's, had `git init` failed. If the Mac builder's checkout says `changed`, it has uncommitted changes of the Mac builder's: say so in the report and go on, since the other lines show each case.

- [ ] **Step 7: The bundle records it**

In `mac/scripts/bundle`, make five changes.

The header comment's lines 7 to 10 become the lines below, which also say what an ad hoc bundle does: main's says it "runs here only", but under the hardened runtime it doesn't open, as the build Mac's crash report of 29 October shows for a Release Jaw signed ad hoc (the Decisions).

```sh
# Release on the build Mac (here when it doesn't answer) and comes back to mac/build/release/Jaw.app. The three
# helpers go into Contents/Helpers, jawd's LaunchAgent into Contents/Library/LaunchAgents, and what
# mac/scripts/built-from says into its Info.plist (JawBuiltFrom and JawBuiltAt), since only a Jaw from
# origin/main or a release's tag on it starts its jawd on an account's folder; everything is signed
# from the inside out with the hardened runtime: with JAW_SIGN_IDENTITY, else the team's Developer ID
# Application identity by its SHA-1, else ad hoc, which doesn't open: under the hardened runtime,
# library validation refuses an app signed ad hoc its own frameworks.
```

After line 16, `app="$out/Jaw.app"`, before the builds, add:

```sh
# Made before the first reading, so the second sees anything jawd is built from written since, a
# change made and undone while it builds included.
stamp="$(mktemp "${TMPDIR:-/tmp}/jaw-bundle.XXXXXX")"
trap 'rm -f "$stamp"' EXIT
built="$(mac/scripts/built-from)"
# The helpers are copied from this checkout's target, whatever target-dir a setting of Cargo's names,
# and go builds jaw-engine from engine/go.mod alone, whatever go.work a folder above it holds.
export CARGO_TARGET_DIR="$repo/target" GOWORK=off
```

After line 43, the `JawdVersion` line that ends `"$app/Contents/Info.plist"`, add:

```sh
again="$(mac/scripts/built-from "$repo" "$stamp")"
if [ "$again" != "$built" ]; then
    echo "==> the checkout changed while make mac built it: run it again" >&2
    exit 1
fi
from="$(echo "$built" | sed -n 1p)"
at="$(echo "$built" | sed -n 2p)"
plutil -replace JawBuiltFrom -string "$from" "$app/Contents/Info.plist"
plutil -replace JawBuiltAt -string "$at" "$app/Contents/Info.plist"
```

Line 49, the note when there's no Developer ID, becomes:

```sh
    echo "==> no Developer ID Application identity for team $team: signing ad hoc, so the app won't open under the hardened runtime"
```

The last line becomes:

```sh
echo "==> $app ($(du -sh "$app" | cut -f1)), jawd $jawd_version, built from $at"
```

Run: `sh -n mac/scripts/bundle && sh -n mac/scripts/built-from && echo parsed`
Expected: `parsed`.

- [ ] **Step 8: Commit**

```bash
git add mac/JawKit/Sources/Client/Build.swift mac/Tests/JawKitTests/BuildTests.swift mac/scripts/built-from mac/scripts/bundle
git commit -F- <<'EOF'
make mac records where a Jaw was built from in its Info.plist, JawBuiltFrom and JawBuiltAt, from mac/scripts/built-from, which reads git and takes none of its locks: main when HEAD is on origin/main, release when it's also at a tag like v0.1.0, branch otherwise, a tag on a commit off origin/main and a tree outside git included, and changed when the tree has uncommitted changes to what jawd and jaw-engine are built from, the app's own sources, the docs and the READMEs not counting; JawKit's Build reads it, and only a Jaw from origin/main or a release's tag on it is to start a jawd on an account's folder, where a branch's jawd could move jaw.db to a version main's can't open, the others saying what they were built from and how to start one, and the bundle says a Jaw it signs ad hoc won't open, since under the hardened runtime library validation refuses it its own frameworks; tests: Build reads the keys, starts only from main or a release and says why not, and built-from said branch here, changed with a new file under crates, branch with one under mac, main for the Mac builder's checkout, branch outside git, and in a throwaway repository branch for a tag off origin/main and release for one on it
EOF
```

**Amended while it was built.** Its review (30 October) found the lines above short of the Decision's "It holds with nobody remembering anything": built-from said main for a jawd that wasn't main's in cases they left open. Three of its git calls could be steered by the repository. Git takes a branch or tag called origin/main before the remote's ref, and every worktree shares the refs. `git status --porcelain` hid untracked files under `status.showUntrackedFiles=no`, though Go compiles every `.go` file in a package and Cargo runs a new `build.rs` by itself. And a `git status` that failed inside `[ -n "$(…)" ]` counted as a clean tree, since a failure there doesn't stop `set -e`. The bundle read the tree minutes after it compiled jawd from it, so a `git stash`, a checkout or a fast-forwarding pull meanwhile recorded main for a jawd built from a branch or a changed tree. A clone of a local checkout, as this project's audits make (`git clone <worktree> /tmp/jaw-audits/…`), has that checkout's main for its origin/main. A folder inside the checkout, given as the argument, made the paths it looks at relative to that folder. Cargo reads `.cargo/config.toml` and `rust-toolchain(.toml)` beside the sources, go reads a `go.work` above `engine/`, and the Makefile's engine target builds jaw-engine, none of them among the six paths; and the bundle copied `target/release/jawd` whether or not cargo had built there, as with `CARGO_TARGET_DIR` or a `target-dir` it builds elsewhere. As Task 2's first fix has it, built-from reads origin/main by its whole ref with `git show-ref --verify`, which takes the name exactly, since even `refs/remotes/origin/main` falls back to a tag called `refs/remotes/origin/main` when the remote's ref is missing. It counts as changes every untracked file `git ls-files --others` finds but those the repository's own .gitignore files leave out, so neither a `status.*` setting nor an ignore file of your own, `.git/info/exclude` or `core.excludesFile`, hides one, and every file git is told to assume unchanged or skip, which `git status` passes over (`changed` and `<branch> at <hash>, with files jawd is built from whose changes git is told to ignore`). It keeps each git call that reads the tree on a line of its own, so one that fails stops it, and `make mac` with it, before anything is recorded. It counts origin/main only where origin, as git fetches it (`git remote get-url`, which follows `url.<base>.insteadOf`), is the project's own repository, `github.com/pdcgomes/jaw` as Cargo.toml names it, in its https or ssh form, with or without `.git`, and otherwise says `branch` and `<branch> at <hash>, in a checkout whose origin isn't github.com/pdcgomes/jaw`. It reads the checkout from its top (`git rev-parse --show-toplevel`), and counts `.cargo`, `rust-toolchain`, `rust-toolchain.toml`, `go.work` and the `Makefile` with the six paths. The bundle takes the record before the builds and checks, before it writes the keys, that built-from still says the same, stopping with "the checkout changed while make mac built it: run it again" when it doesn't, and has cargo build into the checkout's own `target` (`CARGO_TARGET_DIR`), where it copies the helpers from. `mac/scripts/test-built-from`, which `make mac-test` runs before the build Mac's suites, tries 46 cases on throwaway repositories under one `mktemp -d` folder removed on every way out, git kept inside it by `GIT_CEILING_DIRECTORIES` with its own locations unset, and git's global and system settings left out: the review's (a branch or a tag called origin/main, `status.showUntrackedFiles=no`, an index git can't read, a clone of a local checkout, a folder inside the checkout, a new `.cargo/config.toml`, `rust-toolchain`, `rust-toolchain.toml` or `go.work`, and a changed Makefile), the fixes' own (a tag called `refs/remotes/origin/main` with no origin/main, an origin `url.insteadOf` sends to a local checkout, no origin, `.git/info/exclude`, an ignore file of your own, and files git is told to assume unchanged or skip), and the ordinary ones of Step 6 and the review, with the project's origin. Task 2's first built-from said main in 17 of them, and the first fix's passed all 46. `BuildTests.swift` also expects `"Main"` and `""` to start nothing and `Build(info: ["JawBuiltFrom": true]).from` to be nil, which a `Build` that refused only `branch` and `changed` and read any value as a word passed, and fails with 3 issues.

**Amended again.** Its re-review (31 October) found three more ways to main for a jawd that wasn't main's. The working tree's ignore files still hid what jawd is built from: a new `.gitignore` under `crates` reading `*` hid itself and a new `build.rs` beside it, which Cargo runs by itself, and the checkout's `.gitignore`, changed and not committed, or origin/main's own, hid a new `build.rs`, `go.work` or `.cargo/config.toml`, though a `go.work` can replace whatsmeow, session.db's code. GNU make reads a `GNUmakefile`, or a `makefile` on a case-sensitive volume, in place of the `Makefile`, and `scripts/version` and `VERSION` give jawd and jaw-engine their version, which the engine gives WhatsApp as the device's, none of them counted. And git could be pointed at another tree: `GIT_DIR` and `GIT_WORK_TREE` in `make mac`'s environment, `core.worktree` in the checkout's config, or a replace ref, which every worktree shares, standing a branch's commit in for main's. As the second fix has it, built-from counts as changes every untracked file under what jawd is built from but a `.DS_Store`, whatever any ignore file says, and counts `GNUmakefile`, `makefile`, `scripts/version` and `VERSION` with the rest; it unsets git's location variables, takes no replace refs and no grafts (`GIT_NO_REPLACE_OBJECTS=1`, and a `GIT_GRAFT_FILE` under `/var/empty`, which is root's and empty), and refuses, printing nothing, a folder whose work tree, as `core.worktree` can set it, doesn't hold it. It also closes two of the gaps the first round left. The bundle builds with `GOWORK=off`, so go reads no `go.work`, the checkout's or one above it. And the bundle makes a stamp before its first reading and gives it to the second: built-from, given a file, says `changed` and `<branch> at <hash>, with what jawd is built from written since <file>` when anything it counts, a folder included, was written since, so a change made and undone while `make mac` built, a file made and removed again included, stops it too. A `.DS_Store` written since doesn't count, but Finder making a new one changes its folder, so that `make mac` stops and has to be run again. `mac/scripts/test-built-from` also unsets `GIT_TEMPLATE_DIR`, so no template's hooks run on its commits, and tries 67 cases, 21 of them new: the review's (the five ignore-file cases above, a new `GNUmakefile`, a changed `scripts/version` or `VERSION`, `GIT_DIR` and `GIT_WORK_TREE`, `core.worktree`, a replace ref, `release` in a linked worktree, detached and shallow, at an annotated tag as `scripts/release.sh` has it made, and an `ssh://` origin without `.git`), a graft, and the stamp's (a file put back and one made and removed again since it, which say `changed`, a `.DS_Store` written since it, which doesn't, and a stamp that isn't there, which fails). The first fix's built-from said main in 15 of them, and the second fix's passes all 67. What it still doesn't see: a file made at the checkout's top and removed again while `make mac` builds, a change a filter of git's hides, a change undone with its files' times put back, and settings outside the checkout, Cargo's and go's own and the environment's. Step 6's throwaway repository is gone, its cases being `test-built-from`'s. The Files line, the Interfaces and Steps 1, 5 and 7 show the code as it now is, and Step 6 what it prints.

---

### Task 3: Who starts jawd, and what the window says meanwhile

**Files:**
- Create: `mac/JawKit/Sources/Client/Starter.swift`
- Test: `mac/Tests/JawKitTests/StarterTests.swift`

**Interfaces:**
- Consumes: `Spawn` and `Spawn.said(_:)` (Task 1); `Build`, `startsOnAccounts` and `refusal` (Task 2).
- Produces, in JawKit, `public struct Starter: Sendable`:
  - `Starter.Here`, what the app knows at launch: `debug`, `demo`, `home`, `jawsOwn`, `account`, `build: Build`, `jawd: String?`, `environment: [String: String]` and `terminalAgent: Bool`, with `public init(debug:demo:home:jawsOwn:account:build:jawd:environment:terminalAgent:)`.
  - `Starter.Step`: `.unregister`, `.start(Spawn)`, `.wait(TimeInterval)`, `.say(String, warn: Bool)`.
  - `Starter.Shown`: `.waiting`, `.starting`, `.failed(String)`, `.unreachable`.
  - `public static let patience: TimeInterval` (15), `again: TimeInterval` (1), `held: String` (`"another jawd is running on "`).
  - `public init(_ here: Here)` and `public let here: Here`.
  - `public mutating func launched(answers: Bool, now: Date) -> ([Step], Shown)`: a jawd to start comes before `.unregister`.
  - `public mutating func unregistered(removed: Bool, answers: Bool, now: Date) -> [Step]`: acts once, after `.unregister`.
  - `public func unreachable(now: Date) -> Shown`: what the window says while jaw-client can't reach jawd; it starts nothing.
  - `public mutating func exited(status: Int32, said: String, now: Date) -> [Step]`: `.wait(again)` when another jawd held the folder within the 15 s, nothing otherwise.
  - `public mutating func waited(answers: Bool, now: Date) -> [Step]`: after `.wait`, the jawd to start again, once, unless a jawd answers or has said hello.
  - `public mutating func didntStart(_ why: String)` and `public mutating func answered()`.
  - `public var spawn: Spawn?`: the jawd to start, nil without one.
  - `public static func same(_ a: String, _ b: String) -> Bool`: whether two paths are one folder.

- [ ] **Step 1: Write the failing tests**

Create `mac/Tests/JawKitTests/StarterTests.swift`. It uses `repo`, from `LiveTests.swift`:

```swift
import Foundation
@testable import JawKit
import Testing

/// What a Jaw knows at launch: a release built from main, on jaw's own folder, unless told otherwise.
func facts(debug: Bool = false, demo: Bool = false, own: Bool = true, account: Bool = false, from: String? = "main",
           jawd: String? = "/Jaw.app/Contents/Helpers/jawd", terminalAgent: Bool = false) -> Starter.Here
{
    Starter.Here(debug: debug, demo: demo, home: own ? "/u/Library/Application Support/jaw" : "/tmp/jaw-x", jawsOwn: own, account: account,
                 build: Build(from: from, at: from.map { "\($0) at 1a2b3c4" }), jawd: jawd,
                 environment: ["PATH": "/usr/bin:/bin"], terminalAgent: terminalAgent)
}

/// The moment of each test's launch.
let t0 = Date(timeIntervalSince1970: 1_800_000_000)

@Test func `a release from main on jaw's own folder starts the jawd inside it at once, in a group of its own, and only then asks for an earlier Jaw's agent to be unregistered`() {
    var starter = Starter(facts())
    let (first, shown) = starter.launched(answers: false, now: t0)
    let spawn = Spawn(program: "/Jaw.app/Contents/Helpers/jawd", arguments: [],
                      environment: ["PATH": "/usr/bin:/bin", "JAW_HOME": "/u/Library/Application Support/jaw"],
                      log: "/u/Library/Application Support/jaw/jawd.log", ownGroup: true)
    #expect(first == [.start(spawn), .unregister] && shown == .starting, "jawd first, then SMAppService, which nothing waits on")
    let answered = starter.unregistered(removed: false, answers: false, now: t0 + 0.2)
    #expect(answered.isEmpty, "with nothing found, the jawd already starting is the one")
    let again = starter.unregistered(removed: true, answers: false, now: t0 + 0.3)
    #expect(again.isEmpty, "SMAppService's answer counts once")
    #expect(starter.unreachable(now: t0 + 14) == .starting)
    var starting = Starter(facts())
    _ = starting.launched(answers: false, now: t0)
    #expect(starting.unregistered(removed: true, answers: false, now: t0 + 0.2)
        == [.say("an earlier Jaw started jawd at login; it doesn't now, and Jaw starts it as it opens", warn: false)],
        "the jawd already starting is the one, so none is started beside it")
}

@Test func `an earlier Jaw's agent whose jawd answered is said once SMAppService has removed it, and a jawd of the app's takes its place unless another answers`() {
    var starter = Starter(facts())
    let (first, shown) = starter.launched(answers: true, now: t0)
    #expect(first == [.unregister] && shown == .waiting)
    #expect(starter.unreachable(now: t0 + 0.1) == .unreachable, "until SMAppService says the agent has gone, it's jaw-client's to say")
    let steps = starter.unregistered(removed: true, answers: false, now: t0 + 0.3)
    #expect(steps.first == .say("an earlier Jaw started jawd at login; it doesn't now, and Jaw starts it as it opens", warn: false))
    #expect(steps.count == 2 && starter.spawn.map { steps.last == .start($0) } == true)
    #expect(starter.unreachable(now: t0 + 0.4) == .starting)
    var other = Starter(facts())
    _ = other.launched(answers: true, now: t0)
    #expect(other.unregistered(removed: true, answers: true, now: t0 + 0.3).count == 1, "another jawd answers, so none is started")
    var nothing = Starter(facts())
    _ = nothing.launched(answers: true, now: t0)
    #expect(nothing.unregistered(removed: false, answers: false, now: t0 + 0.3).isEmpty,
            "with no agent removed, a jawd that went is jaw-client's to say, as in the terminal")
}

@Test func `a jawd the app started has the terminal's 15 s to answer, after which jaw-client's words show`() {
    var starter = Starter(facts(own: false))
    let (steps, shown) = starter.launched(answers: false, now: t0)
    #expect(steps.count == 1 && shown == .starting, "on a folder that isn't jaw's own, nothing's unregistered")
    #expect(starter.unreachable(now: t0 + 14.9) == .starting && starter.unreachable(now: t0 + 15) == .unreachable)
}

@Test func `a jawd that stopped because another held the folder is started again a second later while nothing answers, whatever the window does, for 15 s`() {
    let held = "another jawd is running on /tmp/jaw-x"
    var starter = Starter(facts(own: false))
    _ = starter.launched(answers: false, now: t0)
    let stopped = starter.exited(status: 1 << 8, said: held, now: t0 + 0.1)
    #expect(stopped == [.wait(Starter.again)], "the app waits a second on a timer of its own, not for jaw-client's next try")
    #expect(starter.unreachable(now: t0 + 0.5) == .starting)
    let again = starter.waited(answers: false, now: t0 + 1.1)
    #expect(starter.spawn.map { again == [.start($0)] } == true, "then starts it again")
    let twice = starter.waited(answers: false, now: t0 + 1.2)
    #expect(twice.isEmpty, "once for each wait")
    let late = starter.exited(status: 1 << 8, said: held, now: t0 + 15.1)
    #expect(late.isEmpty && starter.unreachable(now: t0 + 16.5) == .failed("jawd stopped as it started (exit status: 1): \(held)"),
            "past the 15 s it says why")
    var answering = Starter(facts(own: false))
    _ = answering.launched(answers: false, now: t0)
    _ = answering.exited(status: 1 << 8, said: held, now: t0 + 0.1)
    let taken = answering.waited(answers: true, now: t0 + 1.1)
    #expect(taken.isEmpty, "not while another jawd answers")
    var greeted = Starter(facts(own: false))
    _ = greeted.launched(answers: false, now: t0)
    _ = greeted.exited(status: 1 << 8, said: held, now: t0 + 0.1)
    greeted.answered()
    let after = greeted.waited(answers: false, now: t0 + 1.1)
    #expect(after.isEmpty && greeted.unreachable(now: t0 + 1.2) == .unreachable, "nor once a jawd has said hello")
}

@Test func `a jawd refused by jaw.lock while an earlier Jaw's agent's was stopping is started again once after the wait, and none beside it when SMAppService says it removed that agent`() {
    var starter = Starter(facts())
    let (first, shown) = starter.launched(answers: false, now: t0)
    #expect(starter.spawn.map { first == [.start($0), .unregister] } == true && shown == .starting,
            "nothing answers as the agent's jawd stops, so the app's starts before SMAppService is asked")
    let stopped = starter.exited(status: 1 << 8, said: Starter.held + starter.here.home, now: t0 + 0.1)
    #expect(stopped == [.wait(Starter.again)], "the agent's jawd held jaw.lock for the moment its stop took")
    let removed = starter.unregistered(removed: true, answers: false, now: t0 + 0.2)
    #expect(removed == [.say("an earlier Jaw started jawd at login; it doesn't now, and Jaw starts it as it opens", warn: false)],
            "the app's is already to be started again, so none is started beside it")
    let again = starter.waited(answers: false, now: t0 + 1.1)
    #expect(starter.spawn.map { again == [.start($0)] } == true, "then the app's is started again, the one start since it stopped")
    let twice = starter.waited(answers: false, now: t0 + 1.2)
    #expect(twice.isEmpty, "and only once")
}

@Test func `an earlier Jaw's agent whose jawd held jaw.lock against the app's as it started, then answered, is said once SMAppService has removed it, and a jawd of the app's takes its place, with 15 s of its own, unless another answers`() {
    let said = Starter.Step.say("an earlier Jaw started jawd at login; it doesn't now, and Jaw starts it as it opens", warn: false)
    var starter = Starter(facts())
    let (first, shown) = starter.launched(answers: false, now: t0)
    #expect(starter.spawn.map { first == [.start($0), .unregister] } == true && shown == .starting,
            "nothing answers while the agent's jawd starts, so the app's starts before SMAppService is asked")
    let stopped = starter.exited(status: 1 << 8, said: Starter.held + starter.here.home, now: t0 + 0.1)
    #expect(stopped == [.wait(Starter.again)], "the agent's jawd held jaw.lock as it started")
    let taken = starter.waited(answers: true, now: t0 + 1.1)
    #expect(taken.isEmpty, "by the wait's end the agent's jawd answers, so the app's isn't started again")
    let removed = starter.unregistered(removed: true, answers: false, now: t0 + 20)
    #expect(starter.spawn.map { removed == [said, .start($0)] } == true,
            "SMAppService has stopped the agent's jawd, and none of the app's is on its way, so the app's starts")
    #expect(starter.unreachable(now: t0 + 20.1) == .starting, "its 15 s count from this start, though more than 15 have passed since the first")
    let refused = starter.exited(status: 1 << 8, said: Starter.held + starter.here.home, now: t0 + 20.2)
    #expect(refused == [.wait(Starter.again)], "and one refused by jaw.lock within them is to be started again, as any is")
    var other = Starter(facts())
    _ = other.launched(answers: false, now: t0)
    _ = other.exited(status: 1 << 8, said: Starter.held + other.here.home, now: t0 + 0.1)
    _ = other.waited(answers: true, now: t0 + 1.1)
    #expect(other.unregistered(removed: true, answers: true, now: t0 + 20) == [said], "another jawd answers by then, so none is started")
}

@Test func `a jawd of the app's refused by jaw.lock past its 15 s, or one that couldn't be started, is tried again once SMAppService has removed an earlier Jaw's agent, the window saying Starting jawd, and its words again if it fails again`() {
    let said = Starter.Step.say("an earlier Jaw started jawd at login; it doesn't now, and Jaw starts it as it opens", warn: false)
    var starter = Starter(facts())
    let held = Starter.held + starter.here.home
    let (first, shown) = starter.launched(answers: false, now: t0)
    #expect(starter.spawn.map { first == [.start($0), .unregister] } == true && shown == .starting)
    let late = starter.exited(status: 1 << 8, said: held, now: t0 + 15.1)
    #expect(late.isEmpty && starter.unreachable(now: t0 + 16) == .failed("jawd stopped as it started (exit status: 1): \(held)"),
            "an agent's jawd slow to listen held jaw.lock past the 15 s, so it says why")
    let removed = starter.unregistered(removed: true, answers: false, now: t0 + 30)
    #expect(starter.spawn.map { removed == [said, .start($0)] } == true,
            "SMAppService has stopped that agent's jawd, and the app's that failed isn't on its way, so another starts")
    #expect(starter.unreachable(now: t0 + 30.1) == .starting, "the failure is no longer true, and this start has 15 s of its own")
    var unstarted = Starter(facts())
    _ = unstarted.launched(answers: false, now: t0)
    unstarted.didntStart("can't start jawd: Permission denied")
    let again = unstarted.unregistered(removed: true, answers: false, now: t0 + 0.2)
    #expect(unstarted.spawn.map { again == [said, .start($0)] } == true, "one that couldn't be started is tried once more")
    unstarted.didntStart("can't start jawd: Permission denied")
    #expect(unstarted.unreachable(now: t0 + 0.3) == .failed("can't start jawd: Permission denied"), "and says its words again")
}

@Test func `a jawd that stopped as it started for anything else says why until a jawd answers, and one that stops after it answered is jaw-client's to say`() {
    var starter = Starter(facts(own: false))
    _ = starter.launched(answers: false, now: t0)
    let newer = starter.exited(status: 1 << 8, said: "jaw.db is from a newer jawd (schema 19, this jawd knows 17)", now: t0 + 0.2)
    #expect(newer.isEmpty)
    #expect(starter.unreachable(now: t0 + 30)
        == .failed("jawd stopped as it started (exit status: 1): jaw.db is from a newer jawd (schema 19, this jawd knows 17)"))
    starter.answered()
    #expect(starter.unreachable(now: t0 + 31) == .unreachable)
    var killed = Starter(facts(own: false))
    _ = killed.launched(answers: false, now: t0)
    _ = killed.exited(status: 9, said: "", now: t0 + 0.2)
    #expect(killed.unreachable(now: t0 + 0.3) == .failed("jawd stopped as it started (signal: 9 (SIGKILL))"))
    var crashed = Starter(facts(own: false))
    _ = crashed.launched(answers: false, now: t0)
    crashed.answered()
    let steps = crashed.exited(status: 9, said: "", now: t0 + 600)
    #expect(steps.isEmpty && crashed.unreachable(now: t0 + 601) == .unreachable, "a jawd that crashed after it answered isn't started again")
    var unstarted = Starter(facts(own: false))
    _ = unstarted.launched(answers: false, now: t0)
    unstarted.didntStart("can't start jawd: Permission denied")
    #expect(unstarted.unreachable(now: t0 + 1) == .failed("can't start jawd: Permission denied"))
}

@Test func `on an account's folder a Jaw from a branch, or one that doesn't say, starts no jawd and unregisters nothing, saying why`() {
    for from in ["branch", "changed", nil] as [String?] {
        for (own, account) in [(true, false), (false, true)] {
            var starter = Starter(facts(own: own, account: account, from: from))
            let (steps, shown) = starter.launched(answers: false, now: t0)
            let why = Build(from: from, at: from.map { "\($0) at 1a2b3c4" }).refusal
            #expect(steps.isEmpty && shown == .failed(why), "from \(from ?? "nothing"), on \(own ? "jaw's own folder" : "one holding a session.db")")
            #expect(starter.unreachable(now: t0 + 30) == .failed(why), "said until a jawd answers")
        }
    }
    var answered = Starter(facts(from: "branch"))
    let (none, waiting) = answered.launched(answers: true, now: t0)
    #expect(none.isEmpty && waiting == .waiting, "a jawd that answers there is used, as any is")
    var elsewhere = Starter(facts(own: false, from: "branch"))
    let (_, starting) = elsewhere.launched(answers: false, now: t0)
    #expect(starting == .starting, "on a folder of its own with no session.db, a branch's starts its jawd")
    var empty = Starter(facts(jawd: nil))
    let (steps, shown) = empty.launched(answers: false, now: t0)
    #expect(steps.isEmpty && shown == .failed("there's no jawd inside this Jaw: make mac puts one in its Contents/Helpers"),
            "with nothing to take its place, an earlier agent isn't unregistered")
}

@Test func `a build from source starts only the demo's jawd, which the app stops, never on an account's folder, and says how to start one otherwise`() {
    var demo = Starter(facts(debug: true, demo: true, own: false, jawd: "/repo/target/debug/jawd"))
    let (steps, shown) = demo.launched(answers: false, now: t0)
    #expect(steps == [.start(Spawn(program: "/repo/target/debug/jawd", arguments: ["--engine", "demo", "--no-notify"],
                                   environment: ["PATH": "/usr/bin:/bin", "JAW_HOME": "/tmp/jaw-x"], log: "/tmp/jaw-x/jawd.log", ownGroup: false))])
    #expect(shown == .starting)
    var home = Starter(facts(debug: true, own: false))
    let (none, said) = home.launched(answers: false, now: t0)
    #expect(none.isEmpty && said == .failed("nothing answers on /tmp/jaw-x/jaw.sock, and a build from source starts only --demo's jawd: "
            + "start one there with jaw, or open Jaw with --demo"))
    for (own, account) in [(true, false), (false, true)] {
        var refused = Starter(facts(debug: true, demo: true, own: own, account: account))
        let (steps, shown) = refused.launched(answers: false, now: t0)
        #expect(steps.isEmpty && shown == .failed("--demo runs a made-up account, so not on jaw's own folder or another account's: "
                + "give it a folder of its own with --home"), "on \(own ? "jaw's own folder" : "one holding a session.db")")
    }
    var nothing = Starter(facts(debug: true, demo: true, own: false, jawd: nil))
    let (_, missing) = nothing.launched(answers: false, now: t0)
    #expect(missing == .failed("there's no jawd to run: make build builds it"))
}

@Test func `the terminal's agent is named with what removes it, on jaw's own folder alone, and nothing is unregistered beside it`() {
    for from in ["main", "branch"] {
        var starter = Starter(facts(from: from, terminalAgent: true))
        let (steps, _) = starter.launched(answers: true, now: t0)
        let starts = from == "main" ? ", and Jaw starts jawd as it opens" : ""
        #expect(steps == [.say("jaw agent install's LaunchAgent still starts jawd at login: jaw agent uninstall removes it\(starts)", warn: true)],
                "from \(from): the label is that agent's, so SMAppService is asked nothing")
    }
    var unanswered = Starter(facts(terminalAgent: true))
    let (steps, shown) = unanswered.launched(answers: false, now: t0)
    #expect(steps.count == 2 && unanswered.spawn.map { steps.last == .start($0) } == true && shown == .starting,
            "with nothing answering, its own jawd starts beside that agent, as the terminal's does")
    var elsewhere = Starter(facts(own: false, terminalAgent: true))
    let (none, _) = elsewhere.launched(answers: true, now: t0)
    #expect(none.isEmpty)
}

@Test func `jaw's own folder is known however its path is written`() throws {
    let root = "/tmp/jaws-\(UInt32.random(in: 0 ... .max))"
    let own = root + "/Application Support/jaw"
    try FileManager.default.createDirectory(atPath: own + "/spool", withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: root) }
    try FileManager.default.createSymbolicLink(atPath: root + "/link", withDestinationPath: own)
    try FileManager.default.createSymbolicLink(atPath: root + "/spool", withDestinationPath: own + "/spool")
    #expect(Starter.same(own + "/", own))
    #expect(Starter.same(root + "/Application Support/../Application Support/jaw", own))
    #expect(Starter.same(root + "/link", own), "through a symlink")
    #expect(Starter.same(root + "/spool/..", own), "through a symlink followed by .., as jawd would open it")
    #expect(Starter.same("/private" + own, own), "/tmp is /private/tmp")
    #expect(Starter.same("/System/Volumes/Data/private" + own, own), "through the data volume's firmlink")
    #expect(Starter.same(root + "/application support/JAW", own), "whatever its case")
    #expect(Starter.same(root + "/not yet/jaw", root + "/not yet/jaw/"), "a folder that isn't there yet")
    #expect(!Starter.same(root + "/Application Support/jaw-demo", own) && !Starter.same(root, own))
}

@Test func `the words the starter knows another jawd by are jawd's own`() throws {
    let main = try String(contentsOf: repo.appending(path: "crates/jawd/src/main.rs"), encoding: .utf8)
    #expect(main.contains("\"\(Starter.held){}\""), "jawd's lock says \(Starter.held)…")
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawKit`
Expected: exit 1, with `error: cannot find 'Starter' in scope` from `StarterTests.swift`.

- [ ] **Step 3: Write `Starter`**

Create `mac/JawKit/Sources/Client/Starter.swift`:

```swift
import Foundation

/// Who starts jawd for the app, and what the window says of jawd meanwhile, as the terminal's
/// `wire.rs` has them. When nothing answers at launch, a release starts the jawd inside it, in a
/// process group of its own so it goes on when the app quits, and `--demo` the demo's, which the app
/// stops. A jawd the app started has 15 s to answer, and one that stopped as it started says why from
/// its log, unless another jawd held the folder within 15 s of the first start, when it's started
/// again a second later while nothing answers, whether or not the window is drawing. On an account's
/// folder, jaw's own or one holding a `session.db`, only a Jaw built from origin/main or a release's
/// tag starts one, and `--demo` none. On jaw's own folder such a Jaw then asks for the agent an
/// earlier Jaw registered to be unregistered, never waiting on the answer, unless the terminal's
/// agent holds the label, which it names with what removes it. The app carries out its steps and
/// tells it what happens.
public struct Starter: Sendable {
    /// What the app knows at launch.
    public struct Here: Equatable, Sendable {
        /// A build from source, which starts no jawd but `--demo`'s.
        public var debug: Bool
        /// `--demo`: a made-up account, on a jawd the app starts and stops.
        public var demo: Bool
        /// jaw's folder, which holds the socket and the log.
        public var home: String
        /// Whether `home` is jaw's own folder, which holds the owner's account.
        public var jawsOwn: Bool
        /// Whether `home` holds an engine's `session.db`, as another account's folder does.
        public var account: Bool
        public var build: Build
        /// The jawd to start: the one inside the app for a release, the one `--demo` finds for it.
        public var jawd: String?
        /// The app's environment, which the jawd it starts gets with `JAW_HOME` set to `home`.
        public var environment: [String: String]
        /// Whether `jaw agent install`'s plist is in `~/Library/LaunchAgents`.
        public var terminalAgent: Bool

        public init(debug: Bool, demo: Bool, home: String, jawsOwn: Bool, account: Bool, build: Build, jawd: String?,
                    environment: [String: String], terminalAgent: Bool)
        {
            self.debug = debug
            self.demo = demo
            self.home = home
            self.jawsOwn = jawsOwn
            self.account = account
            self.build = build
            self.jawd = jawd
            self.environment = environment
            self.terminalAgent = terminalAgent
        }
    }

    /// What the app does for it.
    public enum Step: Equatable, Sendable {
        /// Ask `SMAppService` to unregister the agent an earlier Jaw registered, then tell `unregistered`.
        case unregister
        /// Start this jawd, then tell `exited` when it goes, or `didntStart` if it can't be started.
        case start(Spawn)
        /// Wait this long, whatever the window does, then tell `waited`.
        case wait(TimeInterval)
        /// Say this in the status line.
        case say(String, warn: Bool)
    }

    /// What the window says of jawd until it answers.
    public enum Shown: Equatable, Sendable {
        /// Nothing yet: a jawd may answer.
        case waiting
        /// A jawd the app started is starting.
        case starting
        /// It didn't start, or the app won't start one here, and why.
        case failed(String)
        /// Nothing the app started is on its way: jaw-client's words, as it tries again.
        case unreachable
    }

    /// How long a jawd the app started has to answer, as the terminal waits (`PATIENCE` in `wire.rs`).
    public static let patience: TimeInterval = 15
    /// How long after a jawd stopped because another held the folder it's started again, on the app's
    /// own timer, since jaw-client's failed tries are drained only at the window's display frames.
    public static let again: TimeInterval = 1
    /// What jawd says when another jawd holds the folder's `jaw.lock` (`lock` in crates/jawd/src/main.rs).
    public static let held = "another jawd is running on "

    public let here: Here
    /// When the app first started a jawd that hasn't answered yet.
    var since: Date?
    /// Whether the jawd it started last is running.
    var running = false
    /// Whether it's to be started again once the app has waited, after it stopped because another
    /// jawd held the folder.
    var restarting = false
    /// Why it didn't start, or why the app won't start one here, until a jawd answers or the app
    /// starts another.
    var failed: String?
    /// Whether `SMAppService` has yet to answer for an earlier Jaw's agent.
    var unregistering = false

    public init(_ here: Here) {
        self.here = here
    }

    /// At launch, once the app knows whether a jawd answers: what it does, a jawd to start coming before
    /// anything asked of `SMAppService`, and what the window says until jawd's first word.
    public mutating func launched(answers: Bool, now: Date) -> ([Step], Shown) {
        var steps: [Step] = []
        if here.jawsOwn, here.terminalAgent {
            let starts = refusal == nil ? ", and Jaw starts jawd as it opens" : ""
            steps.append(.say("jaw agent install's LaunchAgent still starts jawd at login: jaw agent uninstall removes it\(starts)", warn: true))
        }
        steps += decide(answers: answers, now: now)
        if here.jawsOwn, !here.terminalAgent, refusal == nil {
            unregistering = true
            steps.append(.unregister)
        }
        if let failed { return (steps, .failed(failed)) }
        return (steps, running ? .starting : .waiting)
    }

    /// `SMAppService` answered for the earlier Jaw's agent, `removed` when there was one, whose jawd it
    /// has stopped by then: when nothing answers and no jawd of the app's is running or to be started
    /// again, the app starts its own, even if an earlier one failed, with 15 s from then to answer.
    public mutating func unregistered(removed: Bool, answers: Bool, now: Date) -> [Step] {
        guard unregistering else { return [] }
        unregistering = false
        guard removed else { return [] }
        let said = Step.say("an earlier Jaw started jawd at login; it doesn't now, and Jaw starts it as it opens", warn: false)
        guard !running, !restarting else { return [said] }
        since = nil
        failed = nil
        return [said] + decide(answers: answers, now: now)
    }

    /// jaw-client couldn't reach jawd: what the window says.
    public func unreachable(now: Date) -> Shown {
        if let failed { return .failed(failed) }
        if let since, running || restarting, now.timeIntervalSince(since) < Self.patience { return .starting }
        return .unreachable
    }

    /// The jawd the app started stopped, with this wait status and these last words in its log: when
    /// another jawd held the folder within 15 s of the first start, the app waits before it's started
    /// again; past them, or when it stopped for another reason, the window says why.
    public mutating func exited(status: Int32, said: String, now: Date) -> [Step] {
        guard let since else { return [] }
        running = false
        guard said.hasPrefix(Self.held), now.timeIntervalSince(since) < Self.patience else {
            failed = "jawd stopped as it started (\(Spawn.said(status)))" + (said.isEmpty ? "" : ": \(said)")
            return []
        }
        restarting = true
        return [.wait(Self.again)]
    }

    /// The app has waited after a jawd stopped because another held the folder: the jawd to start
    /// again, once, unless a jawd answers now or has said hello since.
    public mutating func waited(answers: Bool, now: Date) -> [Step] {
        guard restarting else { return [] }
        restarting = false
        return answers ? [] : start(now)
    }

    /// The jawd couldn't be started at all, for this reason.
    public mutating func didntStart(_ why: String) {
        running = false
        failed = why
    }

    /// A jawd answered: what becomes of it from now on is jaw-client's to say.
    public mutating func answered() {
        since = nil
        running = false
        restarting = false
        failed = nil
    }

    /// The jawd to start: a release's own, in a process group of its own, or the demo's, which the
    /// app stops, each with its output appended to `jawd.log` beside the socket.
    public var spawn: Spawn? {
        guard let jawd = here.jawd else { return nil }
        var environment = here.environment
        environment["JAW_HOME"] = here.home
        let log = (here.home as NSString).appendingPathComponent("jawd.log")
        if here.demo {
            return Spawn(program: jawd, arguments: ["--engine", "demo", "--no-notify"], environment: environment, log: log, ownGroup: false)
        }
        return Spawn(program: jawd, arguments: [], environment: environment, log: log, ownGroup: true)
    }

    /// Why the app won't start a jawd here, if it won't.
    var refusal: String? {
        let socket = (here.home as NSString).appendingPathComponent("jaw.sock")
        let account = here.jawsOwn || here.account
        if here.demo, account {
            return "--demo runs a made-up account, so not on jaw's own folder or another account's: give it a folder of its own with --home"
        }
        if here.demo { return here.jawd == nil ? "there's no jawd to run: make build builds it" : nil }
        if here.debug {
            return "nothing answers on \(socket), and a build from source starts only --demo's jawd: start one there with jaw, or open Jaw with --demo"
        }
        if account, !here.build.startsOnAccounts { return here.build.refusal }
        if here.jawd == nil { return "there's no jawd inside this Jaw: make mac puts one in its Contents/Helpers" }
        return nil
    }

    mutating func decide(answers: Bool, now: Date) -> [Step] {
        guard !answers else { return [] }
        if let why = refusal {
            failed = why
            return []
        }
        return start(now)
    }

    mutating func start(_ now: Date) -> [Step] {
        guard let spawn else { return [] }
        since = since ?? now
        running = true
        return [.start(spawn)]
    }

    /// Whether two paths are one folder: the same once standardised, symlinks resolved and case
    /// ignored, or, for a folder that's there, the same file on the same volume as the kernel finds it
    /// at each path as written, through symlinks, `..` after one, and firmlinks.
    public static func same(_ a: String, _ b: String) -> Bool {
        func path(_ p: String) -> String {
            URL(fileURLWithPath: p).standardizedFileURL.resolvingSymlinksInPath().path
        }
        if path(a).lowercased() == path(b).lowercased() { return true }
        func identity(_ p: String) -> (dev_t, ino_t)? {
            var info = stat()
            guard stat(p, &info) == 0 else { return nil }
            return (info.st_dev, info.st_ino)
        }
        guard let first = identity(a), let second = identity(b) else { return false }
        return first == second
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `mac/scripts/build-mac test JawKit`
Expected: exit 0, `** TEST SUCCEEDED **`, with ten more tests than after Task 2.

- [ ] **Step 5: Commit**

```bash
git add mac/JawKit/Sources/Client/Starter.swift mac/Tests/JawKitTests/StarterTests.swift
git commit -F- <<'EOF'
JawKit's Starter decides who starts jawd for the app and what the window says meanwhile, as the terminal's wire.rs has them: when nothing answers at launch, a release starts the jawd inside it in a process group of its own and --demo the demo's, which the app stops, each with its output appended to jawd.log beside the socket; a jawd it started has the terminal's 15 s to answer, one that stopped as it started says why from its log's last words, and one that stopped because another jawd held the folder, by jawd's own words, is started again a second later while nothing answers, after a wait the app keeps itself rather than at jaw-client's next try, which waits for the window's display frames; on an account's folder, jaw's own, known however its path is written, or one holding a session.db, only a Jaw built from origin/main or a release's tag starts one and --demo never runs, and on jaw's own folder such a Jaw asks for the agent an earlier Jaw registered to be unregistered only after starting its jawd, never waiting on the answer, starting its own once the answer says the agent whose jawd answered has gone, and asks nothing while the terminal's agent, which it names with jaw agent uninstall, holds the label; tests: each of those, and jawd's words read from crates/jawd/src/main.rs
EOF
```

**Amended while it was built.** Its review (4 November) found the code and tests above to be the plan's and to meet the Decisions, but no test reaching the one guard against a second start when `SMAppService` answers: with `unregistered`'s `guard since == nil, failed == nil` removed, all 133 of JawKit's tests passed, since the first test's `removed: true` came after its `removed: false`, stopping at the guard that makes the answer count once, and the second test's starters had started no jawd. It also found two departures from the terminal's `unreachable` that the Decisions don't name, both keeping a true reason on screen longer than the terminal does: a jawd of the app's that couldn't be started is said until a jawd answers, where the terminal says it once, and one that stops past its 15 s without having answered is said as stopped as it started until a jawd answers, where by then the terminal says nothing of it. It left four Minors for the branch review (`unreachable`'s `running || restarting` untested, `same` letting the paths' spellings answer before the kernel does, `held` pinned by jawd's format string rather than by what jawd writes, and doc comments saying less than the code), and for Task 5 what the starter can't check, among them that `Here.jawd` is the bundle's own jawd and that the restart rests on the refused jawd's line being the last in `jawd.log`, which others write too. On 4 November the owner chose to add two tests beyond the plan's for the guard: the review's three lines, which end the first test, where `SMAppService`'s answer comes while the app's jawd is starting and only says so; and a test of the sequence of an earlier agent's jawd still stopping as the app's starts, where `jaw.lock` refuses the app's, `SMAppService`'s answer during the wait only says so, and the wait starts the app's jawd once. Without the guard both fail and JawKit's other 132 pass. He chose to keep the two departures, which Task 6 writes into the Mac design. The re-review (4 November) approved the tests and found that the guard keyed on `since`, which outlives the app's jawd being on its way. An earlier agent's jawd still starting at launch, holding `jaw.lock` before it listens, refused the app's; the wait found that jawd answering and started nothing; and `SMAppService`, answering once it had stopped that jawd and before its hello was applied, found the first start's `since`, so the app only said so and nothing started a jawd, the window saying jaw-client's words until Jaw was opened again. On 5 November the owner chose the fix: when `SMAppService` says it removed the agent and no jawd of the app's is on its way, running or to be started again (`running`, `restarting`), the app starts its own unless a jawd answers, whatever `since` says, so one of the app's on its way still keeps it from starting a second. A jawd of the app's that failed isn't on its way, so it doesn't keep the app from starting one: one that `jaw.lock` refused past its 15 s, while an agent's jawd slow to listen held it, is followed by another once `SMAppService` has stopped that jawd, and one that couldn't be started is tried once more, `unregistered` acting once. As first built, the fix also let a failure (`failed`) stop the start, beyond the owner's rule, which left the first of those saying a failure no longer true while nothing started a jawd. The jawd started then has 15 s from its own start, since the earlier attempt is over and `SMAppService`'s answer has no cap: the window says Starting jawd for it, whatever an earlier one's failure said, one `jaw.lock` refuses within them is started again, as any is, and a failure of it is said in its own words. `unregistered` clears `since` and `failed` before it starts, and its doc comment and `failed`'s say what it now does. One new test takes the re-review's order whole, `SMAppService` answering 20 s after launch, and holds that a jawd answering by then keeps the app's from starting: on the fix as first built, it failed three times with the guard keyed on `since`, and twice with `since` kept from the first start. Another takes the two failures, `SMAppService` answering 30 s after a launch whose jawd was refused past its 15 s, and a jawd that couldn't be started, tried once more and saying its words again when it fails again: it fails three times with the guard also keyed on `failed`, as first built, and once with `failed` left as it was. JawKit's other tests passed each time, and with the fix all 136 pass. Steps 1 and 3 show the code as it now is: Step 1 holds thirteen tests, three more than Step 4 counts, so JawKit has 136.

---

### Task 4: What the window shows, traced, and the check that reads it

The check is written first and run on a Release build of the branch as Task 3 leaves it, so its claims are seen failing. Then the app traces what its window shows of jawd, which the check reads, and the check runs again: each run's frames are printed now, and its claims about starting jawd still fail, as they must until Task 5.

**Files:**
- Modify: `mac/JawApp/Sources/AppModel.swift` (after `trace` at 50; `start()`'s last lines at 101-102; `drain()`'s trace line at 122 and its end at 125-126; after `stateView` at 231-234)
- Scratch: `mac/build/start/try-start.sh`

**Interfaces:**
- Consumes: `mac/scripts/built-from` (Task 2), for the record the check's bundle carries.
- Produces, in the app: `AppModel.traced: String?`, `AppModel.traceShown()` and `AppModel.sinceStart: Int`, the milliseconds since the process started, as the trace counts them.
- Produces, with `JAW_TRACE=1`, on standard error: each drained update's line, `jaw-mac: <the update> at <ms> ms`, now ending with the time it was drained; and beside those, `jaw-mac: shown at <ms> ms: <the status line's state, or online> · <what fills the panes' place, by its case, or chats>`, then ` · <jawd's words>` when it didn't start, ending ` · window hidden` when the window isn't visible: a line each time any of that changes, once a frame with it is on screen, its time since the process started.
- Produces: the check, `mac/build/start/try-start.sh`, which also reads the lines Task 5 adds: `jaw-mac: started jawd <pid>`, `jaw-mac: jawd <pid> stopped (…)`, `jaw-mac: unregistering an earlier Jaw's agent at <ms> ms, …` and `jaw-mac: unregistered <n> ms after asking: …`.

- [ ] **Step 1: Write the check**

Write `mac/build/start/try-start.sh`:

```sh
#!/bin/sh
# Parity step 1.2's check, on the build Mac's build user, which has no jaw folder, no jaw LaunchAgent
# and nothing of jaw's in launchd. A release Jaw built without the hardened runtime, whose library
# validation refuses an app signed ad hoc its own frameworks, around the Mac builder's jawd, with no
# engine anywhere jawd would look for one, so nothing reaches WhatsApp, opened as the Finder opens
# apps, its trace on. It starts jawd when nothing answers, saying so in every frame until jawd
# answers; jawd goes on when the app quits and when the app's process group is killed; quitting jawd
# and opening the app brings it back, here while another process holds the folder's lock until 3 s
# after the app's first jawd was refused; a Jaw built from a branch starts none on jaw's own folder;
# and one recorded as main's starts one there at once, asking SMAppService only after that for an
# earlier Jaw's agent. Each claim is printed ok or FAIL. What it starts is stopped by PID, the app's
# preferences are put back as it found them, and its folder, and the jaw folder it made, are removed
# on the way out, a dropped ssh session included.
set -u
from=${JAW_BUILT_FROM:?} at=${JAW_BUILT_AT:?} main=${JAW_MAIN_AT:?}
checkout=~/src/build-mac/jaw/jaw-mac-start
own="$HOME/Library/Application Support/jaw"
label=dev.badcoffee.jaw.jawd
prefs=dev.badcoffee.jaw
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
# By its real path, as LaunchServices runs the app and the app names the jawds it starts: the build
# user's TMPDIR ends in a slash, and /var is /private/var.
scratch=$(mktemp -d "${TMPDIR%/}/jaw-start.XXXXXX") && scratch=$(cd "$scratch" && pwd -P) || exit 1
app=$scratch/Jaw.app
home=$scratch/home
made_own=""
kept_prefs=""
failed=0
caffeinate -i -d -w $$ > /dev/null 2>&1 &

# Stops what names the check's folder (the apps, the jawds inside them and the lock's holder) by
# PID, then puts the app's preferences back as the check found them, and removes LaunchServices' note
# of its app, the jaw folder the check made and the folder. Writes nothing to the session: it may run
# once the ssh session has gone.
cleanup() {
    for pid in $(pgrep -f "$scratch/"); do
        kill -TERM "$pid" 2> /dev/null
    done
    sleep 1
    for pid in $(pgrep -f "$scratch/"); do
        kill -KILL "$pid" 2> /dev/null
    done
    [ -d "$app" ] && "$lsregister" -u "$app" > /dev/null 2>&1
    case "$kept_prefs" in
        had) defaults import "$prefs" "$scratch/prefs.plist" > /dev/null 2>&1 ;;
        none) defaults delete "$prefs" > /dev/null 2>&1 ;;
    esac
    [ -n "$made_own" ] && rm -rf "$own"
    rm -rf "$scratch"
}
trap cleanup EXIT
# `ssh ... 'sh -s'` gives the script no terminal, so a dropped session sends no HUP: the script finds
# out at its next write back, which raises PIPE.
trap 'exit 1' HUP INT TERM PIPE

# Prints "ok: $1" when the command after it succeeds, and "FAIL: $1" otherwise.
claim() {
    what=$1
    shift
    if "$@"; then
        echo "ok: $what"
    else
        echo "FAIL: $what"
        failed=1
    fi
}

# Waits up to $1 seconds for the command after it to succeed.
within() {
    tenths=$(($1 * 10))
    shift
    until "$@"; do
        tenths=$((tenths - 1))
        [ "$tenths" -le 0 ] && return 1
        sleep 0.1
    done
}

# Whether something listens on the socket in folder $1.
answers() {
    python3 -c 'import socket, sys; s = socket.socket(socket.AF_UNIX); s.settimeout(1); s.connect(sys.argv[1])' "$1/jaw.sock" 2> /dev/null
}

# Whether nothing does.
silent() {
    ! answers "$1"
}

# Whether PID $1, one the check knows, has gone.
gone() {
    [ -n "$1" ] && ! kill -0 "$1" 2> /dev/null
}

# Whether PID $1 leads a process group of its own.
leads() {
    [ -n "$1" ] && [ "$(ps -o pgid= -p "$1" | tr -d ' ')" = "$1" ]
}

# Whether PID $1 runs as an app LaunchServices starts does, with launchd as its parent.
launched() {
    [ -n "$1" ] && [ "$(ps -o ppid= -p "$1" | tr -d ' ')" = 1 ]
}

# Whether jawd $1 still runs and answers on folder $2.
runs() {
    [ -n "$1" ] && kill -0 "$1" 2> /dev/null && answers "$2"
}

# Whether $1 is all of $2, an extended regular expression.
matches() {
    printf '%s\n' "$1" | grep -Eqx "$2"
}

# Whether no engine runs on folder $1.
engineless() {
    ! pgrep -f "jaw-engine --home $1" > /dev/null
}

# Whether launchd has nothing under jaw's label in the build user's session: launchctl print says so
# with exit status 113, and any other failure says nothing either way.
unlisted() {
    launchctl print "gui/$(id -u)/$label" > /dev/null 2>&1
    [ $? -eq 113 ]
}

# Opens the app as the Finder does, through LaunchServices in the build user's session, with the
# arguments after $1 and its trace on into $1 (its standard output into $1.out), and prints the PID of
# the newest such app once it runs.
open_jaw() {
    trace=$1
    shift
    : > "$trace"
    : > "$trace.out"
    open -n -g --env JAW_TRACE=1 --stdout "$trace.out" --stderr "$trace" "$app" --args "$@" || return 1
    within 10 pgrep -f "^$app/Contents/MacOS/Jaw" > /dev/null || return 1
    pgrep -n -f "^$app/Contents/MacOS/Jaw"
}

# The PID of the last jawd the app's trace $1 says it started.
jawd_in() {
    sed -n 's/^jaw-mac: started jawd \([0-9][0-9]*\).*/\1/p' "$1" | tail -1
}

# Whether the app's trace $1 has jawd's hello in it.
hello() {
    grep -q '^jaw-mac: hello(' "$1"
}

# What the window showed of jawd in trace $1, a frame at a time: prints each frame's line, indented,
# on standard error, and six words on standard output: its states as letters in order (c
# connecting, s starting, f didn't start, u can't reach, l not linked, o anything else), the first
# frame's time and the first not-linked frame's, in milliseconds, 0 when there's none, how many of
# jaw-client's tries were drained before jawd's hello, the longest time in milliseconds, from the
# first frame on, between two of those drains, and whether every frame had the window visible. A
# failed try is drained at the window's next display frame, and a drained update's line ends with
# its time, so drains close together show the window drawing while what it shows stays the same.
shown() {
    python3 - "$1" << 'PY'
import os, sys

lines = open(sys.argv[1], encoding="utf-8", errors="replace").read().splitlines() if os.path.exists(sys.argv[1]) else []
frames = [line for line in lines if line.startswith("jaw-mac: shown at ")]
for line in frames:
    print("    " + line[len("jaw-mac: "):], file=sys.stderr)


def letter(line):
    for words, said in (("connecting to jawd", "c"), ("starting jawd", "s"), ("jawd didn't start", "f"),
                        ("can't reach jawd", "u"), ("not linked", "l")):
        if words in line:
            return said
    return "o"


def ms(line):
    return int(line.split("shown at ")[1].split(" ms")[0])


def drained(line):
    end = line.rsplit(" at ", 1)[-1]
    return int(end[:-3]) if end.endswith(" ms") and end[:-3].isdigit() else None


hello = next((i for i, line in enumerate(lines) if line.startswith("jaw-mac: hello(")), len(lines))
tries = [line for line in lines[:hello] if line.startswith("jaw-mac: unreachable(")]
letters = "".join(letter(line) for line in frames) or "-"
first = ms(frames[0]) if frames else 0
linked = next((ms(line) for line in frames if letter(line) == "l"), 0)
times = [drained(line) for line in tries]
marks = sorted([first] + [time for time in times if time is not None and time > first])
gap = max((b - a for a, b in zip(marks, marks[1:])), default=0)
hidden = any(line.endswith(" · window hidden") for line in frames)
print(letters, first, linked, len(tries), gap, "hidden" if hidden else "visible")
PY
}

# Reads what the window showed in trace $1 into letters, first, linked, tries, gap and visible.
saw() {
    read -r letters first linked tries gap visible << EOF
$(shown "$1")
EOF
}

# Whether every frame had the window visible, its states as letters all of $1.
frames() {
    [ "$visible" = visible ] && matches "$letters" "$1"
}

# Whether the build user is clean for jaw: no jaw folder, no terminal agent and nothing under jaw's
# label in launchd, no PATH, JAW_ENGINE or JAW_HOME that launchd gives the apps it opens, and no
# engine on the PATH they get.
clean() {
    [ ! -e "$own" ] && [ ! -e "$HOME/Library/LaunchAgents/$label.plist" ] && unlisted || return 1
    [ -z "$(launchctl getenv PATH)$(launchctl getenv JAW_ENGINE)$(launchctl getenv JAW_HOME)" ] || return 1
    for folder in /usr/bin /bin /usr/sbin /sbin; do
        [ -e "$folder/jaw-engine" ] && return 1
    done
    return 0
}
claim "the build user has no jaw folder, no terminal agent and nothing of jaw's in launchd, launchd gives apps no PATH, JAW_ENGINE or JAW_HOME, and no engine is on their PATH" clean
[ "$failed" = 0 ] || exit 1

echo "==> the check's Jaw: the Release app without the hardened runtime, with the Mac builder's jawd inside and no engine, built from $at"
ditto "$checkout/mac/build/DerivedData/Build/Products/Release/Jaw.app" "$app" || exit 1
mkdir -p "$app/Contents/Helpers"
cp "$checkout/target/debug/jawd" "$app/Contents/Helpers/jawd"
plutil -replace JawBuiltFrom -string "$from" "$app/Contents/Info.plist"
plutil -replace JawBuiltAt -string "$at" "$app/Contents/Info.plist"
codesign --force --sign - "$app/Contents/Helpers/jawd" 2> /dev/null
codesign --force --sign - --preserve-metadata=entitlements,requirements,flags "$app" 2> /dev/null
bundled() {
    codesign --verify --deep --strict "$app" && ! codesign -dv "$app" 2>&1 | grep -q runtime \
        && [ ! -e "$app/Contents/Helpers/jaw-engine" ] && [ ! -e "$app/Contents/Library/LaunchAgents" ]
}
claim "it's signed whole, without the hardened runtime, with no engine and no LaunchAgent" bundled
if defaults export "$prefs" "$scratch/prefs.plist" > /dev/null 2>&1; then kept_prefs=had; else kept_prefs=none; fi

echo "==> 1: nothing answers on a folder of its own, so it starts jawd; quitting leaves jawd running"
app1=$(open_jaw "$scratch/1.log" --home "$home" --snapshot "$scratch/1.png")
within 20 hello "$scratch/1.log"
jawd1=$(jawd_in "$scratch/1.log")
saw "$scratch/1.log"
claim "it runs ($app1), opened through LaunchServices: its parent is launchd" launched "$app1"
claim "it started jawd ($jawd1), which answered on the folder's socket" runs "$jawd1" "$home"
claim "that jawd leads a process group of its own" leads "$jawd1"
claim "no engine ran" engineless "$home"
claim "every frame had the window visible, any before jawd answered saying starting jawd, then its state ($letters, the first at $first ms)" frames 's*l+'
within 40 gone "$app1"
quit() {
    gone "$app1" && grep -q '"timed_out_s":30' "$scratch/1.log.out"
}
claim "the app quit by itself once its snapshot gave up, through NSApp.terminate, as Quit Jaw does" quit
outlived() {
    gone "$app1" && runs "$jawd1" "$home"
}
claim "jawd goes on, and answers, with the app gone" outlived

echo "==> 2: quitting jawd and opening the app brings it back, here while another process holds jaw.lock until 3 s after the app's first jawd was refused"
kill -TERM "$jawd1" 2> /dev/null
within 10 gone "$jawd1"
stopped() {
    gone "$jawd1" && silent "$home"
}
claim "jawd stopped when told to, and nothing answers" stopped
# Holds jaw.lock as another jawd would, until 3 s after the app's trace says its jawd stopped saying
# so, or for 20 s at most.
hold() {
    python3 - "$home/jaw.lock" "$scratch/2.log" << 'PY'
import fcntl, sys, time

lock = open(sys.argv[1], "a")
fcntl.flock(lock, fcntl.LOCK_EX)
print("held", flush=True)
until = time.monotonic() + 20
while time.monotonic() < until:
    try:
        refused = "another jawd is running on" in open(sys.argv[2], encoding="utf-8", errors="replace").read()
    except OSError:
        refused = False
    if refused:
        time.sleep(3)
        break
    time.sleep(0.05)
print("released", flush=True)
PY
}
hold > "$scratch/lock.log" 2>&1 &
within 5 grep -q held "$scratch/lock.log"
app2=$(open_jaw "$scratch/2.log" --home "$home")
within 30 hello "$scratch/2.log"
jawd2=$(jawd_in "$scratch/2.log")
saw "$scratch/2.log"
claim "it runs ($app2), opened through LaunchServices: its parent is launchd" launched "$app2"
again() {
    [ "$(grep -c '^jaw-mac: started jawd' "$scratch/2.log")" -ge 2 ] && grep -q 'another jawd is running on' "$home/jawd.log" \
        && grep -q released "$scratch/lock.log"
}
claim "while the lock was held, each jawd it started stopped saying so in jawd.log beside the socket, and once the lock was let go it started another" again
claim "the last one ($jawd2) answered" runs "$jawd2" "$home"
claim "that jawd leads a process group of its own" leads "$jawd2"
waited() {
    frames 's+l+' && [ $((linked - first)) -ge 2000 ] && [ "$tries" -ge 3 ] && [ "$gap" -le 2500 ]
}
claim "the window said starting jawd from its first frame, at $first ms, to jawd's answer, at $linked ms, drawing all the while: $tries of jaw-client's tries were drained at its frames, with at most $gap ms between drains, then jawd's state ($letters)" waited
claim "the app leads a process group of its own" leads "$app2"
led=""
leads "$app2" && led=1
if [ -n "$led" ]; then kill -KILL -- "-$app2"; else kill -KILL "$app2" 2> /dev/null; fi
within 5 gone "$app2"
killed() {
    [ -n "$led" ] && gone "$app2" && runs "$jawd2" "$home"
}
claim "killing the app's process group took the app and left jawd running and answering" killed
kill -TERM "$jawd2" 2> /dev/null
within 10 gone "$jawd2"

echo "==> 3: a Jaw built from a branch starts no jawd on jaw's own folder"
app3=$(open_jaw "$scratch/3.log")
sleep 3
saw "$scratch/3.log"
claim "it runs ($app3), opened through LaunchServices: its parent is launchd" launched "$app3"
refused() {
    frames 'f+' && grep -qF "this Jaw was built from $at, so it doesn't start jawd on jaw's own folder or another account's" "$scratch/3.log"
}
claim "the window says jawd didn't start, and why, from its first frame: this Jaw was built from $at ($letters)" refused
untouched() {
    ! grep -q '^jaw-mac: started jawd' "$scratch/3.log" && ! grep -q '^jaw-mac: unregistering' "$scratch/3.log" && [ ! -e "$own" ]
}
claim "it started no jawd, asked SMAppService nothing and made no jaw folder" untouched
if leads "$app3"; then kill -TERM -- "-$app3"; else kill -TERM "$app3" 2> /dev/null; fi
within 5 gone "$app3"

echo "==> 4: a Jaw recorded as main's starts jawd on jaw's own folder at once, and only then asks SMAppService about an earlier Jaw's agent"
# Makes jaw's folder, the check's own until it ends, only if nobody has made it by now: one mkdir
# looks and makes it at once.
own_folder() {
    mkdir -m 700 "$own" 2> /dev/null && made_own=1
}
claim "the build user still has no jaw folder, so the check makes it now, its own until the check ends" own_folder
if [ -n "$made_own" ]; then
    plutil -replace JawBuiltFrom -string main "$app/Contents/Info.plist"
    plutil -replace JawBuiltAt -string "$main on main, the check's Jaw around the Mac builder's jawd" "$app/Contents/Info.plist"
    codesign --force --sign - --preserve-metadata=entitlements,requirements,flags "$app" 2> /dev/null
    app4=$(open_jaw "$scratch/4.log")
    within 20 hello "$scratch/4.log"
    within 10 grep -q '^jaw-mac: unregistered ' "$scratch/4.log"
    jawd4=$(jawd_in "$scratch/4.log")
    grep '^jaw-mac: unregister' "$scratch/4.log" | sed 's/^jaw-mac: /    /'
    saw "$scratch/4.log"
    claim "it runs ($app4), opened through LaunchServices: its parent is launchd" launched "$app4"
    after() {
        started=$(grep -n '^jaw-mac: started jawd ' "$scratch/4.log" | head -1 | cut -d: -f1)
        asking=$(grep -n '^jaw-mac: unregistering ' "$scratch/4.log" | head -1 | cut -d: -f1)
        answer=$(grep -n '^jaw-mac: unregistered ' "$scratch/4.log" | head -1 | cut -d: -f1)
        [ -n "$started" ] && [ -n "$asking" ] && [ -n "$answer" ] && [ "$started" -lt "$asking" ] && [ "$asking" -lt "$answer" ]
    }
    claim "it started jawd, then asked SMAppService to unregister an earlier Jaw's agent, and the answer came after: nothing waited on it" after
    claim "that jawd ($jawd4) answered on jaw's own folder" runs "$jawd4" "$own"
    claim "that jawd leads a process group of its own" leads "$jawd4"
    claim "no engine ran" engineless "$own"
    claim "the window never said connecting to jawd: any frame before jawd answered said starting jawd, then jawd's state ($letters)" frames 's*l+'
    left() {
        unlisted && [ ! -e "$HOME/Library/LaunchAgents/$label.plist" ]
    }
    claim "nothing of jaw's is in launchd, and there's no agent in ~/Library/LaunchAgents" left
    kill -TERM "$jawd4" 2> /dev/null
    within 10 gone "$jawd4"
    if leads "$app4"; then kill -TERM -- "-$app4"; else kill -TERM "$app4" 2> /dev/null; fi
    within 5 gone "$app4"
fi

nothing() {
    [ -z "$(pgrep -f "$scratch/")" ]
}
claim "nothing the check started is still running" nothing
exit "$failed"
```

- [ ] **Step 2: Run it to see it fail**

With jawd copied (see the Global Constraints), build the app in Release without the hardened runtime on the build Mac, and run the check there, the two joined with `&&` so that a Release build that fails never runs the check on the app an earlier build left there:

```sh
mac/scripts/build-mac build jaw -configuration Release CODE_SIGN_IDENTITY=- ENABLE_HARDENED_RUNTIME=NO \
    && built=$(mac/scripts/built-from) && from=$(echo "$built" | sed -n 1p) && at=$(echo "$built" | sed -n 2p) \
    && main=$(GIT_OPTIONAL_LOCKS=0 git -C /Users/pedrogomes/src/jaw-mac-m1 rev-parse --short HEAD) \
    && ssh -o BatchMode=yes build-mac "JAW_BUILT_FROM=\"$from\" JAW_BUILT_AT=\"$at\" JAW_MAIN_AT=\"$main\" sh -s" < mac/build/start/try-start.sh
```

Expected: the build exits 0 with `** BUILD SUCCEEDED **`. The check prints its first two claims `ok` and exits 1 after about two minutes, most of them spent waiting for hellos that never come. This Jaw doesn't start jawd yet (its `ensure()` points at Jaw › jawd at Login…) and has no `shown at` lines, so every claim that jawd started, stopped, answered, was drawn starting or was started before `SMAppService` was asked prints `FAIL`, as does run 3's claim about the window's words. The claims about what it does already print `ok`: each app ran, opened through LaunchServices; no engine ran; run 1's app quit by itself after 30 s; run 2's app led a process group of its own; run 3 started nothing and made no jaw folder; the check could make the jaw folder for run 4; nothing is in launchd; and nothing is left running. If `open` itself prints an error over ssh, stop and report what it printed, running nothing more: nothing has opened Jaw this way on the build Mac yet. If `it runs` fails for an app, it didn't launch: read the newest `Jaw-*.ips` in `~/Library/Logs/DiagnosticReports` on the build Mac and say what it says in the report. If a dialog shows on the build Mac's screen, nothing answers it and the claims that wait on it fail: say so in the report.

- [ ] **Step 3: Trace what the window shows**

In `mac/JawApp/Sources/AppModel.swift`, make four changes.

After `let trace = ProcessInfo.processInfo.environment["JAW_TRACE"] != nil`, add:

```swift
    /// What the trace last said the window shows of jawd.
    var traced: String?
```

`start()`'s last lines:

```swift
        preferences.changed.append { [weak self] in self?.restyle() }
        glance.model = self
    }
```

become, so the first frame says what the app decided at launch rather than what the window said as it was made, before `start()` ran:

```swift
        preferences.changed.append { [weak self] in self?.restyle() }
        glance.model = self
        window?.refresh()
        if trace { DispatchQueue.main.async { [weak self] in self?.traceShown() } }
    }
```

In `drain()`, the loop and its end:

```swift
        for update in updates {
            if trace { FileHandle.standardError.write(Data("jaw-mac: \(String(describing: update).prefix(160))\n".utf8)) }
            apply(update)
        }
        window?.refresh()
    }
```

become, so each drained update's line ends with when it was drained, which for jaw-client's failed tries is at one of the window's display frames, and the trace then says what that frame shows:

```swift
        for update in updates {
            if trace { FileHandle.standardError.write(Data("jaw-mac: \(String(describing: update).prefix(160)) at \(Self.sinceStart) ms\n".utf8)) }
            apply(update)
        }
        window?.refresh()
        traceShown()
    }
```

After `stateView`, in "What the screen says", add:

```swift
    /// With `JAW_TRACE`, says on standard error what the window shows of jawd at launch and at each
    /// update it drains, once a frame with it is drawn, with the time since the process started: the
    /// status line's state, what fills the panes' place by its case, jawd's words when it didn't
    /// start, and whether the window is hidden, when any of that has changed.
    func traceShown() {
        guard trace else { return }
        let place = stateView.map { kind in String("\(kind)".prefix { $0 != "(" }) } ?? "chats"
        let why = if case let .starting(failed?) = reach { " · \(failed)" } else { "" }
        let hidden = window?.window?.isVisible == true ? "" : " · window hidden"
        let shows = "\(situation.stateLine?.text ?? "online") · \(place)\(why)\(hidden)"
        guard shows != traced else { return }
        traced = shows
        Bench.nextFrame {
            FileHandle.standardError.write(Data("jaw-mac: shown at \(Self.sinceStart) ms: \(shows)\n".utf8))
        }
    }

    /// Milliseconds since the process started, as the trace counts them.
    static var sinceStart: Int {
        Int(Date().timeIntervalSince(Launch.processStart() ?? Date()) * 1000)
    }
```

- [ ] **Step 4: Build, and run the check again**

Run: `mac/scripts/build-mac build`, then `make check-parity`.
Expected: each exits 0.

Then run the Release build and the check exactly as in Step 2.

Expected: the same claims print `ok` and `FAIL` as in Step 2, and the check exits 1, but each run now prints its frames, indented under its banner. The first, a few hundred milliseconds in, is `shown at <ms> ms: jawd didn't start · starting · nothing answers on <its folder>/jaw.sock: Jaw › jawd at Login… starts Jaw's own, jaw agent install the terminal's, and --demo a made-up account`. Frames of jaw-client's words follow it from the client's first try on, since main's `.unreachable` case still puts them in place of the refusal: `shown at <ms> ms: can't reach jawd · trying again in 0 s · asleep`, then the same with `1 s` and with `2 s` as the client's wait grows, about four frames in all; in run 3, which reads its trace about 3 s in, the `2 s` one may not have come yet. That's the trace the check reads, saying what this Jaw still says. Task 5's starter ends the second kind: it keeps the window's `jawd didn't start` until a jawd answers, whatever jaw-client says meanwhile.

- [ ] **Step 5: Commit**

```bash
git add mac/JawApp/Sources/AppModel.swift
git commit -F- <<'EOF'
JAW_TRACE says what the window shows of jawd, the status line's state, what fills the panes' place and jawd's words when it didn't start, each time that or the window's visibility changes, once a frame with it is on screen, with the time since the process started, which each update it drains now ends with too, and the window draws what the app decided at launch from its first frame; parity step 1.2's check, written to read it, ran on the build Mac's build user with a Release Jaw built without the hardened runtime around the Mac builder's jawd and no engine, and now prints each run's frames while failing, as it should until the app starts jawd, on every claim that it does
EOF
```

The check stays in `mac/build/start/`, ignored, for Task 5.

---

### Task 5: The app starts jawd, and jawd at Login… goes

Nothing in the app can stop a jawd it started for a release: `Agent.stop()` stops only `--demo`'s, so the app's quit, a crash and a killed process group all leave that jawd running, until `--for-apps` (the last section). Task 4's check is this task's failing test; Step 8 runs it again.

**Files:**
- Modify: `mac/JawApp/Sources/Agent.swift` (rewritten); `mac/JawApp/Sources/AppModel.swift` (`start()`'s first lines, main's 84-89, two lines lower after Task 4, and `apply(_:)`'s `.hello` at main's 130 and `.unreachable` at main's 137-144, five lines lower); `mac/JawApp/Sources/AppDelegate.swift:53-76`; `mac/JawApp/Sources/Commands.swift:21-22`; `mac/scripts/bundle` (rewritten)
- Delete: `mac/JawApp/Helpers/dev.badcoffee.jaw.jawd.plist`

**Interfaces:**
- Consumes: `Spawn`, `Spawn.said(_:)`, `Spawn.lastWords(_:)` (Task 1); `Build(info:)` (Task 2); `Starter`, `Starter.Here`, `Step`, `Shown`, `launched(answers:now:)`, `unregistered(removed:answers:now:)`, `unreachable(now:)`, `exited(status:said:now:)`, `waited(answers:now:)`, `didntStart(_:)`, `answered()` and `Starter.same(_:_:)` (Task 3); `AppModel.traceShown()`, `AppModel.sinceStart` and the check (Task 4).
- Produces, in the app: `Agent.launched() -> Starter.Shown`, `Agent.unreachable() -> Starter.Shown`, `Agent.answered()`, `Agent.stop()`, `Agent.say: (String, Bool) -> Void`, `Agent.helpers`; `Situation.Reach.init(_ shown: Starter.Shown, why: String = "", retry: Int = 0)`.
- Produces, with `JAW_TRACE=1`, these lines on standard error beside each drained update's: `jaw-mac: started jawd <pid>`, with `, in a process group of its own` for a release's, or `jaw-mac: can't start jawd: <why>`; `jaw-mac: jawd <pid> stopped (<exit status: N, or signal: N (SIGNAME)>)`, with `: <its last words>` when it said any; `jaw-mac: unregistering an earlier Jaw's agent at <ms> ms, which SMAppService says is <its status>`, the time since the process started; and `jaw-mac: unregistered <n> ms after asking: done`, or what `SMAppService` answered.

- [ ] **Step 1: See what goes**

Run: `rg -n 'jawdAtLogin|takeOver|registerAgent|func newer|func order|JawdVersion|jawd at Login|Contents/Library/LaunchAgents' mac/JawApp mac/scripts`
Expected: lines in `AppDelegate.swift`, `Agent.swift`, `Commands.swift`, `mac/scripts/bundle` and the agent's plist. The check's claims about starting jawd, which failed at Task 4's Step 4, are what this task makes hold.

- [ ] **Step 2: The agent carries out the starter's steps**

`mac/JawApp/Sources/Agent.swift` becomes:

```swift
import AppKit
import Foundation
import JawKit
import JawUI
import ServiceManagement

/// Who starts jawd, as JawKit's `Starter` decides it: a release starts the jawd inside it when nothing
/// answers, in a process group of its own so it goes on when the app quits, and on jaw's own folder
/// then asks `SMAppService` to unregister the LaunchAgent an earlier Jaw registered; a debug build,
/// and `--demo`, start only the demo's jawd, on a folder of its own, and stop it when the app quits.
/// There's no jawd at login: the owner decided on 29 October that jawd runs while an app does.
@MainActor
final class Agent {
    /// The label an earlier Jaw registered its jawd under, as `jaw agent install` does.
    static let label = "dev.badcoffee.jaw.jawd"

    let launch: Launch
    var starter: Starter
    /// Says something in the status line, as the model does.
    var say: (String, Bool) -> Void = { _, _ in }
    /// The jawd this app started for `--demo`, stopped when the app quits.
    var demo: pid_t?
    /// `JAW_TRACE=1` says each jawd started and stopped, and when `SMAppService` was asked and what it answered.
    let trace = ProcessInfo.processInfo.environment["JAW_TRACE"] != nil

    init(launch: Launch) {
        self.launch = launch
        #if DEBUG
            let debug = true
        #else
            let debug = false
        #endif
        starter = Starter(Starter.Here(
            debug: debug,
            demo: launch.demo,
            home: launch.home,
            jawsOwn: Starter.same(launch.home, Launch.platformHome),
            account: FileManager.default.fileExists(atPath: (launch.home as NSString).appendingPathComponent("session.db")),
            build: Build(info: Bundle.main.infoDictionary ?? [:]),
            jawd: launch.demo ? Self.jawd : Self.helpers,
            environment: ProcessInfo.processInfo.environment,
            terminalAgent: FileManager.default.fileExists(atPath: Self.terminalAgent),
        ))
    }

    /// The jawd inside the app, the only one a release starts by itself.
    static var helpers: String? {
        let inside = Bundle.main.bundleURL.appending(path: "Contents/Helpers/jawd").path
        return FileManager.default.isExecutableFile(atPath: inside) ? inside : nil
    }

    /// The jawd `--demo` runs: the one inside the app, `JAWD`'s, or, building from source, the one
    /// cargo built.
    static var jawd: String? {
        if let helpers = Self.helpers { return helpers }
        if let env = ProcessInfo.processInfo.environment["JAWD"], FileManager.default.isExecutableFile(atPath: env) {
            return env
        }
        #if DEBUG
            let repo = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
                .deletingLastPathComponent().deletingLastPathComponent()
            for profile in ["debug", "release"] {
                let built = repo.appending(path: "target/\(profile)/jawd").path
                if FileManager.default.isExecutableFile(atPath: built) { return built }
            }
        #endif
        return nil
    }

    /// The plist `jaw agent install` writes, if the terminal installed its own agent.
    static var terminalAgent: String {
        (NSHomeDirectory() as NSString).appendingPathComponent("Library/LaunchAgents/\(label).plist")
    }

    static func answers(_ socket: String) -> Bool {
        let fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        defer { Darwin.close(fd) }
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(socket.utf8)
        guard bytes.count < MemoryLayout.size(ofValue: addr.sun_path) else { return false }
        withUnsafeMutableBytes(of: &addr.sun_path) { raw in
            raw.copyBytes(from: bytes)
            raw[bytes.count] = 0
        }
        let size = socklen_t(MemoryLayout<sockaddr_un>.size)
        return withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.connect(fd, $0, size) }
        } == 0
    }

    /// At launch: what the starter does first, and what the window says until jawd's first word.
    func launched() -> Starter.Shown {
        let (steps, shown) = starter.launched(answers: Self.answers(launch.socket), now: Date())
        run(steps)
        return shown
    }

    /// jaw-client couldn't reach jawd: what the window says.
    func unreachable() -> Starter.Shown {
        starter.unreachable(now: Date())
    }

    /// jawd said hello.
    func answered() {
        starter.answered()
    }

    func run(_ steps: [Starter.Step]) {
        for step in steps {
            switch step {
            case let .say(text, warn): say(text, warn)
            case .unregister: unregister()
            case let .start(spawn): start(spawn)
            case let .wait(seconds): DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [weak self] in self?.waited() }
            }
        }
    }

    /// The wait the starter asked for is over: on the main queue's own timer, so it ends whether or not
    /// the window is drawing.
    func waited() {
        run(starter.waited(answers: Self.answers(launch.socket), now: Date()))
    }

    func start(_ spawn: Spawn) {
        do {
            let pid = try spawn.run { [weak self] pid, status in self?.exited(pid, status, log: spawn.log) }
            if !spawn.ownGroup { demo = pid }
            note("started jawd \(pid)\(spawn.ownGroup ? ", in a process group of its own" : "")")
        } catch {
            let why = "can't start jawd: \(error.localizedDescription)"
            note(why)
            starter.didntStart(why)
        }
    }

    func exited(_ pid: pid_t, _ status: Int32, log: String) {
        if demo == pid { demo = nil }
        let words = Spawn.lastWords(log)
        note("jawd \(pid) stopped (\(Spawn.said(status)))\(words.isEmpty ? "" : ": \(words)")")
        run(starter.exited(status: status, said: words, now: Date()))
    }

    /// Asks `SMAppService` to unregister the agent an earlier Jaw registered, which stops that agent's
    /// jawd before it answers, and tells the starter; an error is there being none it could find.
    /// `SMAppService` calls the handler on libdispatch's default queue, so it's `@Sendable`, never the
    /// main actor's, and hops to the main queue itself.
    func unregister() {
        let service = SMAppService.agent(plistName: "\(Self.label).plist")
        let asked = Date()
        if trace {
            note("unregistering an earlier Jaw's agent at \(AppModel.sinceStart) ms, which SMAppService says is \(Self.said(service.status))")
        }
        service.unregister { @Sendable error in
            let removed = error == nil
            let answer = error.map { "\(($0 as NSError).domain) \(($0 as NSError).code): \($0.localizedDescription)" } ?? "done"
            let took = Int(Date().timeIntervalSince(asked) * 1000)
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    self.note("unregistered \(took) ms after asking: \(answer)")
                    self.run(self.starter.unregistered(removed: removed, answers: Self.answers(self.launch.socket), now: Date()))
                }
            }
        }
    }

    /// An agent's status as the trace says it.
    static func said(_ status: SMAppService.Status) -> String {
        switch status {
        case .notRegistered: "not registered"
        case .enabled: "enabled"
        case .requiresApproval: "waiting for approval"
        case .notFound: "not found"
        @unknown default: "status \(status.rawValue)"
        }
    }

    /// With `JAW_TRACE`, says it on standard error.
    func note(_ text: String) {
        if trace { FileHandle.standardError.write(Data("jaw-mac: \(text)\n".utf8)) }
    }

    /// Stops the jawd the app started for `--demo`, once, giving it a few seconds to go, so its folder
    /// is still by the time the app has quit: a snapshot's folder is removed then. A release's jawd
    /// goes on, as the terminal's does.
    func stop() {
        guard let demo else { return }
        self.demo = nil
        kill(demo, SIGTERM)
        let until = Date().addingTimeInterval(5)
        while kill(demo, 0) == 0, Date() < until {
            usleep(10000)
        }
    }
}

extension Situation.Reach {
    /// What the window says of jawd, from what the starter says and jaw-client's words.
    init(_ shown: Starter.Shown, why: String = "", retry: Int = 0) {
        switch shown {
        case .waiting: self = .waiting
        case .starting: self = .starting(failed: nil)
        case let .failed(said): self = .starting(failed: said)
        case .unreachable: self = .unreachable(why: why, retry: retry)
        }
    }
}
```

- [ ] **Step 3: The model shows what the agent says**

In `mac/JawApp/Sources/AppModel.swift`, make three changes.

`start()`'s first lines:

```swift
    func start() {
        if let why = agent.ensure() {
            reach = .starting(failed: why)
        } else if agent.started != nil {
            reach = .starting(failed: nil)
        }
```

become:

```swift
    func start() {
        agent.say = { [weak self] text, warn in self?.say(text, warn: warn) }
        reach = Reach(agent.launched())
```

In `apply(_:)`, the `.hello` case's first lines:

```swift
        case let .hello(w, me):
            welcome = w
```

become:

```swift
        case let .hello(w, me):
            agent.answered()
            welcome = w
```

and the `.unreachable` case:

```swift
        case let .unreachable(why, retryIn):
            if let stopped = agent.stopped {
                reach = .starting(failed: stopped)
            } else if agent.starting {
                reach = .starting(failed: nil)
            } else {
                reach = .unreachable(why: why, retry: Int(retryIn / .seconds(1)))
            }
```

becomes:

```swift
        case let .unreachable(why, retryIn):
            reach = Reach(agent.unreachable(), why: why, retry: Int(retryIn / .seconds(1)))
```

`stop()` doesn't change: it ends with `agent.stop()`, which stops only `--demo`'s jawd now, and only once, so `Bench.finish`'s second call after a quit held back signals nothing.

- [ ] **Step 4: jawd at Login… goes**

In `mac/JawApp/Sources/AppDelegate.swift`, delete lines 53 to 76: the blank line before `// MARK: - Asked from a menu`, the mark, and `jawdAtLogin(_:)` whole. The file then ends:

```swift
    func applicationShouldHandleReopen(_: NSApplication, hasVisibleWindows: Bool) -> Bool {
        if !hasVisibleWindows { window.showWindow(nil) }
        return true
    }
}
```

In `mac/JawApp/Sources/Commands.swift`, delete these two lines, so the Jaw menu's second group holds Settings… alone:

```swift
        add(Command("agent", "jawd at Login…", phrase: "jawd at login", detail: "start jawd when you log in", place: .app, group: 1)
            { AppDelegate.shared?.jawdAtLogin(nil) })
```

- [ ] **Step 5: The bundle loses its LaunchAgent**

Run: `git rm mac/JawApp/Helpers/dev.badcoffee.jaw.jawd.plist`

`mac/scripts/bundle` becomes:

```sh
#!/bin/sh
# `make mac`: Jaw.app with jawd, jaw-engine and the terminal inside it, each signed, saying what it
# was built from.
#
# jawd and jaw are built for release here, since the build Mac has no Rust or Go, jawd with its
# Info.plist, which its own build embeds, so macOS asks for the microphone in jaw's words, and
# jaw-engine by make engine, which stamps it with the version jaw and jawd say; the app is built in
# Release on the build Mac (here when it doesn't answer) and comes back to mac/build/release/Jaw.app. The three
# helpers go into Contents/Helpers, and what mac/scripts/built-from says into its Info.plist
# (JawBuiltFrom and JawBuiltAt), since only a Jaw from origin/main or a release's tag on it starts its
# jawd on an account's folder; everything is signed from the inside out with the hardened runtime:
# with JAW_SIGN_IDENTITY, else the team's Developer ID Application identity by its SHA-1, else ad hoc,
# which doesn't open: under the hardened runtime, library validation refuses an app signed ad hoc its
# own frameworks.
set -eu
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
team="${JAW_TEAM_ID:-3JP75Z3F98}"
out="mac/build/release"
app="$out/Jaw.app"
# Made before the first reading, so the second sees anything jawd is built from written since, a
# change made and undone while it builds included.
stamp="$(mktemp "${TMPDIR:-/tmp}/jaw-bundle.XXXXXX")"
trap 'rm -f "$stamp"' EXIT
built="$(mac/scripts/built-from)"
# The helpers are copied from this checkout's target, whatever target-dir a setting of Cargo's names,
# and go builds jaw-engine from engine/go.mod alone, whatever go.work a folder above it holds.
export CARGO_TARGET_DIR="$repo/target" GOWORK=off
# jawd and jaw an earlier build left go first, so the copy fails rather than sign them under this
# tree's record when a target a setting of Cargo's names has cargo build into target/<triple>.
rm -f target/release/jawd target/release/jaw

echo "==> jawd, jaw and jaw-engine for release"
cargo build --release -p jawd --bin jawd --quiet
cargo build --release -p jaw --bin jaw --quiet
make -s engine

echo "==> Jaw.app in Release"
rm -rf "$app"
mkdir -p "$out"
if ssh -o BatchMode=yes -o ConnectTimeout=5 "${JAW_BUILD_MAC:-build-mac}" true 2>/dev/null; then
    mac/scripts/build-mac release "$out"
else
    echo "==> the build Mac doesn't answer; building here"
    (cd mac && mise exec -- tuist generate --no-open >/dev/null &&
        xcodebuild build -workspace Jaw.xcworkspace -scheme jaw -configuration Release \
            -destination 'platform=macOS,arch=arm64' -derivedDataPath build/DerivedData-release -quiet)
    ditto mac/build/DerivedData-release/Build/Products/Release/Jaw.app "$app"
fi

echo "==> the helpers, and what it was built from"
mkdir -p "$app/Contents/Helpers"
cp target/release/jawd target/release/jaw target/engine/jaw-engine "$app/Contents/Helpers/"
jawd_version="$("$app/Contents/Helpers/jawd" --version | awk '{print $2}')"
again="$(mac/scripts/built-from "$repo" "$stamp")"
if [ "$again" != "$built" ]; then
    echo "==> the checkout changed while make mac built it: run it again" >&2
    exit 1
fi
from="$(echo "$built" | sed -n 1p)"
at="$(echo "$built" | sed -n 2p)"
plutil -replace JawBuiltFrom -string "$from" "$app/Contents/Info.plist"
plutil -replace JawBuiltAt -string "$at" "$app/Contents/Info.plist"

identity="${JAW_SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null |
    awk -v team="($team)\"" '/"Developer ID Application:/ && index($0, team) { print $2; exit }')}"
if [ -z "$identity" ]; then
    identity="-"
    echo "==> no Developer ID Application identity for team $team: signing ad hoc, so the app won't open under the hardened runtime"
fi
flags="--force --options runtime --timestamp"
[ "$identity" = "-" ] && flags="--force --options runtime"
echo "==> signing with $identity"
# Inside out: the helpers, the frameworks, then the app, keeping the app's own entitlements.
codesign $flags --sign "$identity" --entitlements mac/JawApp/Helpers/jawd.entitlements "$app/Contents/Helpers/jawd"
codesign $flags --sign "$identity" "$app/Contents/Helpers/jaw-engine" "$app/Contents/Helpers/jaw"
for framework in "$app"/Contents/Frameworks/*.framework; do
    [ -e "$framework" ] && codesign $flags --sign "$identity" "$framework"
done
codesign $flags --sign "$identity" --preserve-metadata=entitlements,requirements,flags "$app"
codesign --verify --deep --strict "$app"
echo "==> $app ($(du -sh "$app" | cut -f1)), jawd $jawd_version, built from $at"
```

`JawdVersion` goes with the takeover, its one reader; the last line still says jawd's version. Everything else is the bundle as Task 2 and main left it: built-from read before the builds, after the stamp, and again after them, the keys written only when the two say the same; cargo building into the checkout's own `target` and go reading no `go.work`; and `cargo build` for jawd, whose own build embeds its Info.plist since b8ee0816.

- [ ] **Step 6: See it gone**

Run: `rg -n 'jawdAtLogin|takeOver|registerAgent|func newer|func order|JawdVersion|jawd at Login|Contents/Library/LaunchAgents' mac/JawApp mac/scripts; echo "rg: $?"` and `sh -n mac/scripts/bundle && echo parsed`
Expected: `rg: 1` with nothing listed before it, then `parsed`.

- [ ] **Step 7: Build and test**

Run: `mac/scripts/build-mac build`, then `mac/scripts/build-mac test JawKit`, `mac/scripts/build-mac test JawUITests` and `mac/scripts/build-mac test JawHarnessTests`; then `make check-parity`.
Expected: each exits 0.

A debug build asserts as it launches that no command is listed twice and no key belongs to two commands or to one without a menu (`Menus.problems`, in `applicationDidFinishLaunching`), so the snapshot, which launches one, also shows the menus hold together without the agent command.

Run: `mac/scripts/build-mac snapshot mac/build/start/demo.png`
Expected: `{"panes":1.0,"snapshot":1.0}` among its last lines, and `mac/build/start/demo.png`, which you look at with the Read tool, shows the demo's chats with one open. `--demo`'s jawd started through `Spawn`, served the window, and was stopped by `Agent.stop()` before `build-mac` removed its folder.

- [ ] **Step 8: The check again**

Run the Release build and the check exactly as in Task 4's Step 2.

Expected: the build exits 0, and the check exits 0 in about a minute, printing only `ok` claims, each run's frames indented under its banner, and in run 4 what `SMAppService` was asked and answered. With each run's values in place of the angle brackets:

```
ok: the build user has no jaw folder, no terminal agent and nothing of jaw's in launchd, launchd gives apps no PATH, JAW_ENGINE or JAW_HOME, and no engine is on their PATH
==> the check's Jaw: the Release app without the hardened runtime, with the Mac builder's jawd inside and no engine, built from mac-start-jawd at <hash>, not on origin/main
ok: it's signed whole, without the hardened runtime, with no engine and no LaunchAgent
==> 1: nothing answers on a folder of its own, so it starts jawd; quitting leaves jawd running
    shown at <ms> ms: starting jawd… · starting
    shown at <ms> ms: not linked · linking
ok: it runs (<pid>), opened through LaunchServices: its parent is launchd
ok: it started jawd (<pid>), which answered on the folder's socket
ok: that jawd leads a process group of its own
ok: no engine ran
ok: every frame had the window visible, any before jawd answered saying starting jawd, then its state (sl, the first at <ms> ms)
ok: the app quit by itself once its snapshot gave up, through NSApp.terminate, as Quit Jaw does
ok: jawd goes on, and answers, with the app gone
==> 2: quitting jawd and opening the app brings it back, here while another process holds jaw.lock until 3 s after the app's first jawd was refused
ok: jawd stopped when told to, and nothing answers
    shown at <ms> ms: starting jawd… · starting
    shown at <ms> ms: not linked · linking
ok: it runs (<pid>), opened through LaunchServices: its parent is launchd
ok: while the lock was held, each jawd it started stopped saying so in jawd.log beside the socket, and once the lock was let go it started another
ok: the last one (<pid>) answered
ok: that jawd leads a process group of its own
ok: the window said starting jawd from its first frame, at <ms> ms, to jawd's answer, at <ms> ms, drawing all the while: <n> of jaw-client's tries were drained at its frames, with at most <ms> ms between drains, then jawd's state (sl)
ok: the app leads a process group of its own
ok: killing the app's process group took the app and left jawd running and answering
==> 3: a Jaw built from a branch starts no jawd on jaw's own folder
    shown at <ms> ms: jawd didn't start · starting · this Jaw was built from mac-start-jawd at <hash>, not on origin/main, so it doesn't start jawd on jaw's own folder or another account's, …
ok: it runs (<pid>), opened through LaunchServices: its parent is launchd
ok: the window says jawd didn't start, and why, from its first frame: this Jaw was built from mac-start-jawd at <hash>, not on origin/main (f)
ok: it started no jawd, asked SMAppService nothing and made no jaw folder
==> 4: a Jaw recorded as main's starts jawd on jaw's own folder at once, and only then asks SMAppService about an earlier Jaw's agent
ok: the build user still has no jaw folder, so the check makes it now, its own until the check ends
    unregistering an earlier Jaw's agent at <ms> ms, which SMAppService says is <its status>
    unregistered <n> ms after asking: <what it answered>
    shown at <ms> ms: starting jawd… · starting
    shown at <ms> ms: not linked · linking
ok: it runs (<pid>), opened through LaunchServices: its parent is launchd
ok: it started jawd, then asked SMAppService to unregister an earlier Jaw's agent, and the answer came after: nothing waited on it
ok: that jawd (<pid>) answered on jaw's own folder
ok: that jawd leads a process group of its own
ok: no engine ran
ok: the window never said connecting to jawd: any frame before jawd answered said starting jawd, then jawd's state (sl)
ok: nothing of jaw's is in launchd, and there's no agent in ~/Library/LaunchAgents
ok: nothing the check started is still running
```

How to read it:

- **The frames.** Each `shown at` line is printed once a frame with it is on screen, so its time is when the window had drawn it, and a change of the window's visibility prints one too. Run 2's frames say starting jawd for more than two seconds, since the lock is held until 3 s after the app's first jawd was refused, and the app starts its jawd again each second on its own timer meanwhile. What the window says doesn't change in that time, so the drains show the drawing: each of jaw-client's failed tries is drained at the window's next display frame, and its line says when, so tries that come at most 2 s apart, drained at most 2.5 s apart, mean the window's frames kept coming while jawd started. Run 1's and run 4's first frame may already be `not linked` if jawd answered before the window's first frame; `s*l+` allows that, and run 2 is the one that shows the wait. No run says connecting to jawd: the window draws what the app decided from its first frame, and run 4's app starts jawd before it asks `SMAppService` anything. A second `not linked` line, if one comes, is the status line or the panes' place changing once jawd has answered, which the `+` allows.
- **What `SMAppService` said, and when.** With no earlier agent on the build user and no plist in the bundle, its answer is an error, which the app reads as nothing to unregister. Note in the report its domain and code, the `unregistering` line's time beside the first frame's, and how many milliseconds the answer took. It can't answer question 2: with nothing registered, it says nothing of whether a registration would be found with the plist gone. If it says `done`, something was registered: say so.
- **The quits.** Run 1's app quits through `NSApp.terminate`, so `applicationWillTerminate` ran `Agent.stop()`, which left the release's jawd alone. Run 2's app is killed with its whole process group, as a crash or a terminal's Ctrl-C would take it, and its jawd, in a group of its own, goes on.

If the build Mac has no `python3`, the check says so on its first use; stop and say so in the report rather than running anything here. If a claim fails, fix the code, rebuild in Release without the hardened runtime, and run the check again; never run it on this Mac.

- [ ] **Step 9: Commit**

```bash
git add mac/JawApp/Sources/Agent.swift mac/JawApp/Sources/AppModel.swift mac/JawApp/Sources/AppDelegate.swift mac/JawApp/Sources/Commands.swift \
    mac/scripts/bundle
git commit -F- <<'EOF'
A Mac release starts the jawd inside it when nothing answers, as the terminal does, by the owner's decisions of 29 October as he chose on 30 October: in a process group of its own so it goes on when the app quits, its output appended to jawd.log beside the socket, and the window drawn at once saying Starting jawd until jawd answers, Starter deciding and Agent carrying it out; on an account's folder only a Jaw that make mac built from origin/main or a release's tag on it starts one, and on jaw's own folder it then asks SMAppService, through a Sendable handler and without waiting on it, to unregister the agent an earlier Jaw registered, unless the terminal's agent holds the label, which it names with jaw agent uninstall; Jaw › jawd at Login…, its prompts and the takeover go, with git describe's order they used, and so do the bundle's LaunchAgent and JawdVersion; JAW_TRACE says each jawd started and stopped, and when SMAppService was asked and how long it took to answer; tried on the build Mac's build user with a release Jaw built without the hardened runtime around the Mac builder's jawd and no engine: it started jawd on a folder of its own and on jaw's own folder, the window saying so in every frame and never waiting on SMAppService, jawd went on after the app quit and after its process group was killed, quitting jawd and opening the app brought it back while another process held jaw.lock, and a Jaw built from a branch started none on jaw's own folder
EOF
```

`git rm` in Step 5 has already staged the plist's deletion, so this commit carries it.

**Amended while it was built.** Its review (5 November) found the code above the plan's, byte for byte where the steps give it, and the check's 29 claims `ok` in each of two runs on the build Mac, with the frames Step 8 expects, `sl` in runs 1, 2 and 4 and `f` in run 3, and run 2's widest gap between drains 1620 ms, under its 2500. It also found what "After this plan" had put off until jawd records audio: a jawd that a release Jaw starts with `posix_spawn` has Jaw as its responsible process for macOS's privacy checks, whatever its process group, and keeps it after Jaw quits, where a release's jawd was until now the LaunchAgent's, its own responsible process, or the terminal's, which asks in the terminal's name. So from this task, whenever a release has started it, the jawd every interface uses asks for the microphone in Jaw's name, and Jaw.app had neither `NSMicrophoneUsageDescription` nor, under the hardened runtime, the `com.apple.security.device.audio-input` entitlement, while jawd has recorded voice messages since 2 November (4d96382d), which the terminal asks for with `voice.record`: recording one through that jawd would most likely be refused, or jawd killed, which takes it from every interface, since the app doesn't start jawd again once one has answered. It left three Minors for the branch review: a jawd that can't be started at all is shown starting for a frame, until jaw-client's first drained try, since `launched()` returns what the starter decided before it runs the steps; the root README still offers Jaw › jawd at Login…, and Task 6 doesn't touch it; and with `JAW_TRACE` the launch reads `SMAppService`'s status on the main thread before the first frame. And it left for the owner's trial what the check's bundle, unhardened, signed ad hoc and without an engine, can't show, among them an earlier Jaw's agent registered and then unregistered with its plist gone, the hardened runtime, his Developer ID and the engine under a jawd the app started, and main's newest jawd. On 5 November the owner chose to add both now, in a commit of their own, the microphone's, just before this amendment: `NSMicrophoneUsageDescription`, "Jaw records the voice messages you send.", jawd's own words but for Jaw's name, beside the contacts one in the app's Info.plist (`mac/Project.swift`), and `com.apple.security.device.audio-input` in `mac/JawApp/Jaw.entitlements`, which `make mac` keeps when it signs the app (`--preserve-metadata=entitlements`). The Debug and Release products built on the build Mac carry both, as `plutil` and `codesign` read them, and his trial records a voice message through a jawd a release Jaw started. jawd's own hint when the microphone is refused, "allow jawd in System Settings › Privacy & Security › Microphone" (`crates/jaw-voice/src/system.rs:50`), names the wrong entry for a jawd Jaw started, whose grant is Jaw's; it's left for jawd's builders, since this branch builds no Rust.

---

### Task 6: The step in the docs

**Files:**
- Modify: `mac/README.md` (lines 16, 22 and 24), `docs/plans/2026-10-27-jaw-mac-parity-plan.md` (step 1.2, lines 42-46, and the working rules' first bullet, line 185), `docs/plans/2026-10-26-jaw-mac-design.md` (the Decisions' line 18, the bundle at 84, "Who starts jawd" at 102-108, the "Starting jawd" state at 349), `docs/plans/2026-10-26-jaw-mac-plan.md` (lines 17, 25, 49 and 154)

**Interfaces:**
- Consumes: everything above, and the check's lines from Task 5's Step 8.
- Produces: the README, the parity plan, the Mac design and the Mac plan saying what the app does now and what follows.

- [ ] **Step 1: The README**

In `mac/README.md`, make three changes.

Line 16 becomes:

```sh
make mac                                    # Jaw.app with jawd, jaw-engine and jaw inside, signed, saying what it was built from
```

The paragraph at line 22, which begins "A debug build never opens the owner's jaw folder", becomes:

```markdown
A debug build never opens the owner's jaw folder: without `--home` or `JAW_HOME` it runs `jawd --engine demo`, a made-up account, in a folder of its own, and stops it on quitting; with either it starts no jawd, and says how to start one, and `--demo` never runs on jaw's own folder or on another holding a `session.db`. A release starts the jawd inside it when nothing answers on the folder's `jaw.sock`, as the terminal does: in a process group of its own, so it goes on when the app quits, its output appended to `jawd.log` beside the socket, the window saying "Starting jawd" until it answers, for 15 s, and a jawd that stopped because another held the folder started again a second later while nothing answers (`Starter` and `Spawn`, in JawKit). On an account's folder, jaw's own or one holding a `session.db`, it does that only when `make mac` built it from a commit on origin/main or a release's tag on one, with no uncommitted change to what jawd and the engine are built from, as `mac/scripts/built-from` says and the bundle records (`JawBuiltFrom`, `JawBuiltAt`), since a branch's jawd could move jaw.db to a version main's can't open; any other says so, and how to start one. On jaw's own folder it then asks `SMAppService` to unregister the LaunchAgent an earlier Jaw registered, without waiting on the answer, or, while the terminal's agent is installed, names it with `jaw agent uninstall`, which removes it, and asks nothing. Jaw no longer starts jawd at login (the owner, 29 October); once jawd has `--for-apps`, the jawd it starts will stop a short while after the last app has gone (30 s, a Default of the lifetime design awaiting the owner's review).
```

At the end of the options paragraph at line 24, add: ``With `JAW_TRACE=1` the app says on standard error each update it drains and when, what the window shows of jawd each time that or the window's visibility changes, once a frame with it is drawn, each jawd it starts and how it stopped, and when `SMAppService` was asked about an earlier agent and how long it took to answer.``

- [ ] **Step 2: The parity plan**

In `docs/plans/2026-10-27-jaw-mac-parity-plan.md`, step 1.2's body, from "The terminal starts jawd when nothing answers" to its "Done when" bullet, becomes the text below. The heading stays as it is, since `make check-parity` reads the steps' headings.

```markdown
The terminal starts jawd when nothing answers on `jaw.sock`, says so, and keeps drawing. Since [its plan](2026-10-30-jaw-mac-starting-jawd-plan.md), so does a Mac release, by the owner's decisions of 29 October in the jawd lifetime design (`docs/plans/2026-10-29-jaw-jawd-lifetime-design.md` on `jawd-lifetime`, its Defaults awaiting his review), as he chose on 30 October: "Now, by your 29 Oct decisions: start the bundled jawd when nothing answers, say Starting jawd, and remove jawd at Login… and the takeover; --for-apps and the app flag follow once jawd has them."

- **Done:** a release starts the jawd inside it (`Contents/Helpers/jawd`) when nothing answers, in a process group of its own so it goes on when the app quits, its output appended to `jawd.log` beside the socket, and draws at once, saying "Starting jawd" until jawd answers, for 15 s, as the terminal does; one that stopped because another jawd held the folder is started again a second later while nothing answers, so two interfaces starting jawd at once end with one. On an account's folder, jaw's own or one holding a `session.db`, it starts one only when `make mac` built it from a commit on origin/main or a release's tag on one, as the bundle records, so a build in progress never starts its jawd on the owner's folder; on jaw's own folder it then asks `SMAppService` to unregister the agent an earlier Jaw registered, never waiting on the answer, and names the terminal's agent with `jaw agent uninstall`. Jaw › jawd at Login…, its prompts and the takeover are gone, and so is the bundle's LaunchAgent. Debug builds keep `--demo` and `JAW_HOME`, and `--demo` never runs on an account's folder.
- **Tried** on the build Mac's build user, which is clean for jaw, with a release Jaw built without the hardened runtime around the Mac builder's jawd and no engine, so nothing reached WhatsApp: it started jawd on a folder of its own and on jaw's own folder, the window saying so in every frame until jawd answered, never waiting on `SMAppService`; jawd went on after the app quit and after the app's process group was killed; quitting jawd and opening the app brought it back while another process held the folder's lock; and a Jaw built from a branch started none on jaw's own folder. Not tried end to end: an earlier Jaw's agent actually registered and then unregistered, which no Mac we know has, and the hardened runtime, the Developer ID's signature and the engine under a jawd the app started, which the owner's trial of a `make mac` bundle from origin/main brings.
- **Next, once jawd's branch has their protocol text:** `--for-apps` and the hello's `app`, as the last section of the plan says.
- **Done when** on a clean user account, a release build starts jawd when nothing answers, quitting it stops jawd within 30 s (the lifetime design's Default, awaiting the owner's review), and opening it again brings jawd back, with the window drawn the whole time. All but the stop after quitting holds now.
```

In its working rules, the first bullet, at line 185, paraphrases the Mac plan's rule that Step 4 rewrites, so it follows it: its words "the real agent label is never registered on his Mac by a build in progress" become "no agent opens a release Jaw on his Mac, since one can start its jawd on his folder whenever his isn't answering".

- [ ] **Step 3: The Mac design**

In `docs/plans/2026-10-26-jaw-mac-design.md`, make four changes.

At the end of the Decisions' second bullet, at line 18, which ends "makes two jawds impossible.", add: ` The owner replaced its LaunchAgent and jawd at login on 29 October, deciding that jawd runs while an app does: the app starts the jawd inside it when nothing answers, as "Who starts jawd" says.`

In the bundle's layout under "How it fits together", delete the line `  Library/LaunchAgents/dev.badcoffee.jaw.jawd.plist   registered with SMAppService`.

"Who starts jawd"'s bullets, from "**jawd inside the app, as the one LaunchAgent.**" to "**Deleting the app takes its agent with it.**", become:

```markdown
- **The app starts the jawd inside it when nothing answers** (Owner, 29 and 30 Oct). At launch a release connects to `jaw.sock`; when nothing answers, it starts `Contents/Helpers/jawd` in a process group of its own, so it goes on when the app quits, its output appended to `jawd.log` beside the socket, as the terminal starts the jawd beside it (`wire.rs`, `start_jawd`), and jawd starts the engine beside it, as always. There's no LaunchAgent and no Jaw › jawd at Login…: the owner decided on 29 October that jawd runs while an app does, with no way to keep it running once every app has closed (the jawd lifetime design, `docs/plans/2026-10-29-jaw-jawd-lifetime-design.md` on `jawd-lifetime`), and chose on 30 October to build this half now. Once jawd has `--for-apps` and the hello's `app`, the app passes and says them, and jawd stops a short while after the last app has gone (30 s, the lifetime design's Default, awaiting his review).
- **One jawd, by `jaw.lock`.** jawd refuses a second jawd on a folder whoever starts it. A jawd the app started that stopped because another held the folder is started again a second later while nothing answers, so two interfaces starting jawd at once end with one. The terminal says why and waits instead; the app starts again because it starts its jawd while an earlier Jaw's agent's may still be stopping.
- **The app starts without waiting.** It draws its window at once, saying "Starting jawd" in the status line and in the panes' place until jawd answers, for 15 s, as the terminal says it; one that stopped as it started says why, from its log's last line.
- **Never a branch's jawd on an account's folder.** A release starts a jawd on `~/Library/Application Support/jaw`, or on any folder holding a `session.db`, only when `make mac` built it from a commit on origin/main or a release's tag on one, with no uncommitted change to what jawd and the engine are built from, as the bundle records (`JawBuiltFrom`, `JawBuiltAt`). Any other says so and how to start one, since a branch's jawd could move jaw.db to a version main's can't open. On any other folder, as `--home` or `JAW_HOME` give, a release starts the jawd inside it.
- **What earlier builds left.** At launch on jaw's own folder, such a release starts its jawd first and then has `SMAppService` unregister the agent an earlier Jaw registered, which stops that agent's jawd; when that jawd was the one answering, the app starts its own once `SMAppService` says the agent has gone. While `~/Library/LaunchAgents/dev.badcoffee.jaw.jawd.plist` is there, the app names the terminal's agent with `jaw agent uninstall`, which removes it, and leaves the label alone.
- **The terminal finds jawd beside it,** as `wire.rs`'s `jawd_binary()` already does, and the app's Settings offer to link `Contents/Helpers/jaw` onto the PATH, so `jaw` and `jawd` come from one build and always speak one protocol.
- **Its own folder for development.** The app honours `JAW_HOME`, as the terminal does, and `--demo` runs `jawd demo` in a folder of its own, as `jaw demo` does, never on jaw's own folder or another account's. A debug build never opens the owner's folder unless it's told to, and starts no jawd but `--demo`'s.
```

Under "Linking and the states", the "Starting jawd" bullet becomes:

```markdown
- **Starting jawd:** the window as it was, the status line and the panes' place saying "Starting jawd" while the jawd the app started starts, as the terminal says it.
```

Leave the Decisions' other bullets of 26 October, "Notifications" (whose "jawd's own once it's quit" holds until `--for-apps`, since the jawd a release starts goes on after the app quits), "Changes outside the app" and "The owner's answers" as they are: the lifetime design's own docs step changes those once it's approved.

- [ ] **Step 4: The Mac plan**

In `docs/plans/2026-10-26-jaw-mac-plan.md`, make four changes.

Line 17, the bullet that begins "**The real agent label is never registered on this Mac.**", becomes:

```markdown
- **No agent opens a release Jaw on this Mac.** No build registers a LaunchAgent since parity step 1.2, and none takes over the terminal's agent. A release that `make mac` built from origin/main or a release's tag on it starts the jawd inside it on the owner's folder whenever his isn't answering, and main's jawd can upgrade jaw.db past what his checkout's jawd opens; a release from a branch starts none there, but does on any folder with no `session.db`. So release builds are tried on the build Mac's build user, or with `--home` on a folder of their own there, and a debug build starts no jawd but `--demo`'s.
```

At the end of line 25, the bullet that ends "makes two jawds impossible. Mac-M1 step 3.", add: ` The owner replaced the LaunchAgent and jawd at login on 29 October, deciding that jawd runs while an app does, and parity step 1.2 took them out: a release starts the jawd inside it when nothing answers.`

Line 49's beginning, `3. **jawd at login.** The bundled jawd registered`, becomes `3. **jawd at login,** replaced in parity step 1.2 by the owner's decision of 29 October that jawd runs while an app does: there's no LaunchAgent and no takeover, and a release starts the jawd inside it when nothing answers. As first built: the bundled jawd registered`, and the rest of the line stays.

In line 154, ``General (jawd at login, the `jaw` command on the PATH)`` becomes ``General (the `jaw` command on the PATH; jawd at login went in parity step 1.2)``.

- [ ] **Step 5: Run the check**

Run: `make check-parity`
Expected: exit 0, its list as it was before this plan: step 1.2 has no rows, and its heading is unchanged.

- [ ] **Step 6: Commit**

```bash
git add mac/README.md docs/plans/2026-10-27-jaw-mac-parity-plan.md docs/plans/2026-10-26-jaw-mac-design.md docs/plans/2026-10-26-jaw-mac-plan.md
git commit -F- <<'EOF'
Parity step 1.2's first half is done: the parity plan says what a Mac release does now when nothing answers, what was tried on the build Mac and what wasn't, and that --for-apps and the hello's app follow once jawd's branch has their protocol text; the Mac design's Who starts jawd says the app starts the jawd inside it, by the owner's decisions of 29 and 30 October, with no LaunchAgent, never a branch's jawd on an account's folder, and what earlier builds left, its decision of 26 October points there, its bundle lists no LaunchAgent and its Starting jawd state lasts while jawd starts; the Mac plan's rules, and the parity plan's working rules after them, say no agent opens a release Jaw on the owner's Mac, and the Mac plan marks jawd at login replaced; the Mac README says how a release starts jawd, how a bundle records what it was built from, and what JAW_TRACE says; make check-parity passes, step 1.2 having no rows
EOF
```

- [ ] **Step 7: Clean up and report**

Run: `rm -rf mac/build/start`, then `make prune-builds`, then `git fetch origin && git log --oneline HEAD..origin/main`.
Report each task's runs, the check's lines from Task 5's Step 8 (with what `SMAppService` answered and how long it took, and the first frame's times), the picture looked at (`demo.png`), whether main moved, and what stays: the branch's copy on the build Mac (`~/src/build-mac/jaw/jaw-mac-start`, removed once the branch has landed) and `target/debug/jawd` in the worktree, which goes with it. End the report with this for the owner, in these words or near them: a Jaw that `make mac` made in a checkout or worktree at origin/main, opened while his own jawd is down, starts its jawd on his folder, which upgrades jaw.db from 17 to 20, backed up first as `jaw.db.before-v17`, past what his checkout's jawd opens; a Jaw made in his own checkout records `branch` while his own commit is in it, and starts none there; so before opening one from origin/main with his jawd down, he brings his checkout up to origin/main (`git pull`) and rebuilds his jawd, or starts his own jawd first.

Before writing that, recheck its two numbers, which were so at origin/main's 9b3fbdf4, on the evening of 30 October. 20 is how many migrations origin/main's jawd has: `git show origin/main:crates/jawd/src/store/migrations.lock | grep -c '^[0-9]'`. 17 is how many his checkout's has: the same, with the commit `git worktree list` shows for `/Users/pedrogomes/src/jaw` in place of `origin/main`, which reads his checkout without opening it. If either has moved, write them as they are, and the backup as `jaw.db.before-v<his number>`, which jawd makes only when a data step is among the migrations it runs; say in the report that they moved.

**Amended while it was built.** Its line numbers were 30 October's, so each place was found by its text. Two of its sentences for the parity plan weren't so, and now say what is: the check's bundle held main's jawd of 4 November, not the Mac builder's, and while the terminal's agent is installed the app names it instead of asking `SMAppService`, not as well. Beyond its text, the controller added what the owner chose while this plan was built: the Mac design's "Who starts jawd" also says the two departures from the terminal he kept on 4 November, the start he chose on 5 November when `SMAppService` says it removed an earlier Jaw's agent, and the microphone asked for in Jaw's name; the root README's line on how jawd runs, which still offered Jaw › jawd at Login…, says what a release does; and this plan's owner's trial records a voice message. Step 6 also commits `README.md` and this plan, and Step 7's clean-up and fetch were left to the controller, at landing, after the last rebase. Its review (5 November) found every change its steps ask for made, word for word but for those, and two texts that no longer held: the Mac design's Risks and the parity plan's step 2.7 still said macOS asks about jawd by its own signature, against the design's new bullet; and this plan's numbers, 17, 20 and `jaw.db.before-v17`, were 30 October's, where on 5 November origin/main and the owner's checkout had the same migrations. So the Risks bullet says whose name macOS asks in for each way jawd is started, and what the owner's trial still has to show; step 2.7 needs nothing more from jawd, what's left being the Mac's own recording; the Decision on a Jaw from origin/main gives its numbers as that day's; and the owner's trial says what happens whenever origin/main's jawd has migrations his checkout's hasn't, with no counts to go stale. The review's Minors were taken too: the root README says which releases start jawd and that the app doesn't start it at login; the Mac plan's rule says "any other folder", since a branch's release starts none on jaw's own folder with no `session.db` either; the design says `--demo` runs `jawd --engine demo`, as the Mac README does, where it said `jawd demo`, jawd's million-message account, and likened it to `jaw demo`, which runs no jawd; "What earlier builds left" says the start on `SMAppService`'s answer once; the trial's voice message is recorded in the terminal on his own Mac, through the jawd the app started there, and, as Task 5's reviews asked, with the app open and after it has quit; `jaw agent`'s "app's agent", in the design's "Changes outside the app" and the Mac plan's "Alongside", is only an earlier Jaw's since this step; and `make mac-test`'s comment names `mac/scripts/test-built-from`, which it runs first. The `JAW_TRACE` sentence in `mac/README.md` is left as it is, to be settled in the branch's review with `traceShown()`'s doc comment, whose words it repeats.

**Amended in the branch's review.** The branch's review (5 November) found nothing that starts a second jawd, a branch's jawd on an account's folder or a jawd the record doesn't speak for, and asked for a round of fixes before landing, each checkable without jawd or the app. `built-from` has git look at every tracked file itself, with `core.fsmonitor` off for `git status` and both `ls-files` and no submodule ignored, since a `core.fsmonitor` hook answering no paths hid a changed `crates/lib.rs` and it said `main`; it asks git whether the folder it's given is inside its work tree, rather than comparing two spellings of its path, so `make mac` from a checkout's path typed in another case, on a volume that ignores case, no longer stops, while a folder whose `core.worktree` names another checkout is still refused; and its comment and Task 2's second amendment add to what it can't see a change undone with its files' times put back and settings outside the checkout, Cargo's and go's own and the environment's. `mac/scripts/test-built-from` tries 69 cases, the two new ones failing on the built-from before them; the hook's case first brings its copy's index up to date, as a checkout's in use is. The bundle removes the jawd and jaw an earlier build left in `target/release` before cargo builds, so the copy fails rather than sign another revision's under this tree's record when a target a setting of Cargo's names has cargo build into `target/<triple>`. `traceShown()`'s comment and the Mac README's `JAW_TRACE` sentence say what the window shows of jawd is traced at launch and at each update the app drains, once a frame with it is drawn, where they said each time it or the window's visibility changes, and `trace`'s comment says it's traced beside each update drained and when: a window hidden, a folder picked or a layout changed is traced at the next drain, which is all the check reads, so the code stays as it is, and where the Decision on the check, Task 4's Interfaces and Task 5's Step 8 have a line come each time it changes, it comes at the next drain. `Starter`'s comments say a jawd another's `jaw.lock` refused is started again only when that came within 15 s of the first start, and that past them the window says why. Beyond this plan, the protocol's "The microphone on macOS" says whose words macOS asks in for a jawd launchd, the Mac app or a terminal started; the parity plan no longer says voice hasn't started in jawd, and step 2.6 says what jawd has served since 2 November; and the Mac design's Recording says macOS asks in Jaw's name for a jawd the app started. In this plan, Task 2's and Task 5's amendments name this branch's commits by what they are, since a rebase changes their hashes; the first Decision gives the owner's checkout as it was on 30 October; and the owner's trial says that macOS reopening a Jaw left open at logout counts as opening it with his jawd down, and that origin/main's engine can leave his checkout's refusing session.db as its jawd can jaw.db, so he rebuilds his engine with his jawd, and has the microphone asked for in Jaw's name as he chose, the recording after the app has quit being the reviews'. Task 2's Step 5, Task 3's Step 3, Task 4's Step 3 and Task 5's Step 5 show the code as it now is.

---

## After this plan: `--for-apps` and the hello's `app`

The step's other half has no code here. It starts once `jawd-lifetime`'s first step, jawd's own, has on its branch the protocol's text for hello's `app` and for `--for-apps`, with their fixtures. It lands no sooner than jawd's step does, since the jawd a bundle carries is built from the same tree, and an older jawd turns down a flag it doesn't know. What it does, as this plan reads the lifetime design; where jawd's text says otherwise, it's right:

- **The release passes `--for-apps`.** `Starter.spawn` gives a release's jawd that argument; `--demo`'s jawd doesn't get it, since the app stops it, and the lifetime design keeps `--engine demo` running until it's stopped. The starter's tests pin both.
- **The app says `app` in its hello.** JawKit's hello carries `"app": true` for the app's connection, its request fixture checked as `hello.request.json` is now. The live tests' own connections don't say it, so they never keep a jawd running.
- **The done-when, whole.** The check's runs change where jawd now stops: after the app quits, jawd stops within its wait (30 s by the lifetime design's Default, or what the owner settles in its review), and after the app's process group is killed too, since a dropped connection counts as gone; a jawd whose app went before saying hello stops that long after it listens; and while an app is connected, jawd stays. The check reads `jawd status`, which then says how many apps are connected and when jawd stops. "Quitting it stops jawd within 30 s" holds, and the parity plan's step 1.2 is done.
- **What stays.** The rule for an account's folder, the earlier agent's unregistering after the start and never waited on, the terminal agent's name, the 15 s, and starting a jawd again after another held the folder, which the stop after the last app makes likelier: an app that opens just as jawd stops finds the lock held for the few seconds jawd's stop takes.
- **What isn't the Mac's.** The terminal's half (`start_jawd` passing `--for-apps`, jaw's hello saying `app`, `jaw agent install` and `jaw agent status` going) is on the terminal's branch. Quit asking first when something's due within the hour comes with scheduled messages (step 3.6), since the app shows none yet. The Mac design's decisions of 26 October and its notifications are the lifetime design's docs step's.
- **The owner's trial.** Once this has landed, `make mac` in a checkout or worktree at origin/main, which builds jawd, jaw and jaw-engine on this Mac, so it's for the owner or the controller, makes the bundle this plan's check stood in for, signed with his Developer ID and under the hardened runtime. Made in his own checkout, whose HEAD isn't on origin/main while his own commit is there, even after a `git pull`, it records `branch` and starts no jawd on his folder. Opened on the build Mac's build user, or a user made for it, with the engine beside jawd this time, the bundle from origin/main reaches the linking screen with a code drawn. On his own Mac it connects to his jawd while that runs, starting nothing, as now. While his jawd is down, after a reboot and before he has run jaw, it starts origin/main's jawd on his folder, which goes on running after the app quits; macOS reopening a Jaw left open at logout, as he logs back in, counts as opening it with his jawd down, before he can start his own. His jawd is a debug build of his checkout: when origin/main's jawd has migrations his checkout's hasn't, it upgrades his jaw.db past what his checkout's jawd opens, backing it up first beside it as `jaw.db.before-v<the version it upgrades from>` when a data step is among them, and his own jawd then refuses jaw.db until it's built from a checkout with origin/main's migrations. origin/main's engine likewise brings session.db's schema up to date as it opens it, so a newer whatsmeow there can leave his checkout's engine refusing session.db too. So before he opens it with his jawd down, he brings his checkout up to origin/main (`git pull`) and rebuilds his jawd and its engine (`make build`), or starts his own jawd first. On his own Mac, through the jawd the app started there, he records a voice message in the terminal (Ctrl-R) with the app open, and again after it has quit: macOS should ask for the microphone once, in Jaw's name, as he chose on 5 November, not the terminal app's.
- **The microphone, done here.** A jawd the app starts counts as the app's when it asks for the microphone, and jawd has recorded voice messages since 2 November, ahead of parity steps 2.6 and 2.7, so Jaw.app now carries `NSMicrophoneUsageDescription` in its own Info.plist, beside the one jawd's build embeds, and `com.apple.security.device.audio-input` in its entitlements for the hardened runtime, as Task 5's amendment says.
