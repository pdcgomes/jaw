# jaw: its tasks in mise, the plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every target of jaw's Makefile becomes a mise task in `mise/tasks/`, everything that names make names mise, and the Makefile goes, as [the design](2026-11-06-jaw-mise-design.md) says.

**Architecture:** One `mise.toml` at the root holds the Mac's tools, macOS only, and the signing settings. Each task is one bash file in `mise/tasks/` that finds the repository from its own path and runs without mise, so Homebrew and the release's tarball run them as plain scripts. The callers (`mac/scripts/bundle`, CI, the formula) move to the tasks, `built-from` counts mise's files where it counted the makefiles, the text follows, and the Makefile is deleted last.

**Tech Stack:** mise 2026.9.7 file tasks in bash 3.2; cargo; go; buf v1.74.0; Tuist 4.197.3 and Xcode for the Mac app; GitHub Actions with `jdx/mise-action@v2`; a Homebrew formula.

## Global Constraints

- Work in `~/src/jaw-mise`, on the branch `mise`. Scratch goes under `/tmp/jaw-mise/` only, removed in Task 18.
- The owner's word, 6 November at 21:19: work overnight and land on main when done, so it's ready in the morning, without waiting for `v0.1.0`. If `v0.1.0` isn't tagged when this lands, 0.1.0's notes and checklist, which are still to be run against the new main, say mise's commands (Task 14, Step 12). The branch goes to GitHub only for its CI, after the audit, and is deleted there once it has landed, so 0.1.0's checklist (`docs/releases/0.1.0-publish.md`, step 5) finds no branch of it. As it turned out, the owner tagged `v0.1.0` at fdeba680 on 7 November at 05:30, before the landing, so 0.1.0 ships with make, Step 12's commit was dropped at the landing, and, with his release under way, the branch went straight to main, whose CI is watched instead.
- The owner answered neither question of 6 November, then said to start: so built-from counts mise's tasks and settings as the design's "What jawd is built from" says, and the plan is carried out a fresh subagent to a task, each reviewed, with a review of the whole branch before the gate.
- The data-integrity rule (`.cursor/rules/data-integrity.mdc`): Task 13's commit touches the data layer, so it gets the database expert's audit and the `Data-Audit:` trailer (Task 16). Never skip, fake or self-write the audit.
- The disk rule: `export CARGO_INCREMENTAL=0` in every shell that builds here; `df -h /System/Volumes/Data` before the first build, stopping below 150 GB free; `mise run prune-builds` between rounds and before the final report.
- Never stop processes by name; stop only PIDs you started. Never run the Go engine on a folder holding a `session.db`: scratch homes use `jawd --engine demo`.
- Every task script: `#!/usr/bin/env bash`, `#MISE description="…"`, `#MISE alias="…"` where the design's table gives one, `set -euo pipefail`; the repository found from the script's own path; executable; bash 3.2 only (no associative arrays, `mapfile` or `${x,,}`); another task called by its script (`"$here/engine"`), never by `mise run` or `depends`.
- Environment variables, with make's defaults: `GO` (`go`), `CARGO` (`cargo`), `JAW_VERSION` (through `scripts/version`), `PREFIX` (`/usr/local`), `BINDIR` (`$PREFIX/bin`), `LIBEXECDIR` (`$PREFIX/libexec/jaw`), `DESTDIR` (empty).
- Text names a task as `mise run <task>`; a message reads as a sentence: "mise run build builds it".
- Records keep their words: `docs/audits/`, `docs/reviews/`, `docs/releases/` (0.1.0's notes and checklist with them, since `v0.1.0` was tagged before the landing and Task 14's Step 12 was dropped), and every dated plan but `2026-10-16-jaw-design.md` and `2026-10-26-jaw-mac-design.md`. A dated statement of what was run names the script (`scripts/perf.py`), not a command that didn't exist then.
- Commits in jaw's style: one subject in prose saying what changed and why, ending "Docs only" or "Tests only" where that's all it is.
- Until Task 14 deletes it, the Makefile keeps working, so every commit before it leaves both ways usable.
- In this worktree, after Task 2: `mise trust -q mise.toml` once, since `mise ls` refuses an untrusted config (`mise run` doesn't, and `mise exec` trusts one on its own).
- A PATH with no mise on it, for the checks that say so, made once:

  ```bash
  mkdir -p /tmp/jaw-mise/nomise-bin
  ln -sf /opt/homebrew/bin/go /opt/homebrew/bin/gofmt /opt/homebrew/bin/cmake /tmp/jaw-mise/nomise-bin/
  export NOMISE="/tmp/jaw-mise/nomise-bin:$HOME/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin"
  env PATH="$NOMISE" sh -c 'command -v mise || echo no mise'
  ```

  Expected: `no mise`.

## Files

- **Create:** `mise.toml`; `mise/tasks/` `build`, `engine`, `rust`, `test`, `lint`, `fmt`, `perf`, `proto`, `sync-protos`, `check-generated`, `check-audit`, `check-parity`, `prune-builds`, `clean`, `setup`, `mac`, `mac-test`, `install`, `uninstall`; `docs/audits/2026-11-06-mise.md` (the database expert's, named for the day it's written).
- **Move:** `scripts/release.sh` to `mise/tasks/release`.
- **Delete:** `Makefile`, `mac/mise.toml`.
- **Modify:** `.gitignore`; `.github/workflows/ci.yml`; `packaging/homebrew/jaw.rb`; `mac/scripts/bundle`, `build-mac`, `built-from`, `test-built-from`; `mac/Project.swift`, `mac/parity.toml`, `mac/README.md`; `mac/JawKit/Sources/Client/Starter.swift`, `Build.swift`; `mac/Tests/JawKitTests/StarterTests.swift`, `BuildTests.swift`; `crates/jaw/src/wire.rs`, `agent.rs`, `bench.rs`; `crates/jaw-proto/src/local.rs`; `crates/jawd/build.rs`, `src/demo/mod.rs`, `tests/installed.rs`; `scripts/perf.py`, `prune-builds.py`; the data layer's `crates/jawd/src/engine/go.rs`, `main.rs`, `sock.rs`, `store/write/outbox.rs`, `engine/cmd/jaw-engine/main.go`, `proto/engine/engine.proto`; the generated `engine/internal/link/enginepb/engine.pb.go`; `protocol/fixtures/error.version.json`, `protocol/README.md`; `README.md`; `docs/first-run.md`; `docs/plans/2026-10-16-jaw-design.md`, `2026-10-26-jaw-mac-design.md`; `.cursor/rules/data-integrity.mdc`, `disk-space.mdc`; `marketing/jaw-in-a-minute/README.md`, `jaw.py`.

---

### Task 1: The workstream canvas

**Files:** Create `~/.cursor/projects/Users-pedrogomes-src-jaw/canvases/ws-mise.canvas.tsx` (outside the repository).

- [ ] **Step 1:** Read `~/.cursor/skills-cursor/canvas/SKILL.md` and `.cursor/skills/workstream-canvas/SKILL.md`, and follow them.
- [ ] **Step 2:** Copy `.cursor/skills/workstream-canvas/template.tsx` to the path above and fill in its `workstream` object: the goal above; one step per task of this plan, with the task's number as its `id`; the design's decisions; Needs you: approving this plan, `v0.1.0`'s tag and the repository made public, the sudo check (Task 17), telling the coordinator (Task 17), and the go-ahead to land; links to the design, this plan and the branch.
- [ ] **Step 3:** Link it in the reply: `[jaw's tasks in mise](/Users/pedrogomes/.cursor/projects/Users-pedrogomes-src-jaw/canvases/ws-mise.canvas.tsx)`. From here on, each task's step changes on the canvas in the same turn as its todo.

---

### Task 2: One `mise.toml` at the root

**Files:** Create `mise.toml`. Delete `mac/mise.toml`. Modify `.gitignore`, `mac/scripts/bundle`, `mac/scripts/build-mac`.

**Interfaces:** Produces the settings `JAW_TEAM_ID`, `JAW_BUNDLE_ID` and `JAW_NOTARY_PROFILE` and the tools `tuist`, `swiftformat` and `swiftlint` for every task run through mise.

- [ ] **Step 1: See it missing.** `cd ~/src/jaw-mise && mise exec -- sh -c 'echo "${JAW_TEAM_ID:-none}"'`. Expected: `none` (the root has no settings; `mac/mise.toml` holds them).
- [ ] **Step 2: Write `mise.toml`:**

```toml
[tools]
# The Mac app's (mac/), so macOS only: Linux's builds and CI skip them.
tuist = { version = "4.197.3", os = ["macos"] }
swiftformat = { version = "0.62.1", os = ["macos"] }
swiftlint = { version = "0.65.0", os = ["macos"] }

[env]
# Signing and identity. Override per machine in .mise.local.toml if needed.
JAW_TEAM_ID = "3JP75Z3F98"
JAW_BUNDLE_ID = "dev.badcoffee.jaw"
# notarytool's keychain profile for the team's Apple ID, which Redlamp shares with Drift Station.
JAW_NOTARY_PROFILE = "driftstation-notarize"
```

- [ ] **Step 3:** `git rm -q mac/mise.toml`, and add the line `.mise.local.toml` to the end of `.gitignore`.
- [ ] **Step 4:** In `mac/scripts/bundle`, the build here trusts the root's config before Tuist. Replace

```sh
    (cd mac && mise exec -- tuist generate --no-open >/dev/null &&
```

with

```sh
    (cd mac && mise trust -q "$repo/mise.toml" && mise exec -- tuist generate --no-open >/dev/null &&
```

- [ ] **Step 5:** In `mac/scripts/build-mac`'s `run()`, replace `mise trust -q . >/dev/null 2>&1;` with `mise trust -q ../mise.toml >/dev/null 2>&1;`.
- [ ] **Step 6: See it there.** `mise trust -q mise.toml && mise exec -- sh -c 'echo $JAW_TEAM_ID $JAW_BUNDLE_ID $JAW_NOTARY_PROFILE'`. Expected: `3JP75Z3F98 dev.badcoffee.jaw driftstation-notarize`. Then `mise ls --current`: tuist 4.197.3, swiftformat 0.62.1 and swiftlint 0.65.0, each from `~/src/jaw-mise/mise.toml`. Then `(cd mac && mise exec -- tuist version)`: `4.197.3`.
- [ ] **Step 7: On the build Mac.** `mac/scripts/build-mac build JawKit`. Expected: `** BUILD SUCCEEDED **`. If it says the build Mac doesn't answer, run `(cd mac && mise exec -- tuist generate --no-open)` first and then again, which builds here.
- [ ] **Step 8: Commit.**

```bash
git add mise.toml .gitignore mac/scripts/bundle mac/scripts/build-mac
git commit -q -m "One mise.toml at the root, as Redlamp's, in place of mac/mise.toml (the move to mise's design): the Mac's Tuist, SwiftFormat and SwiftLint, marked macOS only so Linux's builds and CI skip them, and the team, bundle and notary settings; .mise.local.toml ignored for a machine's own; and bundle's build here and build-mac's on the build Mac trust the root's config before mise exec runs Tuist, which mise refuses in a checkout it hasn't been told to trust"
```

---

### Task 3: `engine`, `rust` and `build`

**Files:** Create `mise/tasks/engine`, `mise/tasks/rust`, `mise/tasks/build`.

**Interfaces:** Produces `mise/tasks/engine` (writes `target/engine/jaw-engine`), which `build`, `test`, `perf`, `install` and `mac/scripts/bundle` run as a script.

- [ ] **Step 1: See it missing.** `mise run engine`. Expected: mise says there's no task named `engine`.
- [ ] **Step 2: Write `mise/tasks/engine`:**

```bash
#!/usr/bin/env bash
#MISE description="jaw-engine in target/engine/, stamped with the version jaw and jawd say (scripts/version)"
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
version="$(sh scripts/version)"
cd engine
"${GO:-go}" build -trimpath -ldflags "-X main.version=$version" -o ../target/engine/jaw-engine ./cmd/jaw-engine
```

- [ ] **Step 3: Write `mise/tasks/rust`:**

```bash
#!/usr/bin/env bash
#MISE description="The Rust workspace for debug: target/debug/jawd, target/debug/jaw and the rest"
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
"${CARGO:-cargo}" build --workspace
```

- [ ] **Step 4: Write `mise/tasks/build`:**

```bash
#!/usr/bin/env bash
#MISE description="jaw-engine, then the Rust workspace: target/engine/jaw-engine, target/debug/jawd and target/debug/jaw"
#MISE alias="b"
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
"$here/engine"
"$here/rust"
```

- [ ] **Step 5:** `chmod +x mise/tasks/engine mise/tasks/rust mise/tasks/build`.
- [ ] **Step 6: Run them.** `export CARGO_INCREMENTAL=0 && df -h /System/Volumes/Data && mise run b`. Expected: it ends with cargo's `Finished`. Then `target/engine/jaw-engine --version` says `jaw-engine $(sh scripts/version) (whatsmeow …)`, and `target/debug/jawd --version` says `jawd $(sh scripts/version)`.
- [ ] **Step 7: From a subfolder, and without mise.** `(cd mac && mise run engine)` succeeds. Then `rm target/engine/jaw-engine && env PATH="$NOMISE" mise/tasks/build && test -x target/engine/jaw-engine && echo built`. Expected: `built`.
- [ ] **Step 8: Commit.**

```bash
git add mise/tasks/engine mise/tasks/rust mise/tasks/build
git commit -q -m "mise run engine, rust and build (b), as make's targets were: jaw-engine stamped with scripts/version's version, then the workspace for debug; each a bash file in mise/tasks/ that finds the repository from its own path, so it runs from any folder and without mise"
```

---

### Task 4: `test`, `lint` and `fmt`

**Files:** Create `mise/tasks/test`, `mise/tasks/lint`, `mise/tasks/fmt`.

**Interfaces:** Consumes `mise/tasks/engine`. Produces `mise run test`, `mise run lint` and `mise run fmt`, which CI (Task 11) and the gate (Task 15) run.

- [ ] **Step 1: See it missing.** `mise run lint`. Expected: no task named `lint`.
- [ ] **Step 2: Write `mise/tasks/test`:**

```bash
#!/usr/bin/env bash
#MISE description="jaw-engine, then Go's tests with -race and the Rust workspace's"
#MISE alias="t"
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here/../.."
"$here/engine"
(cd engine && "${GO:-go}" test -race ./...)
"${CARGO:-cargo}" test --workspace
```

- [ ] **Step 3: Write `mise/tasks/lint`:**

```bash
#!/usr/bin/env bash
#MISE description="go vet, gofmt, rustfmt, and clippy with warnings as errors"
#MISE alias="l"
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
(cd engine && "${GO:-go}" vet ./...)
unformatted="$(gofmt -l engine)"
if [ -n "$unformatted" ]; then
    echo "$unformatted"
    echo "gofmt: the files above need formatting"
    exit 1
fi
"${CARGO:-cargo}" fmt --all --check
"${CARGO:-cargo}" clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 4: Write `mise/tasks/fmt`:**

```bash
#!/usr/bin/env bash
#MISE description="Format the engine with gofmt and the Rust workspace with rustfmt"
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
gofmt -w engine
"${CARGO:-cargo}" fmt --all
```

- [ ] **Step 5:** `chmod +x mise/tasks/test mise/tasks/lint mise/tasks/fmt`.
- [ ] **Step 6: Run them.** `mise run fmt && git status --porcelain -- crates engine`. Expected: nothing (main is formatted). `mise run l`. Expected: exit 0; if it fails, `make lint` must fail the same way, and the report says so. `mise run t`. Expected: Go's `ok` lines and cargo's `test result: ok` for every crate. If a test fails, run it again alone (`cargo test -p <crate> <name> -- --exact`). If it passes alone and it's a timing test the audits list as flaky (`docs/audits/2026-10-30-flakes-data.md`, `docs/audits/2026-11-03-voice-flakes.md`, `docs/audits/2026-11-05-idle-wakeups.md`'s O5), note it. Otherwise stop and report it with its output.
- [ ] **Step 7: Commit.**

```bash
git add mise/tasks/test mise/tasks/lint mise/tasks/fmt
git commit -q -m "mise run test (t), lint (l) and fmt, as make's: the engine first, then Go's tests with -race and the workspace's; go vet, gofmt, rustfmt and clippy with warnings as errors; and gofmt and rustfmt writing"
```

---

### Task 5: `proto`, `sync-protos` and `check-generated`

**Files:** Create `mise/tasks/proto`, `mise/tasks/sync-protos`, `mise/tasks/check-generated`.

**Interfaces:** Produces `mise/tasks/proto`, which `check-generated` runs as a script, and `mise run check-generated`, which CI and the gate run.

- [ ] **Step 1: See it missing.** `mise run check-generated`. Expected: no task named `check-generated`.
- [ ] **Step 2: Write `mise/tasks/proto`:**

```bash
#!/usr/bin/env bash
#MISE description="The engine's Go code from proto/engine, with buf v1.74.0 and protoc-gen-go at the version engine/go.mod pins"
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
go="${GO:-go}"
# protoc-gen-go is built again whenever engine/go.mod is newer than it.
if [ ! -x target/tools/protoc-gen-go ] || [ engine/go.mod -nt target/tools/protoc-gen-go ]; then
    (cd engine && "$go" build -o ../target/tools/protoc-gen-go google.golang.org/protobuf/cmd/protoc-gen-go)
fi
"$go" run github.com/bufbuild/buf/cmd/buf@v1.74.0 generate proto/engine --template proto/engine/buf.gen.yaml
```

- [ ] **Step 3: Write `mise/tasks/sync-protos`:**

```bash
#!/usr/bin/env bash
#MISE description="Copy the .proto files whatsmeow ships into proto/whatsapp/, from the version engine/go.mod pins (scripts/sync-protos.py; -- --check only checks)"
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
exec python3 scripts/sync-protos.py "$@"
```

- [ ] **Step 4: Write `mise/tasks/check-generated`:**

```bash
#!/usr/bin/env bash
#MISE description="Generate the engine's code again, and fail if it or proto/whatsapp/ differs from what's committed"
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here/../.."
"$here/proto"
python3 scripts/sync-protos.py --check
git diff --exit-code -- engine/internal/link/enginepb
```

- [ ] **Step 5:** `chmod +x mise/tasks/proto mise/tasks/sync-protos mise/tasks/check-generated`.
- [ ] **Step 6: Run them.** `mise run check-generated && echo same`. Expected: `same`. `mise run sync-protos -- --check`: exit 0. `touch engine/go.mod && mise run proto && git status --porcelain`: nothing changed, protoc-gen-go built again.
- [ ] **Step 7: Commit.**

```bash
git add mise/tasks/proto mise/tasks/sync-protos mise/tasks/check-generated
git commit -q -m "mise run proto, sync-protos and check-generated, as make's: protoc-gen-go at engine/go.mod's version, built again when go.mod is newer, and buf v1.74.0; scripts/sync-protos.py with its arguments; and the check that the generated code and proto/whatsapp/ are what's committed"
```

---

### Task 6: `check-audit`, `check-parity`, `prune-builds`, `clean` and `perf`

**Files:** Create `mise/tasks/check-audit`, `check-parity`, `prune-builds`, `clean`, `perf`.

**Interfaces:** Consumes `mise/tasks/engine` (in `perf`). Produces the commands the rules name: `mise run check-audit` and `mise run prune-builds`.

- [ ] **Step 1: See it missing.** `mise run check-audit`. Expected: no task named `check-audit`.
- [ ] **Step 2: Write `mise/tasks/check-audit`:**

```bash
#!/usr/bin/env bash
#MISE description="Every commit since origin/main that touches the data layer names an approved audit (scripts/check-data-audit.py)"
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
exec python3 scripts/check-data-audit.py origin/main HEAD
```

- [ ] **Step 3: Write `mise/tasks/check-parity`:**

```bash
#!/usr/bin/env bash
#MISE description="Every request and push the terminal uses and the Mac doesn't has a step of the parity plan in mac/parity.toml (-- --terminal <branch> reads a branch before it lands; -- --table lists each with each side's use)"
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
exec python3 scripts/check-parity.py "$@"
```

- [ ] **Step 4: Write `mise/tasks/prune-builds`:**

```bash
#!/usr/bin/env bash
#MISE description="Delete from every worktree's target/ the object files no binary names any more; nothing rebuilds after (scripts/prune-builds.py; -- --dry-run only counts)"
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
exec python3 scripts/prune-builds.py "$@"
```

- [ ] **Step 5: Write `mise/tasks/clean`:**

```bash
#!/usr/bin/env bash
#MISE description="cargo clean: this checkout's whole target/, which costs it a full build (mise run prune-builds frees most of it without one)"
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
"${CARGO:-cargo}" clean
```

- [ ] **Step 6: Write `mise/tasks/perf`:**

```bash
#!/usr/bin/env bash
#MISE description="Every budget in the design, on release builds against a made-up million-message account, as one table in target/perf/ (scripts/perf.py says how; -- --quick leaves out the slowest, -- --small checks the harness)"
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here/../.."
"$here/engine"
"${CARGO:-cargo}" build --release -p jawd -p jaw
"${CARGO:-cargo}" test --release -p jawd --bin jawd --no-run
python3 scripts/perf.py "$@"
```

- [ ] **Step 7:** `chmod +x mise/tasks/check-audit mise/tasks/check-parity mise/tasks/prune-builds mise/tasks/clean mise/tasks/perf`.
- [ ] **Step 8: Run them.** `mise run check-audit && echo audited`. Expected: `audited`. `mise run check-parity`: exit 0, ending "The Mac uses what the terminal doesn't: …". `mise run check-parity -- --table`: the table of requests and pushes. `mise run prune-builds -- --dry-run`: counts, nothing deleted. `mise run perf -- --small`: a table in `target/perf/`, and exit 1 with only "photo out" and "steady" failing, as `make perf` fails on main (found on 6 November: main's media-out bench, `crates/jawd/src/media/outgoing/bench.rs`, says hello with protocol 1, which jawd has refused since 2 November, and steady's quiet 15 s never comes); any other failing step is this branch's to explain. `clean` waits for Task 18.
- [ ] **Step 9: Commit.**

```bash
git add mise/tasks/check-audit mise/tasks/check-parity mise/tasks/prune-builds mise/tasks/clean mise/tasks/perf
git commit -q -m "mise run check-audit, check-parity, prune-builds, clean and perf, as make's, with arguments after -- where make took ARGS= (check-parity -- --table, prune-builds -- --dry-run, perf -- --quick or --small)"
```

---

### Task 7: `setup`, the hook

**Files:** Create `mise/tasks/setup`.

**Interfaces:** Produces `mise run setup`, which the rules and `docs/first-run.md` name (Task 14).

- [ ] **Step 1: See it missing.** `mise run setup`. Expected: no task named `setup`.
- [ ] **Step 2: Write `mise/tasks/setup`:**

```bash
#!/usr/bin/env bash
#MISE description="One-time setup for a clone: the pre-push hook, in the hooks folder every worktree shares, which refuses a push to main whose data-layer commits name no approved audit"
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
hooks="$(git rev-parse --git-common-dir)/hooks"
mkdir -p "$hooks"
install -m 755 scripts/pre-push "$hooks/pre-push"
echo "==> $hooks/pre-push installed"
```

- [ ] **Step 3:** `chmod +x mise/tasks/setup && git add mise/tasks/setup && git commit -q -m "wip: setup"` (amended in Step 5), so a clone has it.
- [ ] **Step 4: Run it in a scratch clone.**

```bash
git clone -q --branch mise ~/src/jaw /tmp/jaw-mise/setup-clone
cd /tmp/jaw-mise/setup-clone && mise run setup && cmp scripts/pre-push .git/hooks/pre-push && echo same
cd ~/src/jaw-mise && rm -rf /tmp/jaw-mise/setup-clone
```

Expected: `==> .git/hooks/pre-push installed`, then `same`.
- [ ] **Step 5: Commit.**

```bash
git commit -q --amend -m "mise run setup, the one-time setup for a clone as Redlamp's is, in place of make hooks: scripts/pre-push into the hooks folder every worktree shares, as before, rather than core.hooksPath, which would point a worktree at a commit from before the hook at no hook at all"
```

---

### Task 8: The Mac's tasks, and what jawd is built from

**Files:** Create `mise/tasks/mac`, `mise/tasks/mac-test`. Modify `mac/scripts/bundle`, `mac/scripts/built-from`, `mac/scripts/test-built-from`.

**Interfaces:** Consumes `mise/tasks/engine` (from `bundle`). Produces `built-from`'s `changed` for an uncommitted change to any of mise's tasks or settings.

- [ ] **Step 1: The failing test.** In `mac/scripts/test-built-from`:
  - add `mise/tasks` to the end of `mkdir -p crates engine proto protocol mac/scripts docs scripts`;
  - replace `echo 'engine:' > Makefile` with two lines, `echo '[tools]' > mise.toml` and `echo 'go build' > mise/tasks/engine`;
  - in `aged()`, replace `protocol Makefile scripts/version` with `protocol mise.toml mise scripts/version`;
  - replace `for new in .cargo/config.toml rust-toolchain rust-toolchain.toml go.work GNUmakefile; do` with `for new in .cargo/config.toml rust-toolchain rust-toolchain.toml go.work mise.local.toml .mise.toml .mise.local.toml .mise/config.toml .config/mise.toml .config/mise/config.toml .tool-versions mise/tasks/new; do`;
  - replace `for input in Makefile scripts/version VERSION; do` with `for input in mise.toml mise/tasks/engine scripts/version VERSION; do`;
  - in the comment above `refuses()`, replace `printing nothing for make mac to record` with `printing nothing for mise run mac to record`.
- [ ] **Step 2: See it fail.** `mac/scripts/test-built-from`. Expected: `FAIL` for "a new mise.local.toml" (and each of mise's other kinds) and for "mise.toml changed", which say `main` where `changed` is wanted.
- [ ] **Step 3: Count mise's files.** In `mac/scripts/built-from`, replace

```sh
inputs="$inputs Makefile GNUmakefile makefile scripts/version VERSION"
```

with

```sh
inputs="$inputs mise.toml mise.local.toml .mise.toml .mise.local.toml mise .mise .config/mise.toml .config/mise"
inputs="$inputs .tool-versions scripts/version VERSION"
```

and in its header replace `the makefiles and what` with `mise's tasks and settings and what`, `as \`make mac\` records it` with `as \`mise run mac\` records it`, `so make mac records nothing` with `so mise run mac records nothing`, and `so make mac sees a change` with `so mise run mac sees a change`.
- [ ] **Step 4: See it pass.** `mac/scripts/test-built-from`. Expected: every line `ok:`, none `FAIL`.
- [ ] **Step 5: Write `mise/tasks/mac`:**

```bash
#!/usr/bin/env bash
#MISE description="Jaw.app with jawd, jaw-engine and jaw inside, each signed, saying what it was built from, in mac/build/release/ (mac/scripts/bundle)"
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
exec mac/scripts/bundle
```

- [ ] **Step 6: Write `mise/tasks/mac-test`:**

```bash
#!/usr/bin/env bash
#MISE description="mac/scripts/test-built-from, jawd's debug build, then JawKit, JawThemes, JawUITests and JawHarnessTests, on the build Mac when it answers (mac/scripts/build-mac)"
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
mac/scripts/test-built-from
"${CARGO:-cargo}" build -p jawd --bin jawd
for scheme in JawKit JawThemes JawUITests JawHarnessTests; do
    mac/scripts/build-mac test "$scheme"
done
```

- [ ] **Step 7: bundle runs the engine's task.** In `mac/scripts/bundle`, replace `make -s engine` with `mise/tasks/engine`; in its header, `# \`make mac\`: Jaw.app` with `# \`mise run mac\`: Jaw.app` and `jaw-engine by make engine, which stamps it` with `jaw-engine by mise/tasks/engine, which stamps it`; and `echo "==> the checkout changed while make mac built it: run it again" >&2` with `echo "==> the checkout changed while mise run mac built it: run it again" >&2`.
- [ ] **Step 8:** `chmod +x mise/tasks/mac mise/tasks/mac-test`, then commit.

```bash
git add mise/tasks/mac mise/tasks/mac-test mac/scripts/bundle mac/scripts/built-from mac/scripts/test-built-from
git commit -q -m "mise run mac and mac-test, as make's, with bundle running mise/tasks/engine; and built-from counts mise's tasks and settings among what jawd is built from, where it counted the makefiles: mise.toml, mise.local.toml, .mise.toml, .mise.local.toml, mise/, .mise/, .config/mise.toml, .config/mise/ and .tool-versions, any of which can change the engine's build or the environment mise run mac builds in, so an uncommitted change to one, a .mise.local.toml that git ignores included, makes a Jaw changed and its jawd never starts on an account's folder. test-built-from's checkout, new files and changed inputs follow, failing with the makefiles still counted"
```

- [ ] **Step 9: Run them.** `mise run mac`. Expected: it ends `==> mac/build/release/Jaw.app (<size>), jawd <version>, built from mise at <commit>, not on origin/main`. `codesign -dvv mac/build/release/Jaw.app 2>&1 | grep -E 'Authority=Developer ID Application|flags='` shows the Developer ID and `runtime`, and `plutil -extract JawBuiltFrom raw mac/build/release/Jaw.app/Contents/Info.plist` says `branch`. Then `mise run mac-test`: test-built-from's `ok:` lines, then `** TEST SUCCEEDED **` for each of the four schemes.

---

### Task 9: `install` and `uninstall`

**Files:** Create `mise/tasks/install`, `mise/tasks/uninstall`.

**Interfaces:** Consumes `mise/tasks/engine`. Produces `mise/tasks/install`, which the formula (Task 10) runs with `PREFIX` set, and which runs without mise.

- [ ] **Step 1: See it missing.** `PREFIX=/tmp/jaw-mise/prefix mise run install`. Expected: no task named `install`.
- [ ] **Step 2: Write `mise/tasks/install`:**

```bash
#!/usr/bin/env bash
#MISE description="jaw, jawd and jaw-engine built for release and installed: jaw and jawd in PREFIX/bin, jaw-engine in PREFIX/libexec/jaw (PREFIX is /usr/local unless set; sudo only for folders you can't write)"
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here/../.."
destdir="${DESTDIR:-}"
prefix="${PREFIX:-/usr/local}"
bindir="$destdir${BINDIR:-$prefix/bin}"
libexecdir="$destdir${LIBEXECDIR:-$prefix/libexec/jaw}"
# As root, as with sudo, it builds nothing: cargo and go would start again in root's caches, without
# the crates and modules you've downloaded.
if [ "$(id -u)" != 0 ]; then
    "${CARGO:-cargo}" build --release --locked -p jaw -p jawd
    "$here/engine"
fi
for built in target/release/jaw target/release/jawd target/engine/jaw-engine; do
    [ -x "$built" ] || { echo "install: nothing to install yet; run mise run install as yourself" >&2; exit 1; }
done
copy() {
    folder="$1"
    shift
    if mkdir -p "$folder" 2>/dev/null && [ -w "$folder" ]; then
        install -m 755 "$@" "$folder"
    else
        sudo mkdir -p "$folder"
        sudo install -m 755 "$@" "$folder"
    fi
}
copy "$bindir" target/release/jaw target/release/jawd
copy "$libexecdir" target/engine/jaw-engine
echo "==> jaw and jawd in $bindir, jaw-engine in $libexecdir"
```

- [ ] **Step 3: Write `mise/tasks/uninstall`:**

```bash
#!/usr/bin/env bash
#MISE description="Take out what mise run install put in, with the same PREFIX (sudo only for folders you can't write)"
set -euo pipefail
destdir="${DESTDIR:-}"
prefix="${PREFIX:-/usr/local}"
bindir="$destdir${BINDIR:-$prefix/bin}"
libexecdir="$destdir${LIBEXECDIR:-$prefix/libexec/jaw}"
as_needed() {
    folder="$1"
    shift
    if [ -w "$folder" ]; then "$@"; else sudo "$@"; fi
}
if [ -d "$bindir" ]; then
    as_needed "$bindir" rm -f "$bindir/jaw" "$bindir/jawd"
fi
if [ -d "$libexecdir" ]; then
    as_needed "$libexecdir" rm -f "$libexecdir/jaw-engine"
    if [ -z "$(ls -A "$libexecdir")" ]; then
        as_needed "$(dirname "$libexecdir")" rmdir "$libexecdir"
    fi
fi
echo "==> jaw and jawd out of $bindir, jaw-engine out of $libexecdir"
```

- [ ] **Step 4:** `chmod +x mise/tasks/install mise/tasks/uninstall`.
- [ ] **Step 5: Install through mise, then the formula's test by hand.**

```bash
P=/tmp/jaw-mise/prefix; H=/tmp/jaw-mise/home; v="$(sh scripts/version)"
PREFIX="$P" mise run install
"$P/bin/jaw" --version; "$P/bin/jawd" --version; "$P/libexec/jaw/jaw-engine" --version
JAW_HOME="$H" "$P/bin/jawd" --engine demo --no-notify > /tmp/jaw-mise/jawd.log 2>&1 &
pid=$!
for _ in $(seq 30); do sleep 1; JAW_HOME="$H" "$P/bin/jaw" snapshot --width 120 --height 30 2>&1 | grep -q 'Lisbon Climbing' && break; done
JAW_HOME="$H" "$P/bin/jaw" snapshot --width 120 --height 30 | grep -c 'Lisbon Climbing'
JAW_HOME="$H" "$P/bin/jaw" status > /tmp/jaw-mise/status.txt
grep -E "^jawd +$v \(protocol " /tmp/jaw-mise/status.txt && ! grep -q 'another build' /tmp/jaw-mise/status.txt && echo "status ok"
kill -TERM "$pid"; wait "$pid" || true
```

Expected: `==> jaw and jawd in /tmp/jaw-mise/prefix/bin, jaw-engine in /tmp/jaw-mise/prefix/libexec/jaw`; `jaw $v`, `jawd $v` and `jaw-engine $v (whatsmeow …)`; a count of 1 or more; status's line `jawd  $v (protocol …)`, then `status ok`. `jaw snapshot` and `jaw status` only connect, so neither starts a jawd of its own.
- [ ] **Step 6: Uninstall.** `PREFIX=/tmp/jaw-mise/prefix mise run uninstall && find /tmp/jaw-mise/prefix -type f | wc -l && test ! -e /tmp/jaw-mise/prefix/libexec/jaw && echo gone`. Expected: `0`, then `gone`.
- [ ] **Step 7: Without mise, and staged.** `env PATH="$NOMISE" PREFIX=/tmp/jaw-mise/prefix2 mise/tasks/install && /tmp/jaw-mise/prefix2/bin/jaw --version` says `jaw $v`. `DESTDIR=/tmp/jaw-mise/stage PREFIX=/usr/local mise run install` puts the three in `/tmp/jaw-mise/stage/usr/local/bin` and `/tmp/jaw-mise/stage/usr/local/libexec/jaw` without asking for sudo, and `DESTDIR=/tmp/jaw-mise/stage PREFIX=/usr/local mise run uninstall` takes them out. Then `rm -rf /tmp/jaw-mise/prefix /tmp/jaw-mise/prefix2 /tmp/jaw-mise/stage /tmp/jaw-mise/home /tmp/jaw-mise/jawd.log /tmp/jaw-mise/status.txt`.
- [ ] **Step 8: Commit.**

```bash
git add mise/tasks/install mise/tasks/uninstall
git commit -q -m "mise run install and uninstall, in place of make release, make install and make uninstall, with make's PREFIX, BINDIR, LIBEXECDIR and DESTDIR: install builds jaw, jawd and the engine for release as you, then copies them, with sudo, which asks once, only into a folder you can't write, so one command installs into /usr/local; as root it installs what's built and builds nothing, as make install did; and uninstall takes them out with sudo where it needs it. Both run as plain scripts, as Homebrew will run install"
```

---

### Task 10: `release`, and the formula

**Files:** Move `scripts/release.sh` to `mise/tasks/release`. Modify `packaging/homebrew/jaw.rb`, `mac/scripts/test-built-from`.

**Interfaces:** Consumes `mise/tasks/install` (in the formula). Produces `mise run release`, which the Mac release's design extends.

- [ ] **Step 1: Move it.** `git mv scripts/release.sh mise/tasks/release`.
- [ ] **Step 2: Make it a task.** In `mise/tasks/release`:
  - replace `#!/bin/sh` with the two lines `#!/usr/bin/env bash` and `#MISE description="A release from a clean tree at a tag like v0.1.0: its tarball, sha256 and Homebrew formula in target/dist/, with the commands that publish them, printed for you to run"`;
  - replace `#     scripts/release.sh` with `#     mise run release`;
  - replace `set -eu` with `set -euo pipefail`;
  - replace `root=$(cd "$(dirname "$0")/.." && pwd)` with `root=$(cd "$(dirname "$0")/../.." && pwd)`;
  - replace `echo "release.sh: $*" >&2` with `echo "release: $*" >&2`.
- [ ] **Step 3: The formula runs the install task.** In `packaging/homebrew/jaw.rb`, replace `# jaw's Homebrew formula, for Formula/jaw.rb in its tap. scripts/release.sh fills in url and sha256` with `# jaw's Homebrew formula, for Formula/jaw.rb in its tap. mise run release fills in url and sha256`, and

```ruby
  def install
    system "make", "install", "PREFIX=#{prefix}"
  end
```

with

```ruby
  def install
    ENV["PREFIX"] = prefix.to_s
    system "mise/tasks/install"
  end
```

  Then `ruby -c packaging/homebrew/jaw.rb`. Expected: `Syntax OK`.
- [ ] **Step 4:** In `mac/scripts/test-built-from`, replace `as scripts/release.sh has it made` with `as mise run release has it made`, and commit.

```bash
git add -A mise/tasks/release packaging/homebrew/jaw.rb mac/scripts/test-built-from
git commit -q -m "mise run release, as Redlamp's release is, in place of scripts/release.sh, moved as it was: the tarball, its sha256 and the formula in target/dist/, then the commands that publish them, printed for you to run; and the formula runs mise/tasks/install with PREFIX set to its prefix, so Homebrew needs no mise, and with it no Tuist, and lays out the same files for its test"
```

- [ ] **Step 5: A release from a scratch clone, built as Homebrew would.**

```bash
git clone -q --branch mise ~/src/jaw /tmp/jaw-mise/release-clone && cd /tmp/jaw-mise/release-clone
printf '# jaw 0.0.0-mise\n\nA scratch release, to check mise run release.\n' > docs/releases/0.0.0-mise.md
git add docs/releases/0.0.0-mise.md && git -c user.name=check -c user.email=check@localhost commit -q -m 'scratch notes'
git -c user.name=check -c user.email=check@localhost tag -a v0.0.0-mise -m scratch
mise run release
mkdir -p /tmp/jaw-mise/unpacked && tar -xzf target/dist/jaw-0.0.0-mise.tar.gz -C /tmp/jaw-mise/unpacked
cd /tmp/jaw-mise/unpacked/jaw-0.0.0-mise
env PATH="$NOMISE" PREFIX=/tmp/jaw-mise/tarball-prefix CARGO_INCREMENTAL=0 mise/tasks/install
/tmp/jaw-mise/tarball-prefix/bin/jaw --version
grep -n 'mise/tasks/install' /tmp/jaw-mise/release-clone/target/dist/jaw.rb
cd ~/src/jaw-mise && rm -rf /tmp/jaw-mise/release-clone /tmp/jaw-mise/unpacked /tmp/jaw-mise/tarball-prefix
```

Expected: the release prints `…/target/dist/jaw-0.0.0-mise.tar.gz`, `sha256` and 64 hex digits, the formula with `url "https://github.com/pdcgomes/jaw/releases/download/v0.0.0-mise/jaw-0.0.0-mise.tar.gz"`, and "Nothing has been pushed or published"; the install ends `==> jaw and jawd in /tmp/jaw-mise/tarball-prefix/bin, …`; `jaw 0.0.0-mise`; and the formula's line with `system "mise/tasks/install"`.

---

### Task 11: CI

**Files:** Modify `.github/workflows/ci.yml`.

- [ ] **Step 1:** In the test job, replace

```yaml
      - run: make lint
      - run: make test
      - run: make check-generated
```

with

```yaml
      - uses: jdx/mise-action@v2
      - run: mise run lint
      - run: mise run test
      - run: mise run check-generated
```

- [ ] **Step 2: It parses, and names no make.** `python3 -c 'import yaml; yaml.safe_load(open(".github/workflows/ci.yml")); print("parses")' && ! grep -n 'make ' .github/workflows/ci.yml && echo "no make"`. Expected: `parses`, then `no make`. CI runs it on GitHub in Task 17.
- [ ] **Step 3: Commit.**

```bash
git add .github/workflows/ci.yml
git commit -q -m "CI runs mise run lint, test and check-generated where it ran make's, installing mise with jdx/mise-action as Redlamp's CI does; its Go and Rust setup stays, and on Linux mise installs nothing, the Mac's tools being macOS only"
```

---

### Task 12: Messages and comments outside the data layer

**Files:** Modify `crates/jaw/src/wire.rs`, `agent.rs`, `bench.rs`; `crates/jaw-proto/src/local.rs`; `crates/jawd/build.rs`, `crates/jawd/src/demo/mod.rs`, `crates/jawd/tests/installed.rs`; `mac/JawKit/Sources/Client/Starter.swift`, `Build.swift`; `mac/Tests/JawKitTests/StarterTests.swift`, `BuildTests.swift`; `scripts/perf.py`, `scripts/prune-builds.py`.

- [ ] **Step 1: The Swift tests first.** In `StarterTests.swift`, replace `"there's no jawd inside this Jaw: make mac puts one in its Contents/Helpers"` with `"there's no jawd inside this Jaw: mise run mac puts one in its Contents/Helpers"` and `"there's no jawd to run: make build builds it"` with `"there's no jawd to run: mise run build builds it"`. In `BuildTests.swift`, replace `"this Jaw doesn't say what it was built from, as make mac's builds do, so it doesn't start jawd"` with `"this Jaw doesn't say what it was built from, as mise run mac's builds do, so it doesn't start jawd"`.
- [ ] **Step 2: See them fail.** `mac/scripts/build-mac test JawKit`. Expected: the three expectations fail, quoting the old messages.
- [ ] **Step 3: The app's messages.** In `Starter.swift`, the same two replacements in the strings returned. In `Build.swift`, `"this Jaw doesn't say what it was built from, as make mac's builds do"` becomes `"this Jaw doesn't say what it was built from, as mise run mac's builds do"`; in its comments, `as \`make mac\` records it` becomes `as \`mise run mac\` records it`, and `as one \`make mac\`` becomes `as one \`mise run mac\``.
- [ ] **Step 4: See them pass.** `mac/scripts/build-mac test JawKit`. Expected: `** TEST SUCCEEDED **`.
- [ ] **Step 5: The terminal's messages and comments,** each replacing only the words shown:
  - `crates/jaw/src/wire.rs`: `"can't start jawd: {e}; make build builds it"` becomes `"can't start jawd: {e}; mise run build builds it"`; in the comment on where jawd is, `as \`make install\`, Homebrew and cargo put them` becomes `as \`mise run install\`, Homebrew and cargo put them`.
  - `crates/jaw/src/agent.rs`: `"jawd isn't beside jaw in {folder}; make build puts them together"` becomes `"jawd isn't beside jaw in {folder}; mise run build puts them together"`.
  - `crates/jaw/src/bench.rs`: `` `make perf` reads it`` becomes `` `mise run perf` reads it``, and `` `make perf`'s, on release builds`` becomes `` `mise run perf`'s, on release builds``.
  - `crates/jaw-proto/src/local.rs`: `` as `make install`, cargo and Jaw.app's`` becomes `` as `mise run install`, cargo and Jaw.app's``.
  - `crates/jawd/build.rs`: `` by `make install`, Homebrew or `make mac` `` becomes `` by `mise run install`, Homebrew or `mise run mac` ``.
  - `crates/jawd/src/demo/mod.rs`: `` as JSON for `make perf` `` becomes `` as JSON for `mise run perf` ``, and `` Timings as `make perf` reads them`` becomes `` Timings as `mise run perf` reads them``.
  - `crates/jawd/tests/installed.rs`: `` jawd as `make install` and Homebrew lay it out`` becomes `` jawd as `mise run install` and Homebrew lay it out``.
  - `scripts/perf.py`: `` table (`make perf`).`` becomes `` table (`mise run perf`).``, and `isn't built; make perf builds it` becomes `isn't built; mise run perf builds it`.
  - `scripts/prune-builds.py`: `` (`make prune-builds`)`` becomes `` (`mise run prune-builds`)``.
- [ ] **Step 6: Build and check.** `mise run fmt && cargo build -p jaw -p jawd && python3 -m py_compile scripts/perf.py scripts/prune-builds.py && mise run l`. Expected: each succeeds; `mise run fmt` may rewrap a lengthened line, which goes in the commit.
- [ ] **Step 7: Commit.**

```bash
git add crates/jaw crates/jaw-proto crates/jawd/build.rs crates/jawd/src/demo crates/jawd/tests/installed.rs mac/JawKit mac/Tests scripts/perf.py scripts/prune-builds.py
git commit -q -m "The messages that tell you to run make name the mise task, in jaw, the Mac app and scripts/perf.py, with the Mac's tests that quote them: mise run build builds it, mise run build puts them together, mise run mac puts one in its Contents/Helpers, as mise run mac's builds do, mise run perf builds it; and the comments outside the data layer that name make's targets name mise's"
```

---

### Task 13: The data layer's messages and comments

**Files:** Modify `crates/jawd/src/engine/go.rs`, `crates/jawd/src/main.rs`, `crates/jawd/src/sock.rs`, `crates/jawd/src/store/write/outbox.rs`, `engine/cmd/jaw-engine/main.go`, `proto/engine/engine.proto`; regenerate `engine/internal/link/enginepb/engine.pb.go`; modify `protocol/fixtures/error.version.json`.

- [ ] **Step 1: The replacements,** each only the words shown:
  - `crates/jawd/src/engine/go.rs`: `` beside jawd as `make install` puts it`` becomes `` beside jawd as `mise run install` puts it``; `rebuild them together with make build` becomes `rebuild them together with mise run build`; `run make sync-protos and make build` becomes `run mise run sync-protos, then mise run build`; `` The engine `make engine` builds`` becomes `` The engine `mise run engine` builds`` (twice); `` where `make test` builds it first`` becomes `` where `mise run test` builds it first``; `"make test builds jaw-engine before the Rust tests"` becomes `"mise run test builds jaw-engine before the Rust tests"`; `isn't built; make engine builds it` becomes `isn't built; mise run engine builds it`.
  - `crates/jawd/src/main.rs`: `or on PATH; make build builds it` becomes `or on PATH; mise run build builds it`; `` as `make install` and Homebrew put it`` becomes `` as `mise run install` and Homebrew put it``.
  - `crates/jawd/src/sock.rs`: `so run make install again` becomes `so install jaw again with mise run install`.
  - `crates/jawd/src/store/write/outbox.rs`: `` // `make perf` reads this line.`` becomes `` // `mise run perf` reads this line.``
  - `engine/cmd/jaw-engine/main.go`: `from git describe: make engine sets it.` becomes `from git describe: mise run engine sets it.`
  - `proto/engine/engine.proto`: `` Go's code is generated by `make proto` and committed`` becomes `` Go's code is generated by `mise run proto` and committed``.
  - `protocol/fixtures/error.version.json`: `this interface is older, so run make install again` becomes `this interface is older, so install jaw again with mise run install`.
- [ ] **Step 2: Generate and check.** `mise run proto && git diff --stat -- engine/internal/link/enginepb`. Expected: `engine.pb.go` changes in its header comment alone. Then `mise run fmt && cargo build -p jawd && cargo test -p jawd --bin jawd engine::go`. Expected: `test result: ok` (the engine's tests pass, or skip saying `mise run engine builds it` where the engine isn't built).
- [ ] **Step 3: Commit,** as a commit of its own, so the database expert audits it and its trailer names the note (Task 16).

```bash
git add crates/jawd/src/engine/go.rs crates/jawd/src/main.rs crates/jawd/src/sock.rs crates/jawd/src/store/write/outbox.rs engine/cmd/jaw-engine/main.go proto/engine/engine.proto engine/internal/link/enginepb/engine.pb.go protocol/fixtures/error.version.json
git commit -q -m "The data layer's messages and comments that name make's targets name mise's (the move to mise's design, The data layer): jawd's engine says to rebuild with mise run build and to run mise run sync-protos, then mise run build, jawd says mise run build builds the engine it can't find and, to an older interface, to install jaw again with mise run install, as the protocol's fixture of that error now says, and the comments in jawd's engine, main, outbox and the engine's main.go and engine.proto, with the Go generated from it, name mise run engine, install, test, perf and proto. Text only: nothing that writes, acknowledges or reads data changes"
git diff --exit-code HEAD -- engine/internal/link/enginepb && mise run check-generated && echo generated
```

Expected: `generated`.

---

### Task 14: The documents and rules, and the Makefile deleted

**Files:** Modify `README.md`, `docs/first-run.md`, `mac/README.md`, `protocol/README.md`, `marketing/jaw-in-a-minute/README.md`, `marketing/jaw-in-a-minute/jaw.py`, `docs/plans/2026-10-16-jaw-design.md`, `docs/plans/2026-10-26-jaw-mac-design.md`, `.cursor/rules/data-integrity.mdc`, `.cursor/rules/disk-space.mdc`, `mac/Project.swift`, `mac/parity.toml`. Delete `Makefile`.

- [ ] **Step 1: README.md's install.** Replace `From a checkout:` with `From a checkout, with [mise](https://mise.jdx.dev):`, and the block under it with

```sh
mise run install                  # jaw and jawd in /usr/local/bin, jaw-engine in /usr/local/libexec/jaw
PREFIX=~/.local mise run install  # or under your home folder, without sudo
mise run uninstall                # with the same PREFIX, takes them out again
```

Replace the paragraph ``  `make install` builds them first. Into a folder only root can write, run `make release` as yourself, then `sudo make install`, which builds nothing.`` with ``  `mise run install` builds them first, as you, and asks for sudo only to copy them into a folder you can't write. Without mise, the same tasks run as scripts, such as `mise/tasks/install`.`` In the next paragraph, replace `` `make mac` builds it, with Xcode and [mise](https://mise.jdx.dev) for Tuist,`` with `` `mise run mac` builds it, with Xcode and the Tuist mise installs,``.
- [ ] **Step 2: README.md's tasks.** Before `### How jawd runs`, add:

```markdown
### Working on jaw

jaw's tasks run with mise from anywhere in a checkout, and `mise tasks` lists them. Each is a script in `mise/tasks/`, which runs without mise too.

| Task | What it does |
| --- | --- |
| `mise run build` (`b`) | jaw-engine, then the Rust workspace, into `target/` |
| `mise run test` (`t`) | Go's tests with `-race`, then the workspace's |
| `mise run lint` (`l`) | `go vet`, gofmt, rustfmt, and clippy with warnings as errors |
| `mise run fmt` | gofmt and rustfmt, writing |
| `mise run install`, `uninstall` | release builds into `PREFIX`, `/usr/local` unless it's set, and out again |
| `mise run perf` | the budgets below, as a table in `target/perf/` (`-- --quick` leaves out the slowest) |
| `mise run proto`, `sync-protos`, `check-generated` | the engine's generated code and WhatsApp's protobufs, and the check that both are as committed |
| `mise run check-audit`, `check-parity` | the data layer's audits, and the Mac's parity with the terminal |
| `mise run mac`, `mac-test` | Jaw.app, signed, in `mac/build/release/`, and the Mac's tests |
| `mise run setup` | the pre-push hook, once for a clone |
| `mise run release` | a release's tarball and formula, and the commands that publish them |
| `mise run prune-builds`, `clean` | the object files no binary names, from every worktree's `target/`; or this checkout's whole `target/` |
```

- [ ] **Step 3: README.md's costs.** Replace `` Measured by `make perf` on 5 November 2026`` with `` Measured by `scripts/perf.py` on 5 November 2026``, and the other three `` `make perf` `` (`` `make perf` doesn't count it``, `` `make perf` is written to run there too``, `` `make perf` takes a quarter of an hour``) with `` `mise run perf` ``.
- [ ] **Step 4: docs/first-run.md.** Replace

