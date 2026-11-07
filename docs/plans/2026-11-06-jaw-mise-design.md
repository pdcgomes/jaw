# jaw: its tasks in mise

6 November 2026. A design; nothing here is built. It replaces jaw's Makefile with mise tasks, laid out as Redlamp's are, and comes before the Mac app's release, which gets a design of its own once this is built. The owner said on 6 November at 21:19 to land it overnight, without waiting for `v0.1.0`; "How it lands" says what follows. The owner's answers are marked "Owner, 6 Nov". The counts below are of origin/main at d4ca8bca.

## What it is

The owner asked on 6 November whether jaw had a script to build and notarize the Mac app, to reuse Redlamp's way if it hadn't, and to move jaw from make to mise, so his projects all work the same way. Redlamp (`~/src/darkroom`), drift-station, lookout, omoi-station and algoreason each have a `mise.toml` at the root and their tasks as files in `mise/tasks/`; jaw has a Makefile, and `mac/mise.toml` for the Mac app's Tuist.

jaw has no such script: `make mac` builds Jaw.app and signs it with the Developer ID, but nothing notarizes it ("Later" says where that starts). This design is the first of the two pieces of work: every target of the Makefile becomes a mise task, and the Makefile goes.

## Decisions

- **mise first, then the Mac app's release** (Owner, 6 Nov), so the release is written as mise tasks among the others, with nothing left calling make. Neither was to land before `v0.1.0` was tagged, since the 0.1.0 checklist (`docs/releases/0.1.0-publish.md`) and its formula used make; then the owner said to land this one overnight without waiting for it (Owner, 6 Nov, 21:19). The owner then tagged `v0.1.0` first, at fdeba680 on 7 November at 05:30, so 0.1.0 ships with make: its notes, checklist, tarball and formula keep make's commands, and the next release is the first with mise's.
- **The Makefile is deleted in the same branch** (Owner, 6 Nov), which changes the rules, the READMEs, CI and the Homebrew formula with it. Not a stub that prints the mise command, and not a Makefile that runs the mise tasks.
- **One file per task in `mise/tasks/`** (Owner, 6 Nov), as the owner's other projects have them, rather than tasks inline in `mise.toml`'s `[tasks]`, which Homebrew couldn't run without mise.
- **One `mise.toml`, at the root** (Owner, 6 Nov), holding `mac/mise.toml`'s tools and settings; `mac/mise.toml` goes.
- **mise doesn't pin Rust or Go** (Owner, 6 Nov). rustup's stable and Homebrew's Go build here, as now, and CI keeps its own setup. Pinned in `mise.toml` alone, `mise run` and plain `cargo` would use different toolchains whenever stable moved, and every worktree's `target/` would build again each time an agent switched between them, which the disk rule (`.cursor/rules/disk-space.mdc`) exists to prevent.
- **make's names, with three changes** (Owner, 6 Nov): `hooks` becomes `setup`, as Redlamp's one-time setup is; `make release`, which built the release binaries, goes, since `install` builds them itself; and `scripts/release.sh` becomes `mise run release`, as Redlamp's release is.
- **`install` asks for sudo for the copy alone** (Owner, 6 Nov), so one command installs into `/usr/local`, where make needed `make release` and then `sudo make install`.
- **Homebrew runs the install task as a script** (Owner, 6 Nov), without mise.
- **What's current changes; records don't** (Owner, 6 Nov). The text that tells you what to run changes; audits, reviews, release notes and dated plans keep what they say, as records of what was run.

## `mise.toml`

```toml
[tools]
# The Mac app's (mac/), so macOS only: Linux's builds and CI skip them.
tuist = { version = "4.197.3", os = ["macos"] }
swiftformat = { version = "0.62.1", os = ["macos"] }
swiftlint = { version = "0.65.0", os = ["macos"] }

[settings]
# mise run runs a task without installing the tools above first, so one that fails to download
# stops no task; mise exec, which bundle and build-mac run Tuist with, still installs it.
task.run_auto_install = false

[env]
# Signing and identity. Under mise these win over the environment and mise's global config; only
# a .mise.local.toml here overrides them, and built-from counts that as an uncommitted change, so a
# Jaw built with one won't start jawd on an account's folder. mac/scripts/bundle run without mise
# takes JAW_TEAM_ID from the environment.
JAW_TEAM_ID = "3JP75Z3F98"
JAW_BUNDLE_ID = "dev.badcoffee.jaw"
# notarytool's keychain profile for the team's Apple ID, which Redlamp shares with Drift Station.
JAW_NOTARY_PROFILE = "driftstation-notarize"
```

