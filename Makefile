# jaw: the engine in Go (engine/), everything else in Rust (crates/).

GO ?= go
CARGO ?= cargo
BUF := github.com/bufbuild/buf/cmd/buf@v1.74.0

.PHONY: build engine rust test lint fmt proto sync-protos check-generated clean

build: engine rust

engine:
	cd engine && $(GO) build -o ../target/engine/jaw-engine ./cmd/jaw-engine

rust:
	$(CARGO) build --workspace

test:
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

clean:
	$(CARGO) clean