```sh
make build          # target/engine/jaw-engine, target/debug/jawd and target/debug/jaw
make hooks          # the pre-push audit check, if this checkout doesn't have it yet
```

with

```sh
mise run build      # target/engine/jaw-engine, target/debug/jawd and target/debug/jaw
mise run setup      # the pre-push audit check, if this checkout doesn't have it yet
```

and

```sh
make install PREFIX=~/.local   # ~/.local/bin/jaw and jawd, ~/.local/libexec/jaw/jaw-engine
make uninstall PREFIX=~/.local
```

with

```sh
PREFIX=~/.local mise run install   # ~/.local/bin/jaw and jawd, ~/.local/libexec/jaw/jaw-engine
PREFIX=~/.local mise run uninstall
```

and `` which needs `make release` first, then `sudo make install`.`` with `and asks for sudo to copy them there.`
- [ ] **Step 5: mac/README.md.** Replace its two make lines with

```sh
mise run mac                                # Jaw.app with jawd, jaw-engine and jaw inside, signed, saying what it was built from
mise run mac-test                           # mac/scripts/test-built-from, then the four suites above
```

replace `` (`mise.toml` here pins Tuist, SwiftFormat and SwiftLint)`` with `` (the repository's `mise.toml` pins Tuist, SwiftFormat and SwiftLint)``, `` makes `Jaw.xcworkspace` for Xcode.`` with `` makes `Jaw.xcworkspace` for Xcode; in a new checkout mise first asks to trust jaw's `mise.toml`.``, `` only when `make mac` built it`` with `` only when `mise run mac` built it``, `` `make check-parity` fails`` with `` `mise run check-parity` fails``, `` `make perf-mac` is still to come`` with `` `mise run perf-mac` is still to come``, and `` as `make mac`'s has`` with `` as `mise run mac`'s has``.
- [ ] **Step 6: The rest of the documents.**
  - `protocol/README.md`: `` jaw then says to run `make install` again.`` becomes `` jaw then says to install it again with `mise run install`.``
  - `marketing/jaw-in-a-minute/README.md`: `` measured by `make perf` on 25 October 2026`` becomes `` measured by `scripts/perf.py` on 25 October 2026``; `marketing/jaw-in-a-minute/jaw.py`: `measured by make perf on 25 Oct 2026` becomes `measured by scripts/perf.py on 25 Oct 2026`.
  - `docs/plans/2026-10-16-jaw-design.md`: `` the million-message bench in `make perf` `` becomes `` the million-message bench in `mise run perf` ``; `` `make perf` before each milestone closes`` becomes `` `mise run perf` before each milestone closes``; `` a README with pictures, `make install` and a Homebrew tap`` becomes `a README with pictures, an install from a checkout and a Homebrew tap`; `` `make install` signs it with a stable identity`` becomes `` `mise run install` signs it with a stable identity``; and the layout's `Makefile                build, test, install, demo, perf, proto` becomes `mise.toml  mise/tasks/  build, test, lint, install, perf, proto, mac, release`.
  - `docs/plans/2026-10-26-jaw-mac-design.md`: `` `jawd demo` and `make perf` are at hand`` becomes `` `jawd demo` and `mise run perf` are at hand``; `` `make perf-mac` `` becomes `` `mise run perf-mac` `` (three times); `` only when `make mac` built it`` becomes `` only when `mise run mac` built it``; `` **`make mac`** builds`` becomes `` **`mise run mac`** builds``; `` `make mac-install` `` becomes `` `mise run mac-install` ``.
- [ ] **Step 7: The rules.** In `.cursor/rules/data-integrity.mdc`, `` `make check-audit` must pass locally; the pre-push hook (`make hooks`) refuses`` becomes `` `mise run check-audit` must pass locally; the pre-push hook (`mise run setup`) refuses``. In `.cursor/rules/disk-space.mdc`: the description's `When and how to run make prune-builds` becomes `When and how to run mise run prune-builds`; `` `make prune-builds` deletes those`` becomes `` `mise run prune-builds` deletes those``; and `` - `make prune-builds`, from any checkout that has it (origin/main since ae37609): it covers`` becomes `` - `mise run prune-builds` (`make prune-builds` in a checkout from before the move to mise, or `python3 scripts/prune-builds.py` in any since ae37609): it covers``.
- [ ] **Step 8: The Mac's comments.** `mac/Project.swift`: `` `make mac` signs releases`` becomes `` `mise run mac` signs releases``, and `make mac-test runs them in their own` becomes `mise run mac-test runs them in their own`. `mac/parity.toml`: `` `make check-parity` fails when the`` becomes `` `mise run check-parity` fails when the``.
- [ ] **Step 9: The Makefile goes.** `git rm -q Makefile`.
- [ ] **Step 12 (before Step 10): 0.1.0's notes and checklist, while 0.1.0 is still to be released.** If `git ls-remote --tags origin v0.1.0` lists nothing, 0.1.0 will be cut from a main without make, so:
  - `docs/releases/0.1.0.md`: `make install PREFIX=~/.local  # jaw and jawd in ~/.local/bin, jaw-engine in ~/.local/libexec/jaw` becomes `PREFIX=~/.local mise run install  # jaw and jawd in ~/.local/bin, jaw-engine in ~/.local/libexec/jaw`, and `make uninstall PREFIX=~/.local` becomes `PREFIX=~/.local mise run uninstall`.
  - `docs/releases/0.1.0-publish.md`: `` `scripts/release.sh`, which step 4 runs`` becomes `` `mise run release`, which step 4 runs``; `` passes `make check-generated`, `make lint`, `make test` under the flakes rule and `make check-audit` `` becomes `` passes `mise run check-generated`, `mise run lint`, `mise run test` under the flakes rule and `mise run check-audit` ``; `` which `scripts/release.sh` needs`` becomes `` which `mise run release` needs``; `cd ~/src/jaw-v0.1.0 && scripts/release.sh` becomes `cd ~/src/jaw-v0.1.0 && mise run release`; `make -C /tmp/jaw-v0.1.0-check/jaw-0.1.0 install PREFIX=/tmp/jaw-v0.1.0-check/prefix` becomes `PREFIX=/tmp/jaw-v0.1.0-check/prefix /tmp/jaw-v0.1.0-check/jaw-0.1.0/mise/tasks/install`; and `` `make uninstall PREFIX=~/.local` first`` becomes `` `PREFIX=~/.local mise run uninstall` first``. Its history stays: main's Linux job "failing at `make lint`" on 5 November.
  - Thursday's `docs/releases/0.1.0-spare-number-session.md` is a record, and stays.
  - These go in a commit of their own, after Step 11's: "0.1.0's notes and checklist say mise's commands, since 0.1.0 is to be cut from a main without make (the owner's word to land the move to mise before it): … Docs only". If `v0.1.0` is tagged by then, this step is skipped. (On 7 November `v0.1.0` was tagged at fdeba680 at 05:30, after this step's commit was made and before the landing, so the commit was dropped at the landing.)
