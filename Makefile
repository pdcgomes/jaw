# jaw: the engine in Go (engine/), everything else in Rust (crates/).

GO ?= go
CARGO ?= cargo
BUF := github.com/bufbuild/buf/cmd/buf@v1.74.0

.PHONY: build engine rust test lint fmt proto sync-protos check-generated check-audit hooks perf clean prune-builds mac mac-test

build: engine rust

engine:
	cd engine && $(GO) build -o ../target/engine/jaw-engine ./cmd/jaw-engine

rust:
	$(CARGO) build --workspace

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

clean:
	$(CARGO) clean

# Deletes from every worktree's target/ the object files no binary names any more, which debug builds
# leave by the tens of GB a day where tests rebuild all day; nothing rebuilds after
# (scripts/prune-builds.py says why).
prune-builds:
	python3 scripts/prune-builds.py
