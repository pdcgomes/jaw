# jaw: the engine in Go (engine/), everything else in Rust (crates/).

GO ?= go
CARGO ?= cargo
BUF := github.com/bufbuild/buf/cmd/buf@v1.74.0

# The version jaw, jawd and jaw-engine all say: git describe's, or VERSION's in a release's
# tarball (scripts/version says how; crates/jaw-version gives the Rust programs the same).
JAW_VERSION ?= $(shell sh scripts/version)

# Where make install puts jaw: jaw and jawd in bin/, and the engine, which only jawd runs, in
# libexec/jaw/, where jawd looks for it beside its own folder. DESTDIR stages it elsewhere.
PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
LIBEXECDIR ?= $(PREFIX)/libexec/jaw

.PHONY: build engine rust release install uninstall test lint fmt proto sync-protos check-generated check-audit check-parity hooks perf clean prune-builds mac mac-test

build: engine rust

engine:
	cd engine && $(GO) build -trimpath -ldflags "-X main.version=$(JAW_VERSION)" -o ../target/engine/jaw-engine ./cmd/jaw-engine

rust:
	$(CARGO) build --workspace

# What make install installs: target/release/jaw and jawd, and the engine.
release: engine
	$(CARGO) build --release --locked -p jaw -p jawd

# As root, as with sudo, it installs what make release built and builds nothing: cargo and go
# would start again in root's caches, without the crates and modules you've downloaded.
install: $(if $(filter 0,$(shell id -u)),,release)
	@test -x target/release/jaw && test -x target/release/jawd && test -x target/engine/jaw-engine || \
		{ echo "nothing to install yet: run make release first, as yourself"; exit 1; }
	install -d "$(DESTDIR)$(BINDIR)" "$(DESTDIR)$(LIBEXECDIR)"
	install -m 755 target/release/jaw target/release/jawd "$(DESTDIR)$(BINDIR)"
	install -m 755 target/engine/jaw-engine "$(DESTDIR)$(LIBEXECDIR)"

uninstall:
	rm -f "$(DESTDIR)$(BINDIR)/jaw" "$(DESTDIR)$(BINDIR)/jawd" "$(DESTDIR)$(LIBEXECDIR)/jaw-engine"
	rmdir "$(DESTDIR)$(LIBEXECDIR)" 2>/dev/null || true

test: engine
	cd engine && $(GO) test -race ./...
	$(CARGO) test --workspace

lint:
	cd engine && $(GO) vet ./...
	@test -z "$$(gofmt -l engine)" || (gofmt -l engine; echo "gofmt: the files above need formatting"; exit 1)
	$(CARGO) fmt --all --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings

fmt:
	gofmt -w engine
	$(CARGO) fmt --all

# Every budget in the design, measured on release builds against a made-up million-message
# account, as one table in target/perf/ (scripts/perf.py says how; --quick leaves out the slowest).
perf: engine
	$(CARGO) build --release -p jawd -p jaw
	python3 scripts/perf.py

# protoc-gen-go at the version engine/go.mod pins, and buf, which compiles the .proto itself.
target/tools/protoc-gen-go: engine/go.mod
	cd engine && $(GO) build -o ../target/tools/protoc-gen-go google.golang.org/protobuf/cmd/protoc-gen-go

proto: target/tools/protoc-gen-go
	$(GO) run $(BUF) generate proto/engine --template proto/engine/buf.gen.yaml

sync-protos:
	python3 scripts/sync-protos.py

check-generated: proto
	python3 scripts/sync-protos.py --check
	git diff --exit-code -- engine/internal/link/enginepb

# Every commit since origin/main that touches the data layer names an approved audit.
check-audit:
	python3 scripts/check-data-audit.py origin/main HEAD

# Every request and push of jawd's that the terminal uses and the Mac doesn't has a step of the Mac's
# parity plan in mac/parity.toml. ARGS="--terminal <branch>" reads a terminal branch before it lands,
# and ARGS=--table lists every request and push with each side's use.
check-parity:
	python3 scripts/check-parity.py $(ARGS)

# The same check before every push to main, from every worktree.
hooks:
	install -m 755 scripts/pre-push "$$(git rev-parse --git-common-dir)/hooks/pre-push"

# jaw for the Mac (mac/): the app with jawd, jaw-engine and jaw inside it, each signed. The app builds
# and tests on the build Mac when it answers (mac/scripts/build-mac), and here when it doesn't.
mac:
	mac/scripts/bundle

mac-test:
	$(CARGO) build -p jawd --bin jawd
	mac/scripts/build-mac test JawKit
	mac/scripts/build-mac test JawThemes
	mac/scripts/build-mac test JawUITests
	mac/scripts/build-mac test JawHarnessTests

clean:
	$(CARGO) clean

# Deletes from every worktree's target/ the object files no binary names any more, which debug builds
# leave by the tens of GB a day where tests rebuild all day; nothing rebuilds after
# (scripts/prune-builds.py says why).
prune-builds:
	python3 scripts/prune-builds.py