- [ ] **Step 10: Nothing current names make.**

```bash
pat='(?<![a-zA-Z_./-])make(\x27s)? (-s |-C \S+ )?(build|engine|rust|release|install|uninstall|test|lint|fmt|proto|sync-protos|check-generated|check-audit|check-parity|hooks|perf|perf-mac|clean|prune-builds|mac|mac-test|mac-install)(?![a-zA-Z-])|\$\(MAKE\)|make -s engine'
git grep -n -P "$pat" -- . ':!docs/audits' ':!docs/reviews' ':!docs/releases' ':!docs/plans' ; git grep -n -P "$pat" -- docs/plans/2026-10-16-jaw-design.md docs/plans/2026-10-26-jaw-mac-design.md; echo "end"
mise tasks ls | wc -l
```

Expected: only `end` (no lines before it), then `20`.
- [ ] **Step 11: Commit.**

```bash
git add -A README.md docs/first-run.md mac/README.md protocol/README.md marketing docs/plans/2026-10-16-jaw-design.md docs/plans/2026-10-26-jaw-mac-design.md .cursor/rules mac/Project.swift mac/parity.toml Makefile
git commit -q -m "The Makefile goes, its targets now mise's tasks (the move to mise's design): the README installs with mise run install, which asks for sudo for the copy alone, and lists the tasks; docs/first-run.md builds with mise run build and sets up with mise run setup; the rules name mise run check-audit, mise run setup and mise run prune-builds, with make prune-builds for a checkout from before; the designs, the protocol's README, the Mac's README, project and parity list name the mise tasks; and the numbers measured on 5 and 25 October name scripts/perf.py, which ran them. The records keep their words. Docs only, but for the Makefile"
```