`.gitignore` gains `.mise.local.toml`, as Redlamp's has it. It's ignored so that one is never committed, not as the place for a machine's settings: built-from counts it.

Checked on 6 November with mise 2026.9.7 on this Mac: a tool marked for another system is neither listed nor installed, and one marked `macos` is; a task run from a subfolder runs at the repository's root, with these settings; and `mise run` runs the tasks of a checkout whose `mise.toml` was never trusted, as a new worktree's is, settings and all, while `mise ls` refuses it until it's trusted, and `mise exec`, in an agent's shell, trusts it on its own (as Task 2's implementer and reviewer found). So the two scripts that run Tuist through `mise exec` say the trust rather than leave it to mise, trusting their own checkout's `mise.toml` first: `mac/scripts/bundle` when it builds the app here, and `mac/scripts/build-mac` on the build Mac, which trusts `mac/mise.toml` today and trusts the root's instead.

## The tasks

Each task is a bash script in `mise/tasks/`, as in Redlamp: `#!/usr/bin/env bash`, `#MISE description="…"`, `#MISE alias="…"` where it has one, and `set -euo pipefail`. Four things differ from Redlamp's:

- **It finds the repository from its own path,** not from git, since `install` also runs in the release's tarball, which has no `.git`.
- **It runs without mise,** for Homebrew and for anyone without it: a task that needs another runs that one's script (`mise/tasks/engine`) rather than `mise run` or mise's `depends`, and it keeps to the bash 3.2 macOS ships.
- **It unsets `CDPATH`,** on the line after `set -euo pipefail`. Exported, it has a relative `cd` print the folder it goes to, so a task run by a relative path, as `mise/tasks/build`, would read its own folder as two lines and fail, and `scripts/version`'s own `cd` would go astray and say `unknown`.
- **A task that takes no arguments refuses them,** on the line after that, exiting 2 with a message: mise passes a task every word after its name, with or without `--`, so make's habit `mise run install PREFIX=~/.local` would reach `install` as an argument and install into `/usr/local`. The four that take arguments, `check-parity`, `prune-builds`, `perf` and `sync-protos`, pass them on.

make's variables become environment variables with the same defaults: `GO` and `CARGO` (`go`, `cargo`), `JAW_VERSION` (`scripts/version`'s), and for installing `PREFIX` (`/usr/local`), `BINDIR` (`$PREFIX/bin`), `LIBEXECDIR` (`$PREFIX/libexec/jaw`) and `DESTDIR`. `ARGS=` goes: arguments follow `--`, as in `mise run check-parity -- --table`.

| Task | Was | What it does |
| --- | --- | --- |
| `build` (`b`) | `make`, `make build` | `engine`, then `rust` |
| `engine` | `make engine` | `target/engine/jaw-engine`, stamped with `scripts/version`'s version |
| `rust` | `make rust` | `cargo build --workspace` |
| `install` | `make release`, `make install` | release builds of jaw, jawd and the engine, copied into `BINDIR` and `LIBEXECDIR` |
| `uninstall` | `make uninstall` | takes them out again |
| `test` (`t`) | `make test` | the engine, then Go's tests with `-race` and the workspace's |
| `lint` (`l`) | `make lint` | `go vet`, gofmt, rustfmt, and clippy with warnings as errors |
| `fmt` | `make fmt` | gofmt and rustfmt, writing |
| `perf` | `make perf` | `scripts/perf.py` on release builds; `-- --quick` leaves out the slowest |
| `proto` | `make proto` | protoc-gen-go at `engine/go.mod`'s version, built again when older than it, then buf v1.74.0 |
| `sync-protos` | `make sync-protos` | `scripts/sync-protos.py` |
| `check-generated` | `make check-generated` | `proto`, then nothing generated differs from what's committed |
| `check-audit` | `make check-audit` | `scripts/check-data-audit.py origin/main HEAD` |
| `check-parity` | `make check-parity` | `scripts/check-parity.py`, with arguments after `--` |
| `setup` | `make hooks` | `scripts/pre-push` into the hooks folder every worktree shares |
| `mac` | `make mac` | `mac/scripts/bundle` |
| `mac-test` | `make mac-test` | `mac/scripts/test-built-from`, jawd's debug build, then JawKit, JawThemes, JawUITests and JawHarnessTests through `mac/scripts/build-mac` |
| `clean` | `make clean` | `cargo clean` |
| `prune-builds` | `make prune-builds` | `scripts/prune-builds.py`, with arguments after `--` (`-- --dry-run`) |
| `release` | `scripts/release.sh` | moved as it is: the tarball, its sha256 and the formula in `target/dist/`, then the commands that publish them, printed for you to run |

