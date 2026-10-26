# Jaw for the Mac

The Mac's interface on jawd's socket, as [the Mac design](../docs/plans/2026-10-26-jaw-mac-design.md) has it and in [the Mac plan](../docs/plans/2026-10-26-jaw-mac-plan.md)'s order. This is Mac-M1, text every day, under way.

## Building

The tools sit behind mise (`mise.toml` here pins Tuist, SwiftFormat and SwiftLint). Builds and tests run on the build Mac, `build-mac` in `~/.ssh/config`, so this Mac stays free; when it doesn't answer they run here.

```sh
cargo build -p jawd --bin jawd              # the jawd the live tests and --demo run, built here
mac/scripts/build-mac test JawKit           # the client: fixtures, window models, a fake jawd, a real one
mac/scripts/build-mac test JawThemes
mac/scripts/build-mac test JawUITests       # the layout, and its reference pictures in both looks
mac/scripts/build-mac build jaw
make mac                                    # Jaw.app with jawd, jaw-engine and jaw inside, signed
make mac-test                               # the three suites above
```

The build Mac has no Rust or Go, so jawd, jaw and jaw-engine are built here and their binaries go with the checkout. `mise exec -- tuist generate --no-open` makes `Jaw.xcworkspace` for Xcode.

A debug build never opens the owner's jaw folder: without `--home` or `JAW_HOME` it runs `jawd --engine demo`, a made-up account, in a folder of its own, and stops it on quitting. Only a signed release registers its jawd at login (Jaw › jawd at Login…), under the label `jaw agent install` uses, offering to take over the terminal's agent on a click and keeping a jawd newer than its own.

The app's options: `--demo`, `--home <folder>`, `--bench launch|open|memory|idle`, which prints one JSON line and quits, and `--snapshot <png>`, which draws the window once a chat is open. The harness (`JawHarness`) draws each component from made-up rows, either look, dark or light, with `--scene <name>`, `--light` and `--snapshot <png>`.

## What it costs

Measured on 26 October 2026 on the build Mac (Apple M4 Max, 16 cores, macOS 27.0.1) with a Release build, against `jawd demo`'s made-up account of a million messages in 3,000 chats served by a release jawd, while it indexed that history for search and other builds kept the load average at 19 to 30. These are Mac-M1's first measurements; `make perf-mac` is still to come.

| | Budget | Measured |
| --- | --- | --- |
| Launch to a usable chat list | 250 ms p99 with jawd running | 188 to 196 ms over five warm launches; 596 ms the first, cold |
| Opening a chat | 30 ms p99 | 33.4 ms p50, **54.8 ms** p99 over the first 20 chats |
| Memory | under 80 MB with a screen of text | 35 MB with the chat list and a chat drawn |
| Idle | no wake-ups but the clock's minute | **14** wake-ups in a quiet minute |
| App size | under 60 MB with the helpers | 44 MB |

Where it falls short: opening a chat at p99, measured at a load of 25 to 30 with jawd's search index being built beside it, is to be measured again on a quiet Mac before it's called a defect; and the quiet minute's 14 wake-ups aren't yet accounted for. The keypress, scrolling and a frame's apply aren't measured yet.