---

### Task 15: The gate

- [ ] **Step 1:** `cd ~/src/jaw-mise && export CARGO_INCREMENTAL=0 && mise run check-generated && mise run l && mise run t`. Expected: each passes, with failing tests judged as in Task 4's Step 6.
- [ ] **Step 2:** `mise run check-audit`. Expected now: it fails on Task 13's commit, which has no audit yet; Task 16 gives it one.
- [ ] **Step 3:** `mise run mac-test`. Expected: as in Task 8's Step 9.
- [ ] **Step 4:** `mise run prune-builds`.

---

### Task 16: The database expert's audit

- [ ] **Step 1:** Launch a fresh `database-expert` subagent (`.cursor/agents/database-expert.md`; if that type isn't offered, a general-purpose one told to read that file first and follow it as its brief), with the worktree `~/src/jaw-mise`, the range `origin/main..HEAD`, and this design and plan. It writes `docs/audits/2026-11-06-mise.md` (or the day it writes it), ending `Verdict: approved` or not.
- [ ] **Step 2:** Fix whatever it marks as data loss or inconsistency, and have a fresh auditor audit again, until it's approved.
- [ ] **Step 3: Commit the note.** `git add docs/audits/2026-11-06-mise.md`, then commit it with a subject in the form main's audit commits take (`git log -1 d4ca8bca` is one): "The database expert's audit of the move to mise, N rounds, approved (docs/audits/2026-11-06-mise.md): " followed by what each round found and what's left as follow-ups, ending "Docs only".
- [ ] **Step 4: The trailer, on each commit `check-data-audit.py` says needs one.** It goes by the script's own list of the data layer's paths, so only Task 13's commit should take it.

```bash
base="$(git merge-base origin/main HEAD)"
git rebase "$base" -x 'python3 scripts/check-data-audit.py HEAD~1 HEAD >/dev/null 2>&1 || git commit -q --amend --no-edit --trailer "Data-Audit: docs/audits/2026-11-06-mise.md"'
mise run check-audit && echo audited
git log --format='%h %s%n  %(trailers:key=Data-Audit,valueonly)' "$base"..HEAD | grep -B1 docs/audits
```

Expected: `audited`, and one commit, Task 13's, with the trailer.

---

### Task 17: Landing

- [ ] **Step 1: The sudo path, with a stand-in for sudo,** since the owner is away and an agent's shell can't answer sudo. A folder made unwritable has `install` take the sudo branch; a `sudo` on the PATH that logs its command, makes the folder writable and runs it shows the copy alone went through it:

```bash
mkdir -p /tmp/jaw-mise/fakesudo /tmp/jaw-mise/ro
printf '%s\n' '#!/bin/sh' 'echo "sudo $*" >> /tmp/jaw-mise/sudo.log' 'chmod -R u+w /tmp/jaw-mise/ro' 'exec "$@"' > /tmp/jaw-mise/fakesudo/sudo
chmod +x /tmp/jaw-mise/fakesudo/sudo && chmod 555 /tmp/jaw-mise/ro
env PATH="/tmp/jaw-mise/fakesudo:$PATH" PREFIX=/tmp/jaw-mise/ro mise run install
cat /tmp/jaw-mise/sudo.log; ls /tmp/jaw-mise/ro/bin
chmod 555 /tmp/jaw-mise/ro/bin /tmp/jaw-mise/ro/libexec/jaw /tmp/jaw-mise/ro/libexec /tmp/jaw-mise/ro
env PATH="/tmp/jaw-mise/fakesudo:$PATH" PREFIX=/tmp/jaw-mise/ro mise run uninstall
cat /tmp/jaw-mise/sudo.log; find /tmp/jaw-mise/ro -type f | wc -l
chmod -R u+w /tmp/jaw-mise/ro && rm -rf /tmp/jaw-mise/ro /tmp/jaw-mise/fakesudo /tmp/jaw-mise/sudo.log
```

  Expected, since install makes both folders before either copy (the fix round's change) and this stand-in opens the whole tree on its first call: one `sudo mkdir -p /tmp/jaw-mise/ro/bin` line for install and none for cargo or go, `jaw` and `jawd` in `bin`; then, the tree locked again, one `sudo rm -f …` line and `0`. A stricter stand-in, which locks the tree again after each command, shows sudo for both folders and then both copies, in that order; the fix round ran it (branch-fix-report). The real check stays for the owner (Step 5).
- [ ] **Step 2: Catch up with main.** `git fetch origin && git rebase origin/main`. If `mac-perf` has landed, move its make lines into tasks: `python3 mac/scripts/test-perf` in `mise/tasks/mac-test` after jawd's debug build, and `mise/tasks/perf-mac`:

```bash
#!/usr/bin/env bash
#MISE description="The Mac app's budgets in both looks, measured on the build Mac against a made-up million-message account, as one table in target/perf-mac/ (mac/scripts/perf says how; -- --small checks the driver): the bundle mise run mac makes, for its size, then the app in Release there; never here"
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here/../.."
ssh -o BatchMode=yes -o ConnectTimeout=5 "${JAW_BUILD_MAC:-build-mac}" true 2>/dev/null \
    || { echo "mise run perf-mac measures on the build Mac, which doesn't answer" >&2; exit 1; }
"$here/mac"
mac/scripts/build-mac perf "$@"
```

  If `mac-menus` has landed, check that its `JawHarnessTests` line is already in `mise/tasks/mac-test`. If either lands after this one, it moves its own lines as it rebases. If the rebase changed anything in the data layer, the database expert audits again (Task 16).
- [ ] **Step 3: The gate again.** Task 15's steps, with `mise run check-audit` passing.
- [ ] **Step 4: CI.** `git push -u origin mise`, then `gh run list --repo pdcgomes/jaw --branch mise --limit 1` and `gh run view <its ID> --repo pdcgomes/jaw`. Expected: `data-audit` and `parity` pass, and the test job fails nowhere that main's latest run doesn't.
- [ ] **Step 5: The owner's sudo check, for the morning report,** since an agent's shell can't answer sudo. He runs, in his own terminal:

```bash
cd ~/src/jaw-mise
sudo mkdir -p /tmp/jaw-mise/rootonly && sudo chown root:wheel /tmp/jaw-mise/rootonly
PREFIX=/tmp/jaw-mise/rootonly mise run install     # builds as you, then asks for your password for the copy alone
ls -l /tmp/jaw-mise/rootonly/bin                   # jaw and jawd, owned by root
PREFIX=/tmp/jaw-mise/rootonly mise run uninstall   # asks again if sudo has forgotten
sudo rm -rf /tmp/jaw-mise/rootonly
```

- [ ] **Step 6: Land,** as the owner said to: from `~/src/jaw-mise`, never a scratch worktree, `git fetch origin && git rebase origin/main`, the gate again if main moved, then `git push origin HEAD:main`, which the pre-push hook checks. Then `git push origin --delete mise`, and `gh run list --repo pdcgomes/jaw --branch main --limit 1` shows main's CI started on the landed commit. The morning report tells the owner the coordinator needs telling, since its briefs and gate scripts call make's targets.

---

### Task 18: Clean up and report

- [ ] **Step 1:** `mise run prune-builds`, then `rm -rf /tmp/jaw-mise`, and the build Mac's copy of this checkout, `ssh build-mac 'rm -rf ~/src/build-mac/jaw/jaw-mise'`.
- [ ] **Step 2:** Bring the canvas up to date: every step's state, the log, what's left for the owner.
- [ ] **Step 3:** Once it has landed, `git -C ~/src/jaw worktree remove ~/src/jaw-mise && git -C ~/src/jaw branch -d mise`, which removes its `target/` with it, as the disk rule asks of a landed branch.
- [ ] **Step 4:** Report: what landed, the audit's verdict, CI's result, the gate's, any test judged a flake, the owner's sudo check (Task 17, Step 5), the coordinator to be told, and, if 0.1.0 wasn't tagged first, that its notes and checklist now say mise's commands, or that tagging the commit before the landing releases it with make as they first said. Then the Mac app's release gets its own design, from the design's "Later".