`mise tasks` lists them with their descriptions, and the README gains the same table in place of its make lines, as Redlamp's README has one.

## Installing

- **As you,** `install` builds jaw and jawd for release (`cargo build --release --locked -p jaw -p jawd`) and the engine, then copies the three. Each folder you can't write, such as `/usr/local/bin` on most Macs, is written with `sudo`, which asks for your password once at most; where sudo can't ask, as in a script with no terminal, it fails saying so. `PREFIX=~/.local mise run install` needs no sudo.
- **As root,** as with `sudo mise/tasks/install`, it installs what's been built and builds nothing, as `make install` did, so cargo and go never start again in root's caches; with nothing built it says to run `mise run install` as yourself.
- **`uninstall`** takes the three out with the same variables, and `libexec/jaw/` if it's left empty, with sudo where it needs it.

## Homebrew

The formula (`packaging/homebrew/jaw.rb`) runs the task itself:

```ruby
  def install
    ENV["PREFIX"] = prefix.to_s
    system "mise/tasks/install"
  end
```

Homebrew needs no mise, so it never installs Tuist; the files land where they do today, so the formula's test is unchanged. The tarball holds `mise/tasks/`, as `.gitattributes` keeps nothing out, and `git archive` keeps the files' modes.

`v0.1.0` was tagged before this landed (fdeba680, 7 November), so 0.1.0's tarball has the Makefile, and the tap's formula for 0.1.0 keeps `make install`, which works with it. `brew install --HEAD`, which builds main with the tap's formula, fails from this landing until the next release puts the new formula, running `mise/tasks/install`, in the tap; the tap is made by 0.1.0's checklist, so nobody has installed from HEAD yet.

## Building from source

The README's install section and `docs/first-run.md` say `mise run install`, `PREFIX=~/.local mise run install` and `mise run uninstall`, `mise run build` for working on jaw, and that without mise the same tasks run as scripts, such as `mise/tasks/install`.

## CI

