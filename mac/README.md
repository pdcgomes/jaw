# Jaw for the Mac

The Mac's interface on jawd's socket, as [the Mac design](../docs/plans/2026-10-26-jaw-mac-design.md) has it and in [the Mac plan](../docs/plans/2026-10-26-jaw-mac-plan.md)'s order. This is Mac-M1, text every day, under way.

## Building

The tools sit behind mise (`mise.toml` here pins Tuist, SwiftFormat and SwiftLint). Builds and tests run on the build Mac, `build-mac` in `~/.ssh/config`, so this Mac stays free; when it doesn't answer they run here.

```sh
cargo build -p jawd --bin jawd              # the jawd the live tests and --demo run, built here
mac/scripts/build-mac test JawKit           # the client: fixtures, window models, a fake jawd, a real one
mac/scripts/build-mac test JawThemes
mac/scripts/build-mac test JawUITests       # the layout, and its reference pictures in both looks
mac/scripts/build-mac test JawHarnessTests  # the harness: its catalog, flags, pictures and scenes
mac/scripts/build-mac build jaw
make mac                                    # Jaw.app with jawd, jaw-engine and jaw inside, signed
make mac-test                               # the four suites above
```

The build Mac has no Rust or Go, so jawd, jaw and jaw-engine are built here and their binaries go with the checkout. `mise exec -- tuist generate --no-open` makes `Jaw.xcworkspace` for Xcode.

A debug build never opens the owner's jaw folder: without `--home` or `JAW_HOME` it runs `jawd --engine demo`, a made-up account, in a folder of its own, and stops it on quitting. Only a signed release registers its jawd at login (Jaw › jawd at Login…), under the label `jaw agent install` uses, offering to take over the terminal's agent on a click and keeping a jawd newer than its own.

The app's options: `--demo`, `--home <folder>`, `--bench launch|open|memory|idle`, which prints one JSON line and quits, and `--snapshot <png>`, which draws the window once a chat is open (or the sheet over it). With `--snapshot`, `--keys` presses shortcuts through the app's own event handling half a second apart and prints the panes after each: `cmd+shift+d`, `ctrl+cmd+right`, `cmd+opt+e`, a digit or a letter, and the steps `size:1200x800`, `save:<layout>`, `revert` and `drop:<row>:<middle|left|right|top|bottom>`. For example, `mac/scripts/build-mac snapshot /tmp/x.png --keys "cmd+d,save:Work,cmd+w,ctrl+cmd+1"`.

## Component harness

Jaw Harness (`JawHarness`) draws the app's components and screens from made-up rows and the protocol's fixtures, in each theme, look and density, and asks jawd for nothing, as [the harness's design](../docs/plans/2026-10-27-jaw-mac-harness-design.md) has it. Its scenes are listed by section on the left, with the conditions they're drawn in under them; each says what to look at, and each band of specimens what would be wrong with it.

```sh
mac/scripts/build-mac harness [scene] [options]    # built on the build Mac and opened here
mac/scripts/build-mac shot <scene> <png> [options] # one scene drawn into a picture
mac/scripts/build-mac sweep                        # every scene in each variant, into mac/build/review
```

After a sweep, the review canvas, `.atelier/mac-harness.canvas.tsx`, shows each scene's pictures by section (`atelier open .atelier/mac-harness.canvas.tsx`). Mark each one Right, or Needs work with a note; the marks are kept under `marks`, by scene, with the hashes of the pictures they were made on, and a scene whose pictures change after its mark asks again. Agents read the marks with `mac/scripts/review-marks`, a line a scene (`--json` for the same as JSON), by the canvas's rule: a mark counts only while its hashes are that scene's pictures' hashes in the sweep's `manifest.json`, in order, so a scene whose pictures have changed since is changed since marked, and one with no mark, or a cleared one, hasn't been looked at. The canvas's pictures come from `build-mac sweep`, which writes them and the canvas's data, with their hashes, together: a `--sweep` run by hand into `mac/build/review` would leave new pictures under the last sweep's hashes until `mac/scripts/review-data` writes the data again.

Its options: `--scene <id>`, `--theme <id>`, `--appearance dark|light`, `--look bubbles|lines`, `--density comfortable|compact`, `--text-size <11…17>`, `--font system|mono`, `--reduce-motion`, `--window <width>x<height>`, `--stage-only`, `--snapshot <png>` and `--sweep <folder>`.

To add a component, write a scene in `JawHarness/Sources/Scenes/` and register it in `BuiltInScenes.swift`. `scripts/check-harness`, which every build runs, fails if the harness names `Jawd`, or JawKit's `Client`, which opens jawd's socket itself.

## How the interface is put together

- **Components** (`JawUI/Sources/Components`): the type scale (`TextStyle`), spacing, radii, lines and motion (`Space`, `Radius`, `Stroke`, `Motion`, which stops under Reduce Motion), colours by `Tone` from the theme's roles, and the controls every surface uses (`IconButton`, `Labels`, `Surface`, and `.textStyle`, `.tone`, `.card`, `.panel` and `KeyCap` in SwiftUI). `scripts/check-components`, run by every build, fails on a font, a radius or a line width set by hand anywhere else; the messages' own fonts are the layout's.
- **Commands** (`JawUI/Sources/Commands`, the app's in `JawApp/Sources/Commands.swift`): each thing the app does is written once, with its menu title, its phrase for the palette, its key and its menu. The menu bar, the toolbar's menus, the panes' context menus, the palette and the keys sheet are made from that list, so every key shows in a menu and every menu item says its key. `Shortcut` gives menus a Shift letter's capital. Debug builds stop at launch on two commands with one key.
- **Panes and layouts** ([the grid layouts design](../docs/plans/2026-10-26-jaw-mac-grid-layouts-design.md)): `PaneGrid` is the layout as slots on a grid's lines, with its rules tested on their own; `GridView` sets the panes' frames by hand, so they follow a live resize in the same frame; `LayoutBook` keeps the named layouts and an edited shape apart from the saved one; `LayoutEditorModel` and its sheet are the editor. One box, the window's (`BoxPlace`), shows the focused chat's composer.
- **Parity with the terminal** ([the parity plan](../docs/plans/2026-10-27-jaw-mac-parity-plan.md)): `parity.toml` lists each request and push of jawd's that the terminal uses and the app doesn't yet, with the plan's step that brings it. `make check-parity` fails when the terminal gains one the list doesn't have, or the app closes one the list still holds, so a step that lands takes its rows out.

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