`.github/workflows/ci.yml`'s test job installs mise after its checkout with `jdx/mise-action@v2`, as Redlamp's CI does, and runs `mise run lint`, `mise run test` and `mise run check-generated` where it ran make. Its Go and Rust setup (setup-go from `engine/go.mod`, dtolnay's stable toolchain, rust-cache) stays. On Linux mise installs nothing, the Mac's tools being macOS only; on the macOS runner, which runs only by hand, it installs them too, which the job doesn't use but which does no harm. The `data-audit` and `parity` jobs run their Python scripts directly and don't change.

## The hook

`mise run setup` copies `scripts/pre-push` into `$(git rev-parse --git-common-dir)/hooks/`, as `make hooks` did. It doesn't set `core.hooksPath`, as Redlamp's setup does: that points each worktree at the hooks in its own checkout, so a worktree at a commit from before them would push to main with no audit check. The hook doesn't use make, reading `check-data-audit.py` from the commit being pushed, and this repository's installed copy matches `scripts/pre-push` (checked on 6 November), so nothing is installed again when this lands.

## What else changes

87 mentions of make's targets, in 37 files other than the records:

- **The rules:** `data-integrity.mdc` (`mise run check-audit`; the pre-push hook, `mise run setup`) and `disk-space.mdc` (`mise run prune-builds`; `make prune-builds` in a checkout from before the move, and `python3 scripts/prune-builds.py` in either, since worktrees at older commits keep their Makefile until they rebase).
- **The documents you read before running something:** `README.md`, `mac/README.md`, `protocol/README.md`, `marketing/jaw-in-a-minute/README.md`, `docs/first-run.md`, and the two designs that stay current, [jaw's](2026-10-16-jaw-design.md) and [the Mac app's](2026-10-26-jaw-mac-design.md).
- **The Mac's scripts and project:** `mac/scripts/bundle` runs `mise/tasks/engine` where it ran `make -s engine`, and trusts its checkout's `mise.toml` before `mise exec`; `built-from`, `test-built-from` and `build-mac` say `mise run mac`, and `build-mac` trusts the root's `mise.toml`; `mac/Project.swift` and `mac/parity.toml` in their comments.
- **Messages that tell you to run make, and what quotes them:** jaw's (`crates/jaw/src/wire.rs`, `agent.rs`), jawd's (`sock.rs`, `main.rs`, `engine/go.rs`), the Mac app's (`mac/JawKit/Sources/Client/Starter.swift`, `Build.swift`) with their tests (`StarterTests`, `BuildTests`), and the protocol's fixture of the version error (`protocol/fixtures/error.version.json`). Each names the mise task, worded to read as a sentence: "mise run build builds it".
- **Comments and docstrings:** `crates/jaw-proto/src/local.rs`, `crates/jaw/src/bench.rs`, `crates/jawd/build.rs`, `crates/jawd/src/demo/mod.rs`, `crates/jawd/tests/installed.rs`, `crates/jawd/src/store/write/outbox.rs`, `scripts/perf.py`, `scripts/prune-builds.py`, `marketing/jaw-in-a-minute/jaw.py`, `engine/cmd/jaw-engine/main.go`, and `proto/engine/engine.proto`, with the Go generated from it, which copies its comment.
- **`scripts/release.sh`'s two mentions elsewhere:** the formula's first comment and a case in `mac/scripts/test-built-from`.

The records keep what they say: `docs/audits/` (but its README, which says how the audits are checked), `docs/reviews/`, `docs/releases/` (0.1.0's notes and checklist with them, since 0.1.0 was tagged before this landed) and the dated plans, about 400 mentions, each saying what was run when it was written.

### The data layer

14 of the mentions are in six of the data layer's files, as `scripts/check-data-audit.py` lists them: jawd's `engine/go.rs` (8), `main.rs` (2), `sock.rs` and `store/write/outbox.rs`, the engine's `cmd/jaw-engine/main.go`, and `proto/engine/engine.proto`. They're messages and comments, and nothing that writes, acknowledges or reads data changes; the data-integrity rule still has the database expert audit them, so they go in a commit of their own, with the trailer naming its note. (`store/vfs.rs`'s one match is a function called `make`, and stays.)

### What jawd is built from

Added while planning, on 6 November. `mac/scripts/built-from` calls a Jaw `changed`, so that its jawd never starts on an account's folder, when anything jawd and the engine are built from has uncommitted changes, and it counts the makefiles among those inputs (`Makefile GNUmakefile makefile`): `make engine` defined the engine's build, and a stray `GNUmakefile` would have changed it. Their place goes to mise's tasks and settings, any of which can change the build or the environment `mise run mac` builds in: at the top and in `mac/` (where the Mac's README has `mise run mac` started), the folders `mise/` (whose `tasks/engine` builds the engine), `.mise/` and `.config/` and every file whose name has `mise` in it and ends `.toml`, has `miserc` in it or is `.tool-versions`, and `mac/`'s `mise-tasks/` and `.mise-tasks/`, whose tasks stand in for the top's. It counts these and every other input, Cargo's and rustup's among them, whatever the case of their names, which git matches with `:(icase)` pathspecs and the files with `:(icase,glob)` patterns, with the shell's globbing off and git's pathspec settings (`GIT_LITERAL_PATHSPECS` and the rest) unset; what it still can't see, as its header says, is a name whose case differs beyond ASCII's, such as `.miſe.local.toml` with a long s, which the volume reads as `.mise.local.toml` and git, folding only ASCII, doesn't match, mise's settings in other folders, should `mise run mac` start in one, and settings outside the checkout or in the environment. Task 8's review, on 7 November, found the first list (`mise.toml`, `mise.local.toml`, `.mise.toml`, `.mise.local.toml`, `.config/mise.toml` and `.tool-versions`, with the three folders) incomplete: mise 2026.9.7 also reads `.config/mise.local.toml`; a `.miserc.toml` or `.config/miserc.toml`; the `mise.<env>.toml` files that a miserc's `env`, `MISE_ENV` or `auto_env`'s platforms (`mise.macos.toml` and the like) name, local, dotted and in `.config/` alike; a `mac/mise.toml` or `mac/.mise.local.toml` when `mise run mac` starts in `mac/`; and a name in another case, such as `.Mise.local.toml`, which this volume reads as the same file. It found too that `GIT_LITERAL_PATHSPECS` or `GIT_NOGLOB_PATHSPECS` in the environment, or the shell's globbing of an unquoted pattern, could hide a file from patterns like these. Its re-review, the same day, found that built-from still said `main` for folders and older inputs named in another case, which mise, Cargo and rustup read where git's pathspecs matched case-sensitively (`.MISE/config.toml`, `.Config/mise/config.toml`, `.CARGO/config.toml`, `Rust-Toolchain.toml`); for `mac/.mise/`, `mac/mise/` and `mac/.config/`, which mise reads when `mise run mac` starts in `mac/`, and a `mac` task in `mac/mise-tasks/` or the like, which it then runs in place of the top's; and for a `.config` that's a symlink to a folder, which git lists as the link alone. A `.mise.local.toml` makes a build `changed` though git ignores it, as an untracked Cargo or go setting does. `mac/scripts/test-built-from` follows: its checkout has `mise.toml` and `mise/tasks/engine` where it had a Makefile; a new file of each of mise's kinds says `changed` where a new `GNUmakefile` did; and `mise.toml` and `mise/tasks/engine` changed say `changed` where the Makefile did. built-from's header names mise's tasks and settings where it named the makefiles.

## How it lands

- **Built** on the branch `mise`, in `~/src/jaw-mise`, from origin/main.
- **Lands overnight, without waiting for `v0.1.0`** (Owner, 6 Nov, 21:19), with the gate any branch passes and the database expert's approved note. `v0.1.0` was tagged first, at fdeba680 on 7 November, so 0.1.0 ships with make: its notes and checklist keep make's commands, and the commit that had moved them to mise's (Task 14, Step 12) was dropped at the landing.
- **`mac-menus` and `mac-perf` change the Makefile too:** `mac-perf` adds `perf-mac` and a line in `mac-test`. A branch that lands after this one moves its own make lines into tasks as it rebases; if this one lands after them, it moves theirs.
- **The coordinator is told** when it lands: its briefs and gate scripts, kept outside the repository, call make's targets.

## Checks

- Every task runs in the branch's worktree and does what its target did: `build`, `engine`, `rust`, `test`, `lint`, `fmt` (leaving nothing changed), `perf -- --small` (which checks the harness on 50,000 messages, not the budgets, and on main fails at "photo out" and "steady" however it's run, as Task 6 found), `proto`, `sync-protos`, `check-generated`, `check-audit`, `check-parity` with and without `-- --table`, `mac`, `mac-test`, `prune-builds -- --dry-run`, `release` at a scratch tag in a scratch clone, and `setup` in a scratch clone.
- `install` into a scratch prefix, through `mise run` and as a plain script with no mise on the PATH, then the formula's test by hand against what it installed: the three say the version, and `jawd --engine demo` on a scratch home has `jaw snapshot` draw Lisbon Climbing; then `uninstall` takes out the three and `libexec/jaw/`.
- The release's tarball, made by `mise run release` in a scratch clone, unpacked and installed with `mise/tasks/install` and no mise on the PATH, as Homebrew does: `jaw --version` says the version.
- `install` into a folder only root can write asks for sudo for the copy alone: checked once by the owner, since an agent's shell can't answer sudo.
- CI on Linux fails nowhere main doesn't (on 5 November main's test job failed at `lint`, which `linux-ci` was fixing).
- `git grep` finds none of make's targets in the files above.
- The gate (`check-generated`, `lint`, `test` under the flakes rule, `check-audit`), and the database expert's note ending `Verdict: approved`.
- `mise run prune-builds` after the builds, as the disk rule asks, and the scratch folders removed.

## Later

- **The Mac app's release,** the second piece of work, with its own design once this is built. It starts from what's there on 6 November:
  - `make mac` builds Jaw.app with jawd, jaw-engine and jaw inside, each signed with the Developer ID, the hardened runtime and a timestamp; but the app's signature carries `com.apple.security.get-task-allow`, which notarization refuses. Xcode signs it into the ad hoc build on the build Mac and `bundle`'s re-signing keeps it with `--preserve-metadata=entitlements`; Redlamp's release builds with `CODE_SIGN_INJECT_BASE_ENTITLEMENTS=NO`, and [the starting-jawd plan](2026-10-30-jaw-mac-starting-jawd-plan.md) lists it as due before the Mac's releases are notarized.
  - Redlamp's `mise/tasks/notarize` (`notarytool submit --wait` with the keychain profile, then staple and `spctl --assess`) carries over with `JAW_NOTARY_PROFILE`, which `mac/mise.toml` names today and the root's `mise.toml` will, as do its zip and disk image scripts (`scripts/release-dmg.sh`, `check-release-zip.sh`, `check-release-dmg.sh`).
  - Its questions: Sparkle's updates now, or with the Mac plan's Mac-M5 step 8; one version and tag for the terminal and the app, or the app's own (`mac/Version.xcconfig` says 0.1.0-prealpha); whether `mise run release` publishes, as Redlamp's does, or prints the commands, as jaw's does; and a Homebrew cask, named so it doesn't clash with the formula's `jaw`.
  - Making the disk image mounts it, which the agents' sandbox refuses (as Redlamp's scripts say), so the release is run in the owner's own terminal.
- **Pinning Rust and Go,** if it's wanted later, in `rust-toolchain.toml` and in `engine/go.mod`'s `toolchain` line, which every cargo and go here reads, mise's tasks' included, rather than in `mise.toml`.
- **`brew install --HEAD`** works again once the next release's formula is in the tap, `v0.1.0` having been tagged before this landed.
