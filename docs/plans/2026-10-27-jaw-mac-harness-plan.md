# Jaw Harness, phase one: the shell and the loop

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Jaw Harness rewritten in the way of Redlamp's and omoi-station's, with today's six scenes, launch flags, pictures drawn by the harness's own views, a sweep of every scene in each variant, and a review canvas where the owner marks each scene, as the first phase of `docs/plans/2026-10-27-jaw-mac-harness-design.md` has it.

**Architecture:** `JawHarness` becomes a SwiftUI app hosting AppKit. Scenes are values in a catalog built once (`HarnessScene`, `HarnessCatalog`, registered in `BuiltInScenes.swift`), each drawn by a closure from `Conditions`, the controls under the scene list as values. One view, `SceneCanvas`, draws a scene on the stage and into pictures, which `Pictures` makes off the screen with `cacheDisplay` and writes, with a manifest, for `--snapshot` and `--sweep`. `mac/scripts/build-mac` runs it on the build Mac; `mac/scripts/review-data` turns the sweep's manifest into the data of an Atelier canvas, `.atelier/mac-harness.canvas.tsx`.

**Tech Stack:** Swift 6 (SwiftUI and AppKit, macOS 26, Swift Testing), Tuist 4.197 through mise, xcodebuild on the build Mac through `mac/scripts/build-mac`, POSIX `sh`, Python 3, an Atelier canvas in TSX.

## Global Constraints

- The spec is `docs/plans/2026-10-27-jaw-mac-harness-design.md`, and this plan is its first phase, "The shell and the loop". Where this plan and the spec differ, the spec wins: stop and say so.
- Work only in the worktree `/Users/pedrogomes/src/jaw-harness`, on the branch `mac-harness`. Never push, never pass `--no-verify`, never rewrite commits that aren't yours. Never edit, build in, or run a git command that changes anything in `/Users/pedrogomes/src/jaw-mac-m1`: the Mac app's chat works there.
- Build and test on the build Mac with `mac/scripts/build-mac` (`build JawHarness`, `test JawHarnessTests`, and from Task 2 `shot`, from Task 3 `sweep` and `harness`). It copies this checkout to `~/src/build-mac/jaw/jaw-harness` there and runs the job; when the build Mac doesn't answer, it runs here.
- Swift 6 language mode, macOS 26, warnings are errors. `mac/scripts/check-components` runs before every build and fails on `systemFont(ofSize`, `.font(.system(`, `cornerRadius: <digit>` or `borderWidth = <digit>` in `JawHarness/Sources` too: take fonts from `TextStyle` (`.textStyle(.caption)`), radii from `Radius`, lines from `Stroke`, spacing from `Space` (`Space.xxl` is 24).
- The harness asks jawd for nothing: it never makes a `Jawd`, never opens `~/Library/Application Support/jaw`, never starts jawd or jaw-engine. Never stop a process by name; stop only a PID you started, or one whose command line names this worktree.
- Scratch stays in the worktree's ignored `mac/build/` (pictures you look at in `mac/build/shots/`) and the build Mac's copy of the checkout. A test that writes files uses its own folder, `FileManager.default.temporaryDirectory.appending(path: "jaw-harness-tests-<UUID>")`, and deletes it.
- Commit messages are one long sentence in the project's voice (read `git log -5`), with no prefixes such as `feat:`. Code matches its surroundings: doc comments in plain sentences, British spelling (colour), no comments that narrate a change, 4-space indentation, trailing commas in multi-line argument lists as the existing files have them.
- Scene ids are lower-case words joined by hyphens: `roles`, `mimic`, `chat-list`, `conversation`, `palette`, `states`.
- A variant's name is `dark` or `light`, then `-bubbles` or `-lines` for a scene drawn in both looks, then `-compact` for its compact variant: `dark-bubbles-compact`. Variants run dark before light, bubbles before lines, comfortable before compact.
- A scene is drawn `width` points wide (960 unless it says) and as tall as it needs, with 24 points (`Space.xxl`) of its surface around it, at 2×.
- A sweep writes `<folder>/<scene-id>/<variant>.png` and `<folder>/manifest.json`; `build-mac sweep` sweeps into `mac/build/review/`. The canvas is `.atelier/mac-harness.canvas.tsx`, its data `.atelier/mac-harness.data.ts`, and the owner's marks are kept in its state under `marks`, by scene id, each `{ state: "right" | "needs-work", note, hashes, at }`.
- Phase one builds only what the spec's first phase lists. The spec's later building blocks and fields (`AppKitSpecimen`, `ScreenSpecimen`, `Knob`, `fillsStage`, `pinned`, `--check`, `--record`, screen sizes, `check-harness`'s second and third rules) come with the phases that use them.

## File structure

New:

- `mac/JawHarness/Sources/HarnessScene.swift`: `HarnessSection`, `Conditions`, `Axes`, `Variant`, `HarnessScene`, `HarnessCatalog`.
- `mac/JawHarness/Sources/HarnessLaunch.swift`: the launch flags.
- `mac/JawHarness/Sources/HarnessApp.swift`: the app, and the delegate that runs `--snapshot` and `--sweep`.
- `mac/JawHarness/Sources/BuiltInScenes.swift`: the table of contents.
- `mac/JawHarness/Sources/HarnessRootView.swift`: the sidebar and the stage's place; `HarnessWindow`, for `--window`.
- `mac/JawHarness/Sources/ConditionsControls.swift`: the controls under the scene list.
- `mac/JawHarness/Sources/Stage.swift`: `StageSurface`, `StageSettings`, `SceneCanvas`, `Stage`.
- `mac/JawHarness/Sources/Specimen.swift`: `Note`, `NotesKey`, `SpecimenGroup`, `Specimen`, `ControllerSpecimen`.
- `mac/JawHarness/Sources/Pictures.swift`: drawing scenes into pictures, `--snapshot` and `--sweep`.
- `mac/JawHarness/Sources/Fixtures/Fixtures.swift`: the protocol's fixtures from the bundle, and the group's members.
- `mac/JawHarness/Sources/Scenes/FoundationScenes.swift`, `ChatListScenes.swift`, `ConversationScenes.swift`, `PaletteScenes.swift`, `StateScenes.swift`: the six scenes.
- `mac/Tests/JawHarnessTests/CatalogTests.swift`, `LaunchTests.swift`, `PicturesTests.swift`, `ScenesTests.swift`.
- `mac/scripts/check-harness`, `mac/scripts/review-data`.
- `.atelier/mac-harness.canvas.tsx`.

Deleted: `mac/JawHarness/Sources/main.swift`.

Changed: `mac/Project.swift`, `mac/scripts/build-mac`, `mac/scripts/check`, `Makefile`, `.gitignore`, `mac/README.md`.

---

### Task 1: The catalog, the conditions and the flags

**Files:**
- Create: `mac/JawHarness/Sources/HarnessScene.swift`, `mac/JawHarness/Sources/HarnessLaunch.swift`
- Create: `mac/Tests/JawHarnessTests/CatalogTests.swift`, `mac/Tests/JawHarnessTests/LaunchTests.swift`
- Modify: `mac/Project.swift` (a test target), `Makefile` (`mac-test`)

**Interfaces:**
- Produces, in the `JawHarness` module:
  - `enum HarnessSection: String, CaseIterable, Identifiable` with cases, in order, `foundations` ("Foundations"), `controls` ("Controls"), `chatList` ("Chat list"), `conversation` ("Conversation"), `panes` ("Panes"), `overlays` ("Palette and overlays"), `states` ("States"), `screens` ("Screens"), `live` ("Live"), `performance` ("Performance"); `var id: String`; `var symbol: String`.
  - `struct Conditions: Hashable, Sendable { var theme: String; var dark: Bool; var look: Look; var density: Density; var textSize: CGFloat; var font: MessageFont; var reduceMotion: Bool; var roles: Roles { get }; var style: Style { get } }`, defaulting to jaw's theme, dark, bubbles, comfortable, 13, the system font, no Reduce Motion.
  - `struct Axes: OptionSet, Hashable, Sendable` with `.look` and `.density`.
  - `struct Variant: Hashable, Sendable { var dark: Bool; var look: Look?; var density: Density?; var name: String { get }; func applied(to: Conditions) -> Conditions; static func all(_ axes: Axes) -> [Variant] }`.
  - `struct HarnessScene: Identifiable { let id, title, symbol, synopsis: String; let section: HarnessSection; let covers: [Any.Type]; let axes: Axes; let width: CGFloat; let content: @MainActor (Conditions) -> AnyView; var inspector: (@MainActor () -> AnyView)?; var variants: [Variant] { get } }` with `init(id:title:symbol:synopsis:section:covers:axes: = [], width: = 960, @ViewBuilder content: @escaping @MainActor (Conditions) -> some View)`.
  - `struct HarnessCatalog { private(set) var scenes: [HarnessScene]; mutating func register(_:); var sections: [HarnessSection] { get }; func scenes(in:) -> [HarnessScene]; func scene(id: String?) -> HarnessScene? }`.
  - `struct HarnessLaunch: Equatable { struct Size: Hashable, Sendable { var width, height: CGFloat }; var scene: String?; var conditions: Conditions; var window: Size?; var stageOnly: Bool; var snapshot: String?; var sweep: String?; @MainActor static let current: HarnessLaunch; @MainActor static func parse(_ arguments: [String]) -> HarnessLaunch }`.
  - The test target `JawHarnessTests` and its scheme, hosted by the harness app.

The old `main.swift` stays until Task 2; nothing in it is called by the new files.

- [ ] **Step 1: Add the test target**

In `mac/Project.swift`, after the line `tests("JawUI", [.target(name: "JawUI"), .target(name: "JawKit"), .target(name: "JawThemes")]),` add:

```swift
        // Hosted by the harness, so its scenes, their fixtures and its pictures are tested as it runs them.
        tests("JawHarness", [.target(name: "JawHarness"), .target(name: "JawKit"), .target(name: "JawThemes"),
                             .target(name: "JawUI")]),
```

In `Makefile`, in `mac-test`, after `mac/scripts/build-mac test JawUITests` add the line (a tab, as the others):

```make
	mac/scripts/build-mac test JawHarnessTests
```

- [ ] **Step 2: Write the failing tests**

`mac/Tests/JawHarnessTests/CatalogTests.swift`:

```swift
@testable import JawHarness
import JawThemes
import JawUI
import SwiftUI
import Testing

/// The catalog the sidebar lists, the variants a sweep draws a scene in, and the conditions it's
/// drawn in.

func stub(_ id: String, _ section: HarnessSection, axes: Axes = []) -> HarnessScene {
    HarnessScene(id: id, title: id, symbol: "circle", synopsis: "", section: section, covers: [], axes: axes) { _ in
        EmptyView()
    }
}

@Test func `the sidebar lists the sections that have scenes in their order, and each one's scenes as registered`() {
    var catalog = HarnessCatalog()
    catalog.register(stub("b", .states))
    catalog.register(stub("a", .foundations))
    catalog.register(stub("c", .states))
    #expect(catalog.sections == [.foundations, .states])
    #expect(catalog.scenes(in: .states).map(\.id) == ["b", "c"])
    #expect(catalog.scene(id: "c")?.section == .states)
    #expect(catalog.scene(id: "z") == nil && catalog.scene(id: nil) == nil)
}

@Test func `a sweep draws every scene dark and light, and in each look and density it asks for`() {
    #expect(Variant.all([]).map(\.name) == ["dark", "light"])
    #expect(Variant.all(.look).map(\.name) == ["dark-bubbles", "dark-lines", "light-bubbles", "light-lines"])
    #expect(Variant.all([.look, .density]).map(\.name) == [
        "dark-bubbles", "dark-bubbles-compact", "dark-lines", "dark-lines-compact",
        "light-bubbles", "light-bubbles-compact", "light-lines", "light-lines-compact",
    ])
    #expect(stub("s", .conversation, axes: .density).variants.map(\.name) == ["dark", "dark-compact", "light", "light-compact"])
}

@Test func `a variant changes only what it names`() {
    var mine = Conditions()
    mine.textSize = 15
    mine.look = .lines
    let light = Variant(dark: false, look: nil, density: nil).applied(to: mine)
    #expect(!light.dark && light.look == .lines && light.textSize == 15)
    let compact = Variant(dark: true, look: .bubbles, density: .compact).applied(to: mine)
    #expect(compact.dark && compact.look == .bubbles && compact.style.compact)
}

@Test func `a scene is drawn in the app's own style, in the theme's roles`() {
    var conditions = Conditions()
    #expect(conditions.roles == Catalog.jaw.dark)
    conditions.dark = false
    #expect(conditions.style == Style(roles: Catalog.jaw.light))
}
```

`mac/Tests/JawHarnessTests/LaunchTests.swift`:

```swift
@testable import JawHarness
import JawUI
import Testing

/// The launch flags, as scripts and `build-mac` pass them.

@MainActor @Test func `the flags pick the scene, the conditions, the window and what to draw`() {
    let launch = HarnessLaunch.parse([
        "JawHarness", "--scene", "roles", "--appearance", "light", "--look", "lines", "--density", "compact",
        "--text-size", "40", "--font", "mono", "--reduce-motion", "--window", "1200x800", "--stage-only",
        "--snapshot", "/x/roles.png", "--sweep", "/x/review",
    ])
    #expect(launch.scene == "roles")
    #expect(!launch.conditions.dark && launch.conditions.look == .lines && launch.conditions.density == .compact)
    #expect(launch.conditions.textSize == 17, "held to the sizes Settings offers")
    #expect(launch.conditions.font == .mono && launch.conditions.reduceMotion)
    #expect(launch.window == HarnessLaunch.Size(width: 1200, height: 800) && launch.stageOnly)
    #expect(launch.snapshot == "/x/roles.png" && launch.sweep == "/x/review")
}

@MainActor @Test func `a value it doesn't know leaves what it sets as it was`() {
    let launch = HarnessLaunch.parse([
        "JawHarness", "--theme", "nope", "--appearance", "sepia", "--look", "tiles", "--window", "big",
        "--text-size", "huge", "--scene",
    ])
    #expect(launch == HarnessLaunch.parse(["JawHarness"]))
}
```

- [ ] **Step 3: Run them to see them fail**

Run: `mac/scripts/build-mac test JawHarnessTests`
Expected: the build fails, with errors such as `cannot find 'HarnessCatalog' in scope` and `cannot find type 'Conditions' in scope`.

- [ ] **Step 4: Write the catalog and the conditions**

`mac/JawHarness/Sources/HarnessScene.swift`:

```swift
import JawThemes
import JawUI
import SwiftUI

/// Where a scene sits in the sidebar, in sidebar order.
enum HarnessSection: String, CaseIterable, Identifiable {
    case foundations = "Foundations"
    case controls = "Controls"
    case chatList = "Chat list"
    case conversation = "Conversation"
    case panes = "Panes"
    case overlays = "Palette and overlays"
    case states = "States"
    case screens = "Screens"
    case live = "Live"
    case performance = "Performance"

    var id: String {
        rawValue
    }

    var symbol: String {
        switch self {
        case .foundations: "paintpalette"
        case .controls: "slider.horizontal.3"
        case .chatList: "list.bullet.rectangle"
        case .conversation: "bubble.left.and.bubble.right"
        case .panes: "rectangle.split.3x1"
        case .overlays: "command"
        case .states: "moon.zzz"
        case .screens: "macwindow"
        case .live: "play.circle"
        case .performance: "gauge.with.dots.needle.67percent"
        }
    }
}

/// What a scene is drawn in: the controls under the scene list, as values a scene hands to the
/// components it shows, as the app hands them its preferences.
struct Conditions: Hashable, Sendable {
    var theme = Catalog.jaw.id
    var dark = true
    var look: Look = .bubbles
    var density: Density = .comfortable
    var textSize: CGFloat = 13
    var font: MessageFont = .system
    var reduceMotion = false

    var roles: Roles {
        Catalog.family(theme).roles(dark: dark)
    }

    var style: Style {
        Style(roles: roles, look: look, density: density, textSize: textSize, font: font)
    }
}

/// What a sweep draws a scene in besides dark and light, which it draws every scene in.
struct Axes: OptionSet, Hashable, Sendable {
    let rawValue: Int

    /// Bubbles and lines, for a scene that draws messages.
    static let look = Axes(rawValue: 1 << 0)
    /// Comfortable and compact, where density changes what's drawn.
    static let density = Axes(rawValue: 1 << 1)
}

/// One of the conditions a sweep draws a scene in, named as its picture is: `dark`, `light-lines`,
/// `dark-bubbles-compact`.
struct Variant: Hashable, Sendable {
    var dark: Bool
    var look: Look?
    var density: Density?

    var name: String {
        [dark ? "dark" : "light", look?.rawValue, density == .compact ? "compact" : nil]
            .compactMap { $0 }
            .joined(separator: "-")
    }

    /// `conditions` in this variant's appearance, and in its look and density where it names them.
    func applied(to conditions: Conditions) -> Conditions {
        var c = conditions
        c.dark = dark
        if let look { c.look = look }
        if let density { c.density = density }
        return c
    }

    /// Dark before light, bubbles before lines, comfortable before compact.
    static func all(_ axes: Axes) -> [Variant] {
        let looks: [Look?] = axes.contains(.look) ? Look.allCases.map(Optional.some) : [nil]
        let densities: [Density?] = axes.contains(.density) ? Density.allCases.map(Optional.some) : [nil]
        return [true, false].flatMap { dark in
            looks.flatMap { look in densities.map { Variant(dark: dark, look: look, density: $0) } }
        }
    }
}

/// One entry in the harness: a component, or a screen, in the states worth reviewing.
struct HarnessScene: Identifiable {
    let id: String
    let title: String
    let symbol: String
    /// One line on what to look at, shown under the window's title: what turns a gallery into a
    /// review.
    let synopsis: String
    let section: HarnessSection
    /// The `JawUI` types it shows, which `scripts/check-harness` reads.
    let covers: [Any.Type]
    /// What a sweep draws it in besides dark and light.
    let axes: Axes
    /// How wide it's drawn, in points; it's as tall as it needs.
    let width: CGFloat
    /// Built only when it's shown, so a scene that isn't costs nothing.
    let content: @MainActor (Conditions) -> AnyView
    /// A pane of knobs beside the stage, for a scene that has one.
    var inspector: (@MainActor () -> AnyView)?

    init(id: String, title: String, symbol: String, synopsis: String, section: HarnessSection, covers: [Any.Type],
         axes: Axes = [], width: CGFloat = 960, @ViewBuilder content: @escaping @MainActor (Conditions) -> some View)
    {
        self.id = id
        self.title = title
        self.symbol = symbol
        self.synopsis = synopsis
        self.section = section
        self.covers = covers
        self.axes = axes
        self.width = width
        self.content = { AnyView(content($0)) }
    }

    /// The conditions a sweep draws it in.
    var variants: [Variant] {
        Variant.all(axes)
    }
}

/// The scenes, in the order they're registered. Built once and kept: a scene's knobs keep their
/// models in it, and a catalog made again would quietly hand the stage new ones.
struct HarnessCatalog {
    private(set) var scenes: [HarnessScene] = []

    mutating func register(_ scene: HarnessScene) {
        precondition(!scenes.contains { $0.id == scene.id }, "two harness scenes are called \(scene.id)")
        scenes.append(scene)
    }

    /// The sections that have scenes, in sidebar order, so an empty one never shows as a heading.
    var sections: [HarnessSection] {
        HarnessSection.allCases.filter { section in scenes.contains { $0.section == section } }
    }

    func scenes(in section: HarnessSection) -> [HarnessScene] {
        scenes.filter { $0.section == section }
    }

    func scene(id: String?) -> HarnessScene? {
        scenes.first { $0.id == id }
    }
}
```

- [ ] **Step 5: Write the flags**

`mac/JawHarness/Sources/HarnessLaunch.swift`:

```swift
import Foundation
import JawThemes
import JawUI

/// What the harness was launched to do, from its command line:
///
/// - `--scene <id>` opens a scene.
/// - `--theme <id>`, `--appearance dark|light`, `--look bubbles|lines`, `--density comfortable|compact`,
///   `--text-size <11…17>`, `--font system|mono` and `--reduce-motion` set the conditions.
/// - `--window <width>x<height>` sizes the window, in points.
/// - `--stage-only` hides the scene list and the inspector.
/// - `--snapshot <png>` draws the scene into a picture, and quits.
/// - `--sweep <folder>` draws every scene in each of its variants, with `manifest.json`, and quits.
///
/// A value it doesn't know leaves what it sets as it was.
struct HarnessLaunch: Equatable {
    struct Size: Hashable, Sendable {
        var width: CGFloat
        var height: CGFloat
    }

    var scene: String?
    var conditions = Conditions()
    var window: Size?
    var stageOnly = false
    var snapshot: String?
    var sweep: String?

    @MainActor static let current = parse(CommandLine.arguments)

    @MainActor
    static func parse(_ arguments: [String]) -> HarnessLaunch {
        func value(_ flag: String) -> String? {
            arguments.firstIndex(of: flag).flatMap { $0 + 1 < arguments.count ? arguments[$0 + 1] : nil }
        }
        var launch = HarnessLaunch()
        launch.scene = value("--scene")
        if let theme = value("--theme"), Catalog.families.contains(where: { $0.id == theme }) {
            launch.conditions.theme = theme
        }
        switch value("--appearance") {
        case "dark": launch.conditions.dark = true
        case "light": launch.conditions.dark = false
        default: break
        }
        if let look = value("--look").flatMap(Look.init(rawValue:)) {
            launch.conditions.look = look
        }
        if let density = value("--density").flatMap(Density.init(rawValue:)) {
            launch.conditions.density = density
        }
        if let size = value("--text-size").flatMap(Double.init) {
            let sizes = Preferences.textSizes
            launch.conditions.textSize = min(max(CGFloat(size), sizes.lowerBound), sizes.upperBound)
        }
        if let font = value("--font").flatMap(MessageFont.init(rawValue:)) {
            launch.conditions.font = font
        }
        launch.conditions.reduceMotion = arguments.contains("--reduce-motion")
        let size = value("--window")?.split(separator: "x").compactMap { Double($0) } ?? []
        if size.count == 2 {
            launch.window = Size(width: size[0], height: size[1])
        }
        launch.stageOnly = arguments.contains("--stage-only")
        launch.snapshot = value("--snapshot")
        launch.sweep = value("--sweep")
        return launch
    }
}
```

- [ ] **Step 6: Run the tests to see them pass**

Run: `mac/scripts/build-mac test JawHarnessTests`
Expected: `Test run with 6 tests` … `passed`, and `** TEST SUCCEEDED **`. If the test bundle can't link `JawUI`'s symbols, or the run prints `Class … is implemented in both`, take the three frameworks out of the test target's dependencies (the host app's copies are the ones loaded) and run again; say in the report which way it went.

- [ ] **Step 7: Commit**

```bash
git add mac/Project.swift Makefile mac/JawHarness/Sources/HarnessScene.swift mac/JawHarness/Sources/HarnessLaunch.swift mac/Tests/JawHarnessTests
git commit -F- <<'EOF'
Jaw Harness's scenes as values: a catalog built once that refuses two scenes of one id and lists its sections in the sidebar's order, the conditions a scene is drawn in (the theme, dark or light, the look, the density, the text size, the font and Reduce Motion) as the app's own style, the variants a sweep draws a scene in and how each is named, from dark to light-lines-compact, and the launch flags that set them, a value they don't know leaving what it sets as it was; JawHarnessTests, hosted by the harness, runs in make mac-test
EOF
```

---

### Task 2: The window, the stage and `--snapshot`

**Files:**
- Create: `mac/JawHarness/Sources/HarnessApp.swift`, `BuiltInScenes.swift`, `HarnessRootView.swift`, `ConditionsControls.swift`, `Stage.swift`, `Specimen.swift`, `Pictures.swift`, `Scenes/FoundationScenes.swift` (all under `mac/JawHarness/Sources/`)
- Create: `mac/Tests/JawHarnessTests/PicturesTests.swift`
- Delete: `mac/JawHarness/Sources/main.swift`
- Modify: `mac/JawHarness/Sources/HarnessScene.swift` (the shared catalog), `mac/scripts/build-mac` (`shot`)

**Interfaces:**
- Consumes: everything Task 1 produced.
- Produces:
  - `extension HarnessCatalog { @MainActor static let shared: HarnessCatalog }`, from `BuiltInScenes.catalog()`.
  - `enum BuiltInScenes { @MainActor static func catalog() -> HarnessCatalog }`, registering `.roles` and `.mimic`.
  - `extension HarnessScene { static var roles: HarnessScene; static var mimic: HarnessScene }`.
  - `enum StageSurface: String, CaseIterable, Identifiable { case ground, surface, window; func color(_: Roles) -> Color }`.
  - `struct StageSettings { var conditions: Conditions; var surface: StageSurface; var inspectorShown: Bool; init(launch: HarnessLaunch) }`.
  - `struct SceneCanvas: View { let scene: HarnessScene; let conditions: Conditions; var surface: StageSurface = .ground }`.
  - `struct Note: Hashable, Codable, Sendable { let title: String; let note: String }`, `struct NotesKey: PreferenceKey`.
  - `struct SpecimenGroup<Content: View>: View` with `init(title: String, note: String, roles: Roles, @ViewBuilder content: () -> Content)`.
  - `struct Specimen<Content: View>: View` with `init(caption: String, roles: Roles, @ViewBuilder content: () -> Content)`.
  - `struct ControllerSpecimen: NSViewControllerRepresentable` with `init(make: @escaping @MainActor () -> NSViewController)`.
  - `@MainActor enum Pictures { static let scale: CGFloat = 2; static let settle: Duration; enum PictureError: Error, CustomStringConvertible { case noScene(String), cantDraw(String) }; struct Drawn { let png: Data; let points: CGSize; let pixels: [Int]; let hash: String; let notes: [Note] }; static func draw(_: HarnessScene, _: Conditions) async throws -> Drawn; static func snapshot(_: HarnessLaunch, catalog: HarnessCatalog, to: URL) async throws }`.
  - `mac/scripts/build-mac shot <scene> <out.png> [harness options]`.

- [ ] **Step 1: Write the failing tests**

`mac/Tests/JawHarnessTests/PicturesTests.swift`:

```swift
import AppKit
@testable import JawHarness
import JawUI
import SwiftUI
import Testing

/// Scenes drawn into pictures: at their width with the stage's margin around them, at 2×, the same
/// each time, carrying their bands' notes.

func block(_ id: String = "block", axes: Axes = []) -> HarnessScene {
    HarnessScene(id: id, title: "Block", symbol: "square", synopsis: "a block of the accent", section: .foundations,
                 covers: [], axes: axes, width: 100) { c in
        Rectangle().fill(c.roles.accent.color).frame(width: 100, height: 50)
    }
}

/// A folder of the test's own, which it deletes.
func scratch(_ name: String) -> URL {
    FileManager.default.temporaryDirectory.appending(path: "jaw-harness-tests-\(name)-\(UUID().uuidString)")
}

@MainActor @Test func `a scene is drawn at its width with the stage's margin around it, at 2×, the same each time`() async throws {
    let dark = try await Pictures.draw(block(), Conditions())
    #expect(dark.points == CGSize(width: 148, height: 98))
    #expect(dark.pixels == [296, 196])
    #expect(dark.hash.count == 16)
    let again = try await Pictures.draw(block(), Conditions())
    #expect(again.hash == dark.hash, "nothing moves between pictures")
    var light = Conditions()
    light.dark = false
    let lit = try await Pictures.draw(block(), light)
    #expect(lit.hash != dark.hash, "dark and light are different pictures")
}

@MainActor @Test func `a picture carries the notes of the bands drawn in it`() async throws {
    let scene = HarnessScene(id: "noted", title: "Noted", symbol: "note", synopsis: "", section: .foundations, covers: [],
                             width: 300) { c in
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(title: "One", note: "the first", roles: c.roles) { Text("1").textStyle(.body) }
            SpecimenGroup(title: "Two", note: "the second", roles: c.roles) { Text("2").textStyle(.body) }
        }
    }
    let drawn = try await Pictures.draw(scene, Conditions())
    #expect(drawn.notes == [Note(title: "One", note: "the first"), Note(title: "Two", note: "the second")])
}

@MainActor @Test func `a snapshot draws the scene it's asked for, and fails on one there isn't`() async throws {
    var catalog = HarnessCatalog()
    catalog.register(block())
    let folder = scratch("snapshot")
    defer { try? FileManager.default.removeItem(at: folder) }
    var launch = HarnessLaunch.parse(["JawHarness", "--scene", "block"])
    try await Pictures.snapshot(launch, catalog: catalog, to: folder.appending(path: "block.png"))
    let rep = try #require(NSBitmapImageRep(data: Data(contentsOf: folder.appending(path: "block.png"))))
    #expect(rep.pixelsWide == 296 && rep.pixelsHigh == 196)
    launch.scene = "nope"
    await #expect(throws: Pictures.PictureError.self) {
        try await Pictures.snapshot(launch, catalog: catalog, to: folder.appending(path: "nope.png"))
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawHarnessTests`
Expected: the build fails: `cannot find 'Pictures' in scope`, `cannot find 'SpecimenGroup' in scope`, `cannot find 'Note' in scope`.

- [ ] **Step 3: Replace the old harness with the app**

Delete `mac/JawHarness/Sources/main.swift` (`git rm mac/JawHarness/Sources/main.swift`).

`mac/JawHarness/Sources/HarnessApp.swift`:

```swift
import AppKit
import SwiftUI

/// Jaw Harness: the app's components and screens in the states worth reviewing, drawn from made-up
/// rows and the protocol's fixtures, as Redlamp's and omoi-station's harnesses draw theirs. It links
/// the app's own frameworks and asks no jawd for anything.
@main
struct HarnessApp: App {
    @NSApplicationDelegateAdaptor(HarnessDelegate.self) private var delegate

    var body: some Scene {
        WindowGroup("Jaw Harness") {
            HarnessRootView()
                .frame(minWidth: 900, minHeight: 600)
        }
        .defaultSize(width: 1400, height: 900)
    }
}

/// Draws what `--snapshot` asks for once the app has started, then quits; a failure says why on
/// standard error and quits with status 1, so a script stops on it.
final class HarnessDelegate: NSObject, NSApplicationDelegate {
    @MainActor
    func applicationDidFinishLaunching(_: Notification) {
        let launch = HarnessLaunch.current
        guard let snapshot = launch.snapshot else { return }
        Task { @MainActor in
            do {
                try await Pictures.snapshot(launch, catalog: HarnessCatalog.shared, to: URL(fileURLWithPath: snapshot))
                NSApp.terminate(nil)
            } catch {
                FileHandle.standardError.write(Data("jaw-harness: \(error)\n".utf8))
                exit(1)
            }
        }
    }
}
```

At the end of `mac/JawHarness/Sources/HarnessScene.swift` add:

```swift
extension HarnessCatalog {
    /// The harness's own scenes, made once for the life of the app.
    @MainActor static let shared = BuiltInScenes.catalog()
}
```

`mac/JawHarness/Sources/BuiltInScenes.swift`:

```swift
/// The harness's table of contents: a scene's line here puts it in the sidebar, in this order within
/// its section. The scene itself is in a file of its own under `Scenes/`.
enum BuiltInScenes {
    @MainActor
    static func catalog() -> HarnessCatalog {
        var catalog = HarnessCatalog()
        catalog.register(.roles)
        catalog.register(.mimic)
        return catalog
    }
}
```

- [ ] **Step 4: Write the sidebar, the controls and the stage**

`mac/JawHarness/Sources/HarnessRootView.swift`:

```swift
import AppKit
import JawUI
import SwiftUI

/// The scenes by section on the left, the conditions under them, and the stage on the right. The
/// conditions live here rather than in the stage, so moving to another scene keeps what you were
/// judging the last one in.
@MainActor
struct HarnessRootView: View {
    private let catalog = HarnessCatalog.shared
    @State private var selection: String? = HarnessLaunch.current.scene ?? HarnessCatalog.shared.scenes.first?.id
    @State private var settings = StageSettings(launch: HarnessLaunch.current)
    @State private var columns: NavigationSplitViewVisibility = HarnessLaunch.current.stageOnly ? .detailOnly : .automatic

    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            List(selection: $selection) {
                ForEach(catalog.sections) { section in
                    Section {
                        ForEach(catalog.scenes(in: section)) { scene in
                            Label(scene.title, systemImage: scene.symbol).tag(scene.id)
                        }
                    } header: {
                        Label(section.rawValue, systemImage: section.symbol)
                    }
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) { ConditionsControls(conditions: $settings.conditions) }
            .navigationSplitViewColumnWidth(min: 220, ideal: 250, max: 320)
        } detail: {
            if let scene = catalog.scene(id: selection) {
                Stage(scene: scene, settings: $settings)
            } else {
                ContentUnavailableView("Choose a scene", systemImage: "square.grid.2x2")
            }
        }
        .preferredColorScheme(settings.conditions.dark ? .dark : .light)
        // AppKit's views take the app's appearance, not SwiftUI's colour scheme.
        .onChange(of: settings.conditions.dark, initial: true) { _, dark in
            NSApp.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        }
        .onChange(of: settings.conditions.reduceMotion, initial: true) { _, reduced in
            Motion.reduced = { reduced }
        }
        .task { await HarnessWindow.size(HarnessLaunch.current.window) }
    }
}

/// `--window`: the harness's window sized as asked once it's on screen, and kept out of its saved
/// frame, since a scripted size isn't the owner's.
enum HarnessWindow {
    @MainActor
    static func size(_ size: HarnessLaunch.Size?) async {
        guard let size else { return }
        var window: NSWindow?
        for _ in 0 ..< 50 {
            window = NSApp.windows.first { $0.isVisible && $0.canBecomeMain }
            if window != nil { break }
            try? await Task.sleep(for: .milliseconds(100))
        }
        guard let window, let area = (window.screen ?? NSScreen.main)?.visibleFrame else { return }
        window.setFrameAutosaveName("")
        window.setFrame(NSRect(x: area.midX - size.width / 2, y: area.midY - size.height / 2, width: size.width,
                               height: size.height), display: true)
    }
}
```

`mac/JawHarness/Sources/ConditionsControls.swift`:

```swift
import JawThemes
import JawUI
import SwiftUI

/// The conditions every scene is drawn in, under the scene list, where they hold from scene to scene:
/// the theme, dark or light, the look, the density, the text size, the font and Reduce Motion.
struct ConditionsControls: View {
    @Binding var conditions: Conditions

    var body: some View {
        VStack(alignment: .leading, spacing: Space.m) {
            Picker("Theme", selection: $conditions.theme) {
                ForEach(Catalog.families) { Text($0.name).tag($0.id) }
            }
            Picker("Appearance", selection: $conditions.dark) {
                Text("Dark").tag(true)
                Text("Light").tag(false)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            Picker("Look", selection: $conditions.look) {
                ForEach(Look.allCases, id: \.self) { Text($0.title).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            Picker("Density", selection: $conditions.density) {
                ForEach(Density.allCases, id: \.self) { Text($0.title).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            HStack(spacing: Space.m) {
                Text("Text").textStyle(.callout)
                Slider(value: $conditions.textSize, in: Preferences.textSizes, step: 1)
                Text("\(Int(conditions.textSize)) pt").textStyle(.caption).monospacedDigit()
            }
            Picker("Font", selection: $conditions.font) {
                ForEach(MessageFont.allCases, id: \.self) { Text($0.title).tag($0) }
            }
            Toggle("Reduce Motion", isOn: $conditions.reduceMotion)
        }
        .padding(Space.l)
        .background(.bar)
        .overlay(alignment: .top) { Divider() }
    }
}
```

`mac/JawHarness/Sources/Stage.swift`:

```swift
import AppKit
import JawThemes
import JawUI
import SwiftUI

/// What the stage puts behind a scene: a component is judged on the surface it ships on.
enum StageSurface: String, CaseIterable, Identifiable {
    /// The window's ground: the conversation, the panes.
    case ground = "Ground"
    /// The sidebar's, the list's, and their bubbles'.
    case surface = "Surface"
    /// The system's window colour, which sheets sit on.
    case window = "Window"

    var id: String {
        rawValue
    }

    func color(_ roles: Roles) -> Color {
        switch self {
        case .ground: roles.ground.color
        case .surface: roles.surface.color
        case .window: Color(nsColor: .windowBackgroundColor)
        }
    }
}

/// The stage's settings, kept by the root view so they hold from scene to scene.
struct StageSettings {
    var conditions: Conditions
    var surface = StageSurface.ground
    var inspectorShown: Bool

    init(launch: HarnessLaunch) {
        conditions = launch.conditions
        inspectorShown = !launch.stageOnly
    }
}

/// A scene drawn on its surface in its conditions, `width` points wide and as tall as it needs, with
/// the stage's margin around it: as the stage shows it, and as a picture is drawn.
struct SceneCanvas: View {
    let scene: HarnessScene
    let conditions: Conditions
    var surface = StageSurface.ground

    var body: some View {
        scene.content(conditions)
            .frame(width: scene.width, alignment: .topLeading)
            .padding(Space.xxl)
            .background(surface.color(conditions.roles))
            .environment(\.colorScheme, conditions.dark ? .dark : .light)
    }
}

/// One scene on the stage: on the chosen surface, in the conditions, with its inspector.
struct Stage: View {
    let scene: HarnessScene
    @Binding var settings: StageSettings

    var body: some View {
        ScrollView([.horizontal, .vertical]) {
            SceneCanvas(scene: scene, conditions: settings.conditions, surface: settings.surface)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .background(settings.surface.color(settings.conditions.roles))
        // AppKit views take their colours when they're made, so other conditions make them again.
        .id(StageIdentity(scene: scene.id, conditions: settings.conditions))
        .navigationTitle(scene.title)
        .navigationSubtitle(scene.synopsis)
        .inspector(isPresented: inspectorShown) {
            scene.inspector?()
                .inspectorColumnWidth(min: 260, ideal: 300, max: 420)
        }
        .toolbar {
            ToolbarItem {
                Picker("Surface", selection: $settings.surface) {
                    ForEach(StageSurface.allCases) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented)
                .help("The surface behind the scene")
            }
            if scene.inspector != nil {
                ToolbarItem {
                    Toggle(isOn: $settings.inspectorShown) {
                        Label("Inspector", systemImage: "sidebar.right")
                    }
                    .help("The scene's knobs")
                }
            }
        }
    }

    /// Only a scene with an inspector opens one, so no empty pane shows beside the others.
    private var inspectorShown: Binding<Bool> {
        let settings = $settings
        let hasInspector = scene.inspector != nil
        return Binding(get: { settings.wrappedValue.inspectorShown && hasInspector },
                       set: { settings.wrappedValue.inspectorShown = $0 })
    }
}

private struct StageIdentity: Hashable {
    let scene: String
    let conditions: Conditions
}
```

- [ ] **Step 5: Write the specimens**

`mac/JawHarness/Sources/Specimen.swift`:

```swift
import AppKit
import JawThemes
import JawUI
import SwiftUI

/// What a band of specimens says to look for. The bands a scene draws hand theirs up, so a sweep's
/// manifest carries each scene's notes and the review canvas shows them beside its pictures.
struct Note: Hashable, Codable, Sendable {
    let title: String
    let note: String
}

struct NotesKey: PreferenceKey {
    static let defaultValue: [Note] = []

    static func reduce(value: inout [Note], nextValue: () -> [Note]) {
        value += nextValue()
    }
}

/// A titled band of specimens with a note on what would be wrong with them: a gallery shows you a
/// component, and a note on what to look for turns looking into reviewing.
struct SpecimenGroup<Content: View>: View {
    let title: String
    let note: String
    let roles: Roles
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: Space.l) {
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(title).textStyle(.heading).tone(.primary, roles)
                Text(note).textStyle(.callout).tone(.secondary, roles)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: 640, alignment: .leading)
            }
            content
        }
        .padding(.bottom, Space.xxl)
        .preference(key: NotesKey.self, value: [Note(title: title, note: note)])
    }
}

/// One component with a monospaced caption under it.
struct Specimen<Content: View>: View {
    let caption: String
    let roles: Roles
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            content
            Text(caption).textStyle(.mono).tone(.tertiary, roles)
        }
    }
}

/// An AppKit controller where SwiftUI puts it, at the frame it's given: a chat list, a conversation.
struct ControllerSpecimen: NSViewControllerRepresentable {
    let make: @MainActor () -> NSViewController

    func makeNSViewController(context _: Context) -> NSViewController {
        make()
    }

    func updateNSViewController(_: NSViewController, context _: Context) {}
}
```

- [ ] **Step 6: Write the pictures**

`mac/JawHarness/Sources/Pictures.swift`:

```swift
import AppKit
import CryptoKit
import JawUI
import SwiftUI

/// Scenes drawn into pictures by their own views, off the screen, so no Screen Recording is needed:
/// one for `--snapshot`, and every scene in each of its variants for `--sweep`.
@MainActor
enum Pictures {
    /// Pictures are drawn at 2×, as a Retina screen shows them.
    static let scale: CGFloat = 2
    /// How long a scene is given before it's drawn, for what it does once it's made: rows applied on
    /// the next turn and laid out off the main thread.
    static let settle = Duration.milliseconds(400)

    enum PictureError: Error, CustomStringConvertible {
        case noScene(String)
        case cantDraw(String)

        var description: String {
            switch self {
            case let .noScene(id): "there's no scene called \(id)"
            case let .cantDraw(id): "\(id) couldn't be drawn"
            }
        }
    }

    /// A scene drawn: the picture, its size in points and in pixels, a hash of its pixels, which
    /// changes exactly when the picture does, and the notes its bands carried.
    struct Drawn {
        let png: Data
        let points: CGSize
        let pixels: [Int]
        let hash: String
        let notes: [Note]
    }

    /// Draws `scene` in `conditions`, in a window of its own off the screen, once it has had `settle`
    /// to do what it does after it's made.
    static func draw(_ scene: HarnessScene, _ conditions: Conditions) async throws -> Drawn {
        let taker = NoteTaker()
        let canvas = SceneCanvas(scene: scene, conditions: conditions)
            .onPreferenceChange(NotesKey.self) { notes in
                MainActor.assumeIsolated { taker.notes = notes }
            }
        let host = NSHostingView(rootView: AnyView(canvas))
        let size = host.fittingSize
        let window = NSWindow(contentRect: NSRect(origin: NSPoint(x: -30000, y: -30000), size: size), styleMask: [.borderless],
                              backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.appearance = NSAppearance(named: conditions.dark ? .darkAqua : .aqua)
        window.contentView = host
        window.orderFront(nil)
        defer { window.orderOut(nil) }
        try await Task.sleep(for: settle)
        host.layoutSubtreeIfNeeded()
        let bounds = host.bounds
        guard bounds.width >= 1, bounds.height >= 1,
              let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int((bounds.width * scale).rounded()),
                                         pixelsHigh: Int((bounds.height * scale).rounded()), bitsPerSample: 8,
                                         samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                         bytesPerRow: 0, bitsPerPixel: 0)
        else { throw PictureError.cantDraw(scene.id) }
        rep.size = bounds.size
        host.cacheDisplay(in: bounds, to: rep)
        guard let png = rep.representation(using: .png, properties: [:]), let pixels = rep.bitmapData else {
            throw PictureError.cantDraw(scene.id)
        }
        let digest = SHA256.hash(data: Data(bytes: pixels, count: rep.bytesPerRow * rep.pixelsHigh))
        let hash = String(digest.map { String(format: "%02x", $0) }.joined().prefix(16))
        return Drawn(png: png, points: bounds.size, pixels: [rep.pixelsWide, rep.pixelsHigh], hash: hash, notes: taker.notes)
    }

    /// `--snapshot`: the scene `--scene` names, or the first, in the conditions the flags set.
    static func snapshot(_ launch: HarnessLaunch, catalog: HarnessCatalog, to url: URL) async throws {
        let scene = launch.scene == nil ? catalog.scenes.first : catalog.scene(id: launch.scene)
        guard let scene else { throw PictureError.noScene(launch.scene ?? "(none)") }
        let drawn = try await draw(scene, launch.conditions)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try drawn.png.write(to: url)
    }
}

/// The notes a scene's bands handed up as SwiftUI laid it out.
@MainActor
private final class NoteTaker {
    var notes: [Note] = []
}
```

If a picture comes out blank or without its AppKit views (Step 10 shows), the window off the screen is the first suspect: put it on the main screen's frame instead, ordered behind every other window (`window.orderBack(nil)`), and say so in the report.

- [ ] **Step 7: Write the first two scenes**

`mac/JawHarness/Sources/Scenes/FoundationScenes.swift`:

```swift
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var roles: HarnessScene {
        HarnessScene(
            id: "roles", title: "Roles", symbol: "swatchpalette",
            synopsis: "Every colour the app asks for by what it's for, with the contrast of what's drawn on it",
            section: .foundations, covers: [],
        ) { RolesScene(roles: $0.roles) }
    }

    static var mimic: HarnessScene {
        HarnessScene(
            id: "mimic", title: "The Mimic", symbol: "mouth",
            synopsis: "Jaw's creature awake, mid-chomp and asleep, at whole multiples of its pixels",
            section: .foundations, covers: [MimicImage.self, MimicView.self],
        ) { MimicScene(roles: $0.roles) }
    }
}

/// The theme's roles as swatches: each with "Aa" in the text colour and the contrast it reads at, and
/// the senders' colours with the contrast they read at on the surface their names sit on.
private struct RolesScene: View {
    let roles: Roles

    var named: [(String, RGBA)] {
        [
            ("ground", roles.ground), ("surface", roles.surface), ("selection", roles.selection), ("text", roles.text),
            ("secondary", roles.secondary), ("tertiary", roles.tertiary), ("separator", roles.separator),
            ("accent", roles.accent), ("yours", roles.yours), ("mention", roles.mention), ("online", roles.online),
            ("failed", roles.failed),
        ]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "The roles",
                note: "Each with \"Aa\" in the text colour. The grounds text sits on (ground, surface, selection, yours) want 4.5:1 or more; the others aren't for text, so a low ratio on them is right.",
                roles: roles,
            ) {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 140), spacing: Space.l)], spacing: Space.l) {
                    ForEach(named, id: \.0) { name, colour in
                        Specimen(caption: "\(name) · " + String(format: "%.1f:1", roles.text.contrast(with: colour)), roles: roles) {
                            RoundedRectangle(cornerRadius: Radius.control).fill(colour.color).frame(height: 50)
                                .overlay(Text("Aa").textStyle(.body).tone(.primary, roles))
                        }
                    }
                }
            }
            SpecimenGroup(
                title: "The senders",
                note: "Names in a group, by the index jawd gives each member, on the surface their bubbles are. Each should read at 4.5:1 or more there; one under it is a name that's hard to read.",
                roles: roles,
            ) {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 140), spacing: Space.l)], spacing: Space.l) {
                    ForEach(Array(roles.senders.enumerated()), id: \.offset) { index, colour in
                        Specimen(caption: "sender \(index) · " + String(format: "%.1f:1", colour.contrast(with: roles.surface)), roles: roles) {
                            RoundedRectangle(cornerRadius: Radius.control).fill(roles.surface.color).frame(height: 50)
                                .overlay(Text("Ana Ribeiro").textStyle(.body).foregroundStyle(colour.color))
                        }
                    }
                }
            }
        }
    }
}

/// The Mimic's three looks at 4×, as empty states and linking draw it, and the scales it's drawn at,
/// with a button that makes it chomp.
private struct MimicScene: View {
    let roles: Roles
    @State private var chomps = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "Its looks",
                note: "Awake in empty chats and linking, mid-chomp when a message comes into an empty chat, asleep (grey, eyes shut) when jawd is out of reach. Every pixel is a whole square: a soft edge means it was scaled by something other than a whole number.",
                roles: roles,
            ) {
                HStack(alignment: .bottom, spacing: Space.xxl) {
                    Specimen(caption: "awake", roles: roles) { MimicImage(look: .awake, scale: 4, chomps: chomps) }
                    Specimen(caption: "mid-chomp", roles: roles) { MimicImage(look: .chomp, scale: 4) }
                    Specimen(caption: "asleep", roles: roles) { MimicImage(look: .asleep, scale: 4) }
                    Button("Chomp") { chomps += 1 }
                }
            }
            SpecimenGroup(
                title: "Its scales",
                note: "1×, 2× and 8× its pixels. Each is crisp at its own size, with no pixel wider than its neighbours.",
                roles: roles,
            ) {
                HStack(alignment: .bottom, spacing: Space.xxl) {
                    Specimen(caption: "1×", roles: roles) { MimicImage(look: .awake, scale: 1) }
                    Specimen(caption: "2×", roles: roles) { MimicImage(look: .awake, scale: 2) }
                    Specimen(caption: "8×", roles: roles) { MimicImage(look: .awake, scale: 8) }
                }
            }
        }
    }
}
```

- [ ] **Step 8: Add `shot` to `build-mac`**

In `mac/scripts/build-mac`:

1. In the header's list of jobs, after the `snapshot` line, add:

```sh
#   mac/scripts/build-mac shot <scene> <out.png> [harness options]  # one harness scene drawn into a picture
```

2. After the line `what="${1:-}"; [ $# -gt 0 ] && shift`, add:

```sh
harness=build/DerivedData/Build/Products/Debug/JawHarness.app/Contents/MacOS/JawHarness
```

3. In the fallback `case` (when the build Mac doesn't answer), before `*) echo "$what needs $host" >&2; exit 1 ;;`, add:

```sh
        shot)
            scene="$1"; out="$2"; shift 2
            "$here/mac/scripts/check" build JawHarness | grep -q 'BUILD SUCCEEDED' || { "$here/mac/scripts/check" build JawHarness | tail -20; exit 1; }
            exec "$here/mac/$harness" --scene "$scene" --snapshot "$out" "$@"
            ;;
```

4. In the main `case`, after the `snapshot)` job, add:

```sh
    shot)
        scene="$1"; out="$2"; shift 2
        run "scripts/check build JawHarness | grep -q 'BUILD SUCCEEDED' || { scripts/check build JawHarness | tail -20; exit 1; }; $harness --scene $scene --snapshot build/shot.png $*"
        scp -q -o BatchMode=yes "$host:$remote/mac/build/shot.png" "$out" && echo "==> $out"
        ;;
```

5. In the usage line at the end, add `shot` to the list of jobs.

- [ ] **Step 9: Run the tests to see them pass**

Run: `mac/scripts/build-mac test JawHarnessTests`
Expected: `Test run with 9 tests` … `passed`, `** TEST SUCCEEDED **`.

- [ ] **Step 10: Look at both scenes**

Run:

```sh
mkdir -p mac/build/shots
mac/scripts/build-mac shot roles mac/build/shots/roles-dark.png
mac/scripts/build-mac shot mimic mac/build/shots/mimic-light.png --appearance light
```

Expected: `==> mac/build/shots/roles-dark.png` and `==> mac/build/shots/mimic-light.png`. Open each picture (the Read tool shows images). `roles-dark.png` is 2,016 pixels wide (1,008 points at 2×): "The roles" and its note, twelve swatches each with "Aa" and a caption of the role's name and its ratio (`ground · ` and a number, `:1`), then "The senders" with eight swatches of "Ana Ribeiro" in each colour, all on Tokyo Night's dark ground (`#1a1b26`). `mimic-light.png`: the Mimic three times at 4× beside a Chomp button, then at 1×, 2× and 8×, on Tokyo Night Day's light ground (`#e1e2e7`), with dark text. Nothing is blank, clipped or drawn on a different ground.

- [ ] **Step 11: Commit**

```bash
git add -A mac/JawHarness mac/Tests/JawHarnessTests/PicturesTests.swift mac/scripts/build-mac
git commit -F- <<'EOF'
Jaw Harness as a SwiftUI app in the way of Redlamp's and omoi-station's, replacing main.swift: the scenes by section, with the conditions under them held from scene to scene; the stage, each scene on the surface it ships on with its title, its synopsis and its inspector; bands of specimens with a note on what would be wrong with them; and --snapshot, the scene drawn by its own views in a window off the screen at 2×, so no Screen Recording is needed, which build-mac shot brings back; the theme's roles, with the contrast of what's drawn on each, and the Mimic are its first scenes
EOF
```

---

### Task 3: The sweep

**Files:**
- Modify: `mac/JawHarness/Sources/Pictures.swift` (the sweep and its manifest), `mac/JawHarness/Sources/HarnessApp.swift` (`--sweep`), `mac/scripts/build-mac` (`sweep`, `harness`)
- Modify: `mac/Tests/JawHarnessTests/PicturesTests.swift`

**Interfaces:**
- Consumes: `Pictures.draw`, `Pictures.PictureError`, `Note`, `HarnessCatalog`, `Variant` (Tasks 1 and 2); `block(_:axes:)` and `scratch(_:)` in `PicturesTests.swift`.
- Produces:
  - `Pictures.PictureError.notASweep(String)`.
  - `struct Pictures.Manifest: Codable, Equatable { let made: String; let scenes: [Scene] }` with `struct Scene: Codable, Equatable { let id, section, title, synopsis: String; let notes: [Note]; let pictures: [Picture] }` and `struct Picture: Codable, Equatable { let variant, file: String; let points: [Double]; let pixels: [Int]; let hash: String }`; `manifest.json` is it encoded with sorted keys.
  - `@discardableResult static func sweep(_ catalog: HarnessCatalog, conditions: Conditions, to folder: URL) async throws -> Manifest`.
  - `mac/scripts/build-mac sweep [harness options]` (pictures in `mac/build/review/`) and `mac/scripts/build-mac harness [scene] [harness options]` (the app in `mac/build/harness/`, opened here).

- [ ] **Step 1: Write the failing tests**

Append to `mac/Tests/JawHarnessTests/PicturesTests.swift`:

```swift
@MainActor @Test func `a sweep draws each scene in each of its variants and says what each picture is`() async throws {
    var catalog = HarnessCatalog()
    catalog.register(block())
    catalog.register(block("looks", axes: .look))
    let folder = scratch("sweep")
    defer { try? FileManager.default.removeItem(at: folder) }
    let manifest = try await Pictures.sweep(catalog, conditions: Conditions(), to: folder)
    #expect(manifest.scenes.map(\.id) == ["block", "looks"])
    #expect(manifest.scenes[0].pictures.map(\.file) == ["block/dark.png", "block/light.png"])
    #expect(manifest.scenes[1].pictures.map(\.variant) == ["dark-bubbles", "dark-lines", "light-bubbles", "light-lines"])
    #expect(manifest.scenes[0].pictures[0].points == [148, 98] && manifest.scenes[0].pictures[0].pixels == [296, 196])
    #expect(manifest.scenes[0].synopsis == "a block of the accent" && manifest.scenes[0].section == "Foundations")
    for picture in manifest.scenes.flatMap(\.pictures) {
        #expect(FileManager.default.fileExists(atPath: folder.appending(path: picture.file).path), "\(picture.file)")
    }
    let written = try JSONDecoder().decode(Pictures.Manifest.self, from: Data(contentsOf: folder.appending(path: "manifest.json")))
    #expect(written == manifest)
    let again = try await Pictures.sweep(catalog, conditions: Conditions(), to: folder)
    #expect(again.scenes == manifest.scenes, "a sweep of the same scenes draws the same pictures")
}

@MainActor @Test func `a sweep never empties a folder it didn't make`() async throws {
    let folder = scratch("keep")
    defer { try? FileManager.default.removeItem(at: folder) }
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    try Data("mine".utf8).write(to: folder.appending(path: "notes.txt"))
    var catalog = HarnessCatalog()
    catalog.register(block())
    await #expect(throws: Pictures.PictureError.self) {
        try await Pictures.sweep(catalog, conditions: Conditions(), to: folder)
    }
    #expect(FileManager.default.fileExists(atPath: folder.appending(path: "notes.txt").path))
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `mac/scripts/build-mac test JawHarnessTests`
Expected: the build fails: `type 'Pictures' has no member 'sweep'`, `'Manifest' is not a member type of enum 'JawHarness.Pictures'`.

- [ ] **Step 3: Write the sweep**

In `mac/JawHarness/Sources/Pictures.swift`, add a case to `PictureError` and its description:

```swift
        case notASweep(String)
```

```swift
            case let .notASweep(path): "\(path) has files a sweep didn't make in it; sweep into an empty folder, or one a sweep made"
```

and, inside `enum Pictures`, after `snapshot`:

```swift
    /// What a sweep drew, as `manifest.json` has it: each scene with its notes and its pictures.
    struct Manifest: Codable, Equatable {
        struct Picture: Codable, Equatable {
            let variant: String
            /// From the sweep's folder: `<scene>/<variant>.png`.
            let file: String
            let points: [Double]
            let pixels: [Int]
            let hash: String
        }

        struct Scene: Codable, Equatable {
            let id: String
            let section: String
            let title: String
            let synopsis: String
            let notes: [Note]
            let pictures: [Picture]
        }

        let made: String
        let scenes: [Scene]
    }

    /// `--sweep`: every scene in each of its variants, as `<scene>/<variant>.png` under `folder`, with
    /// `manifest.json` saying what each is. A folder a sweep made is emptied first; one with anything
    /// else in it is left as it is, and the sweep fails.
    @discardableResult
    static func sweep(_ catalog: HarnessCatalog, conditions: Conditions, to folder: URL) async throws -> Manifest {
        try clear(folder)
        var scenes: [Manifest.Scene] = []
        for scene in catalog.scenes {
            try FileManager.default.createDirectory(at: folder.appending(path: scene.id), withIntermediateDirectories: true)
            var pictures: [Manifest.Picture] = []
            var notes: [Note] = []
            for variant in scene.variants {
                let drawn = try await draw(scene, variant.applied(to: conditions))
                let file = "\(scene.id)/\(variant.name).png"
                try drawn.png.write(to: folder.appending(path: file))
                pictures.append(Manifest.Picture(variant: variant.name, file: file,
                                                 points: [Double(drawn.points.width), Double(drawn.points.height)],
                                                 pixels: drawn.pixels, hash: drawn.hash))
                if notes.isEmpty { notes = drawn.notes }
            }
            scenes.append(Manifest.Scene(id: scene.id, section: scene.section.rawValue, title: scene.title,
                                         synopsis: scene.synopsis, notes: notes, pictures: pictures))
        }
        let manifest = Manifest(made: ISO8601DateFormatter().string(from: Date()), scenes: scenes)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try encoder.encode(manifest).write(to: folder.appending(path: "manifest.json"))
        return manifest
    }

    /// Readies `folder` for a sweep: one a sweep made is emptied, a missing one made, and one with
    /// anything else in it refused, so a mistyped path can't lose what's there.
    private static func clear(_ folder: URL) throws {
        let fm = FileManager.default
        let inside = ((try? fm.contentsOfDirectory(atPath: folder.path)) ?? []).filter { !$0.hasPrefix(".") }
        if inside.contains("manifest.json") {
            try fm.removeItem(at: folder)
        } else if !inside.isEmpty {
            throw PictureError.notASweep(folder.path)
        }
        try fm.createDirectory(at: folder, withIntermediateDirectories: true)
    }
```

In `mac/JawHarness/Sources/HarnessApp.swift`, replace `HarnessDelegate` with:

```swift
/// Draws what `--snapshot` or `--sweep` asks for once the app has started, then quits; a failure says
/// why on standard error and quits with status 1, so a script stops on it.
final class HarnessDelegate: NSObject, NSApplicationDelegate {
    @MainActor
    func applicationDidFinishLaunching(_: Notification) {
        let launch = HarnessLaunch.current
        guard launch.snapshot != nil || launch.sweep != nil else { return }
        Task { @MainActor in
            do {
                if let snapshot = launch.snapshot {
                    try await Pictures.snapshot(launch, catalog: HarnessCatalog.shared, to: URL(fileURLWithPath: snapshot))
                }
                if let folder = launch.sweep {
                    let manifest = try await Pictures.sweep(HarnessCatalog.shared, conditions: launch.conditions,
                                                            to: URL(fileURLWithPath: folder))
                    let pictures = manifest.scenes.reduce(0) { $0 + $1.pictures.count }
                    print("swept \(manifest.scenes.count) scenes into \(pictures) pictures in \(folder)")
                }
                NSApp.terminate(nil)
            } catch {
                FileHandle.standardError.write(Data("jaw-harness: \(error)\n".utf8))
                exit(1)
            }
        }
    }
}
```

- [ ] **Step 4: Add `sweep` and `harness` to `build-mac`**

In `mac/scripts/build-mac`:

1. In the header's list, after the `shot` line, add:

```sh
#   mac/scripts/build-mac sweep [harness options]                   # every harness scene in each variant, into mac/build/review
#   mac/scripts/build-mac harness [scene] [harness options]         # the harness, built there and opened here
```

2. In the fallback `case`, after `shot)`'s job, add:

```sh
        sweep)
            "$here/mac/scripts/check" build JawHarness | grep -q 'BUILD SUCCEEDED' || { "$here/mac/scripts/check" build JawHarness | tail -20; exit 1; }
            "$here/mac/$harness" --sweep "$here/mac/build/review" "$@"
            exit
            ;;
        harness)
            "$here/mac/scripts/check" build JawHarness | grep -q 'BUILD SUCCEEDED' || { "$here/mac/scripts/check" build JawHarness | tail -20; exit 1; }
            if [ $# -gt 0 ]; then scene="$1"; shift; set -- --scene "$scene" "$@"; fi
            exec open -n "$here/mac/build/DerivedData/Build/Products/Debug/JawHarness.app" --args "$@"
            ;;
```

3. In the main `case`, after `shot)`, add:

```sh
    sweep)
        run "scripts/check build JawHarness | grep -q 'BUILD SUCCEEDED' || { scripts/check build JawHarness | tail -20; exit 1; }; $harness --sweep build/review $*"
        mkdir -p "$here/mac/build/review"
        rsync -a --delete -e "ssh -o BatchMode=yes" "$host:$remote/mac/build/review/" "$here/mac/build/review/"
        echo "==> $here/mac/build/review"
        ;;
    harness)
        run "scripts/check build JawHarness | grep -q 'BUILD SUCCEEDED' || { scripts/check build JawHarness | tail -20; exit 1; }"
        mkdir -p "$here/mac/build/harness"
        rsync -a --delete -e "ssh -o BatchMode=yes" "$host:$remote/mac/build/DerivedData/Build/Products/Debug/JawHarness.app/" "$here/mac/build/harness/JawHarness.app/"
        if [ $# -gt 0 ]; then scene="$1"; shift; set -- --scene "$scene" "$@"; fi
        open -n "$here/mac/build/harness/JawHarness.app" --args "$@"
        ;;
```

4. Add `sweep` and `harness` to the usage line.

- [ ] **Step 5: Run the tests to see them pass**

Run: `mac/scripts/build-mac test JawHarnessTests`
Expected: `Test run with 11 tests` … `passed`, `** TEST SUCCEEDED **`.

- [ ] **Step 6: Sweep, and open the harness**

Run: `mac/scripts/build-mac sweep`
Expected: `swept 2 scenes into 4 pictures in build/review`, then `==> …/mac/build/review`. `mac/build/review/manifest.json` lists `roles` and `mimic`, each with `dark` and `light` pictures whose files are there, each scene with its bands' notes. Look at `mac/build/review/mimic/dark.png`.

Run: `mac/scripts/build-mac harness mimic`
Expected: the harness opens on this Mac at The Mimic, its sidebar listing Foundations with Roles and The Mimic, the conditions under them. Find its PID with `pgrep -f /Users/pedrogomes/src/jaw-harness/mac/build/harness/JawHarness.app` and stop it with `kill <pid>`. If `open` is refused here, say so in the report and leave it.

- [ ] **Step 7: Commit**

```bash
git add mac/JawHarness/Sources/Pictures.swift mac/JawHarness/Sources/HarnessApp.swift mac/scripts/build-mac mac/Tests/JawHarnessTests/PicturesTests.swift
git commit -F- <<'EOF'
Jaw Harness's sweep: --sweep draws every scene in each of its variants into <scene>/<variant>.png beside a manifest of each picture's scene, section, variant, size, hash and notes, drawing the same pictures each time and never emptying a folder it didn't make; build-mac sweep brings them back to mac/build/review, and build-mac harness opens the harness here
EOF
```

---

### Task 4: The made-up account and the other four scenes

**Files:**
- Create: `mac/JawHarness/Sources/Fixtures/Fixtures.swift`
- Create: `mac/JawHarness/Sources/Scenes/ChatListScenes.swift`, `ConversationScenes.swift`, `PaletteScenes.swift`, `StateScenes.swift`
- Create: `mac/Tests/JawHarnessTests/ScenesTests.swift`
- Modify: `mac/JawHarness/Sources/BuiltInScenes.swift`, `mac/Project.swift` (the harness's resources)

**Interfaces:**
- Consumes: `HarnessScene`, `SpecimenGroup`, `Specimen`, `ControllerSpecimen`, `Conditions`, `Pictures.draw`, `HarnessCatalog.shared` (Tasks 1 to 3); from `JawUI`, `Samples` (`group`, `direct`, `chats`, `update`), `ChatListController`, `ConversationController`, `LayoutSpec`, `Layouts`, `PaletteModel`, `PaletteView`, `StateView`, `EmptyChatView`, `StatusLine`; from `JawKit`, `Update`, `Found`, `ChatInfo`.
- Produces:
  - `enum Fixtures { static var found: Found; static let members: [ChatInfo.Member]; static func result<T: Decodable>(_ name: String, as: T.Type = T.self) -> T }`.
  - `extension HarnessScene { static var chatList, conversation, palette, states: HarnessScene }`.
  - `BuiltInScenes.catalog()` registering, in order, `.roles`, `.mimic`, `.chatList`, `.conversation`, `.palette`, `.states`.

- [ ] **Step 1: Put the protocol's fixtures in the harness**

In `mac/Project.swift`, in the `JawHarness` target, change `resources: ["JawApp/Resources/**"],` to:

```swift
            // The app's icon, and the protocol's fixtures, which the scenes decode as jawd sends them.
            resources: ["JawApp/Resources/**", "../protocol/fixtures/*.json"],
```

- [ ] **Step 2: Write the failing tests**

`mac/Tests/JawHarnessTests/ScenesTests.swift`:

```swift
@testable import JawHarness
import JawKit
import JawUI
import Testing

/// The harness's own scenes: each draws as wide as it says, says what to look at, and draws from
/// fixtures that decode.

@MainActor @Test func `the protocol's fixtures are in the harness and decode as jawd sends them`() {
    #expect(Fixtures.found.results.map(\.kind) == [.chat, .person, .folder, .message])
    #expect(Fixtures.members.map(\.name) == ["Ana Ribeiro", "Rui Costa", "Inês Gomes"])
}

@MainActor @Test func `every scene draws as wide as it says, says what to look for, and is named for a script`() async throws {
    #expect(HarnessCatalog.shared.scenes.map(\.id) == ["roles", "mimic", "chat-list", "conversation", "palette", "states"])
    for scene in HarnessCatalog.shared.scenes {
        let drawn = try await Pictures.draw(scene, scene.variants[0].applied(to: Conditions()))
        #expect(drawn.points.width == scene.width + 2 * Space.xxl, "\(scene.id)")
        #expect(drawn.points.height > 2 * Space.xxl, "\(scene.id) draws something")
        #expect(!drawn.notes.isEmpty, "\(scene.id) has a band saying what to look for")
        #expect(!scene.synopsis.isEmpty, "\(scene.id) says what to look at")
        #expect(scene.id.allSatisfy { $0.isLowercase || $0.isNumber || $0 == "-" }, "\(scene.id)")
    }
}
```

- [ ] **Step 3: Run them to see them fail**

Run: `mac/scripts/build-mac test JawHarnessTests`
Expected: the build fails: `cannot find 'Fixtures' in scope`.

- [ ] **Step 4: Write the fixtures**

`mac/JawHarness/Sources/Fixtures/Fixtures.swift`:

```swift
import Foundation
import JawKit

/// The made-up account the scenes draw: `Samples`' chats and messages, in `JawUI`, and the protocol's
/// own fixtures, read from the harness's bundle and decoded as jawd sends them, so a scene shows
/// jawd's shapes.
enum Fixtures {
    /// `find`'s answer in `protocol/fixtures/find.result.json`: a chat, a person, a folder and a message.
    static var found: Found {
        result("find.result")
    }

    /// The members of `Samples.group`, for `@` in its box.
    static let members: [ChatInfo.Member] = decode("""
    [{"person":"person:4","name":"Ana Ribeiro","admin":false},
     {"person":"person:7","name":"Rui Costa","admin":true},
     {"person":"person:9","name":"Inês Gomes","admin":false}]
    """)

    /// The `result` of an answer in `protocol/fixtures/<name>.json`.
    static func result<T: Decodable>(_ name: String, as _: T.Type = T.self) -> T {
        decode(file(name), as: Answer<T>.self).result
    }

    private struct Answer<T: Decodable>: Decodable {
        let result: T
    }

    private static func file(_ name: String) -> String {
        guard let url = Bundle.main.url(forResource: name, withExtension: "json"),
              let text = try? String(contentsOf: url, encoding: .utf8)
        else { fatalError("protocol/fixtures/\(name).json isn't in the harness's bundle") }
        return text
    }

    private static func decode<T: Decodable>(_ text: String, as _: T.Type = T.self) -> T {
        do {
            return try JSONDecoder().decode(T.self, from: Data(text.utf8))
        } catch {
            fatalError("a fixture doesn't decode as \(T.self): \(error)")
        }
    }
}
```

- [ ] **Step 5: Write the four scenes**

`mac/JawHarness/Sources/Scenes/ChatListScenes.swift`:

```swift
import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var chatList: HarnessScene {
        HarnessScene(
            id: "chat-list", title: "The list", symbol: "list.bullet.rectangle",
            synopsis: "A folder's chats as the list draws them, pinned first, the open one selected",
            section: .chatList, covers: [ChatListController.self], width: 640,
        ) { ChatListScene(roles: $0.roles) }
    }
}

/// `Samples`' four chats in the chat list at its usual width, Ana's open.
private struct ChatListScene: View {
    let roles: Roles

    var body: some View {
        SpecimenGroup(
            title: "Four chats at 300 points",
            note: "Ana Ribeiro pinned and open, with your draft \"see you\" in the accent; Lisbon Climbing with 3 unread, a mention and someone typing; Mãe muted, with 1 unread; Climbing Portugal, a community. A name is bold only when it has unread messages, the times line up on the right, and nothing is cut off.",
            roles: roles,
        ) {
            ControllerSpecimen {
                let list = ChatListController(folder: "all", roles: roles)
                _ = list.view
                let rows = Samples.chats
                list.apply(Update(seq: 1, upsert: rows, total: UInt64(rows.count)))
                list.select(Samples.direct.id)
                return list
            }
            .frame(width: 300, height: 260)
        }
    }
}
```

`mac/JawHarness/Sources/Scenes/ConversationScenes.swift`:

```swift
import JawKit
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var conversation: HarnessScene {
        HarnessScene(
            id: "conversation", title: "The conversation", symbol: "bubble.left.and.bubble.right",
            synopsis: "A group's newest messages as the conversation draws them, in either look, comfortable or compact",
            section: .conversation, covers: [ConversationController.self], axes: [.look, .density], width: 680,
        ) { ConversationScene(conditions: $0) }
    }
}

/// Lisbon Climbing's messages from `Samples`, opened at the newest, as a pane 640 points wide shows them.
private struct ConversationScene: View {
    let conditions: Conditions

    var body: some View {
        SpecimenGroup(
            title: "Lisbon Climbing, at the newest",
            note: "Your messages pending, sent, edited and failed with Retry, a photo with reactions, and a card for each kind jawd sends: a video, a document, a place, a poll, a contact, a link's preview, a voice message and an album of five. Runs of one sender sit closer than messages between senders; in lines, yours start a third of the way across; no card overlaps the text around it.",
            roles: conditions.roles,
        ) {
            ControllerSpecimen {
                let c = ConversationController(chat: Samples.group, spec: LayoutSpec(style: conditions.style, me: ["person:1"]),
                                               layouts: Layouts(), place: .newest(100))
                _ = c.view
                c.members = Fixtures.members
                DispatchQueue.main.async { c.apply(Samples.update) }
                return c
            }
            .frame(width: 640, height: 900)
        }
    }
}
```

`mac/JawHarness/Sources/Scenes/PaletteScenes.swift`:

```swift
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var palette: HarnessScene {
        HarnessScene(
            id: "palette", title: "Palette", symbol: "command",
            synopsis: "The palette over find's answer to \"ar\" from the protocol's fixture",
            section: .overlays, covers: [PaletteView.self], width: 700,
        ) { PaletteScene(roles: $0.roles) }
    }
}

/// The palette with "ar" typed and jawd's answer from `protocol/fixtures/find.result.json`.
private struct PaletteScene: View {
    let roles: Roles
    @State private var model: PaletteModel = {
        let model = PaletteModel()
        model.query = "ar"
        model.answered(id: 1, Fixtures.found)
        return model
    }()

    var body: some View {
        SpecimenGroup(
            title: "\"ar\" typed",
            note: "A chat, a person, a folder and a message, under the strip of counts (Chats 1 · People 2 · Folders 1 · Messages 214). The letters that matched are marked in each, each kind has its heading, and the first row is selected.",
            roles: roles,
        ) {
            PaletteView(model: model, roles: roles) {}
        }
    }
}
```

`mac/JawHarness/Sources/Scenes/StateScenes.swift`:

```swift
import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var states: HarnessScene {
        HarnessScene(
            id: "states", title: "The panes' place", symbol: "moon.zzz",
            synopsis: "What fills the panes' place when there's no chat to show, an empty chat, and the status line",
            section: .states, covers: [StateView.self, EmptyChatView.self, StatusLine.self],
        ) { StatesScene(roles: $0.roles) }
    }
}

/// Each state the panes' place can show, an empty chat, and the status line saying jawd's state.
private struct StatesScene: View {
    let roles: Roles

    var kinds: [(String, StateView.Kind)] {
        [
            ("no chat open", .noChat), ("no chats yet", .noChats), ("starting jawd", .starting(failed: nil)),
            ("jawd didn't start", .starting(failed: "jawd stopped as it started (status 1)")),
            ("jawd out of reach", .asleep(why: "jawd isn't running: nothing answers on ~/Library/Application Support/jaw/jaw.sock", retryIn: 2)),
            ("logged out", .loggedOut(reason: "logged out from the phone")), ("not linked", .unlinked),
        ]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "States",
                note: "The Mimic awake, or asleep (grey, eyes shut) when jawd is out of reach; the heading says what's wrong and the line under it what to do, in jawd's words where they're jawd's. Logged out and not linked say Jaw isn't affiliated with WhatsApp or Meta.",
                roles: roles,
            ) {
                LazyVGrid(columns: [GridItem(.flexible(), spacing: Space.l), GridItem(.flexible())], spacing: Space.l) {
                    ForEach(kinds, id: \.0) { caption, kind in
                        Specimen(caption: caption, roles: roles) {
                            StateView(kind: kind, roles: roles)
                                .frame(height: 360)
                                .clipShape(RoundedRectangle(cornerRadius: Radius.field))
                        }
                    }
                }
            }
            SpecimenGroup(
                title: "An empty chat",
                note: "A group with no messages yet: the Mimic sits in it, and the line under it says who what you type goes to.",
                roles: roles,
            ) {
                EmptyChatView(chat: Samples.group)
                    .frame(maxWidth: .infinity)
                    .frame(height: 300)
            }
            SpecimenGroup(
                title: "The status line",
                note: "jawd's state on the left, in the accent, or in the mention colour when it warns, and how the focused chat is beside it, quieter.",
                roles: roles,
            ) {
                VStack(spacing: Space.m) {
                    StatusLine(state: "catching up 1,204 of 5,000", warn: false, chat: "someone's typing…", roles: roles)
                        .frame(height: 30)
                    StatusLine(state: "phone offline: photos and older history wait for it", warn: true, chat: nil, roles: roles)
                        .frame(height: 30)
                }
            }
        }
    }
}
```

In `mac/JawHarness/Sources/BuiltInScenes.swift`, after `catalog.register(.mimic)`, add:

```swift
        catalog.register(.chatList)
        catalog.register(.conversation)
        catalog.register(.palette)
        catalog.register(.states)
```

- [ ] **Step 6: Run the tests to see them pass**

Run: `mac/scripts/build-mac test JawHarnessTests`
Expected: `Test run with 13 tests` … `passed`, `** TEST SUCCEEDED **`.

- [ ] **Step 7: Sweep and look**

Run: `mac/scripts/build-mac sweep`
Expected: `swept 6 scenes into 18 pictures in build/review`. Look at `mac/build/review/chat-list/dark.png`, `conversation/dark-bubbles.png`, `conversation/light-lines-compact.png`, `palette/light.png` and `states/dark.png`, and check each against its band's note: the chat list's four rows with Ana's selected; the conversation's newest messages and their cards, bubbles on the right for yours in bubbles, yours indented in lines; the palette's four results with their letters marked; the seven states, the empty chat and the two status lines. A conversation drawn empty means its rows hadn't been laid out within `Pictures.settle`; give the scene more time rather than raising `settle` for every picture, and say so in the report.

- [ ] **Step 8: Commit**

```bash
git add mac/Project.swift mac/JawHarness mac/Tests/JawHarnessTests/ScenesTests.swift
git commit -F- <<'EOF'
Jaw Harness's other four scenes, ported from the first harness with notes on what to look for: the chat list at 300 points, the conversation at the newest in both looks, comfortable and compact, the palette over find's answer to "ar" from the protocol's fixture, and the panes' place in each state with an empty chat and the status line; the protocol's fixtures are copied into the harness's bundle and decoded as jawd sends them, and a test draws every scene and checks it says what to look for
EOF
```

---

### Task 5: The review canvas and the check

**Files:**
- Create: `mac/scripts/check-harness`, `mac/scripts/review-data`, `.atelier/mac-harness.canvas.tsx`
- Modify: `mac/scripts/check`, `mac/scripts/build-mac` (the sweep writes the canvas's data; `.atelier` not copied), `.gitignore`, `mac/README.md`

**Interfaces:**
- Consumes: `mac/build/review/manifest.json` as Task 3 writes it.
- Produces:
  - `mac/scripts/check-harness`, run by `mac/scripts/check` before every build: fails when `JawHarness/Sources` names `Jawd`.
  - `mac/scripts/review-data`: writes `.atelier/mac-harness.data.ts` (`export type Sweep`, `export const sweep: Sweep`), each picture with `path: "mac/build/review/<file>"`.
  - `.atelier/mac-harness.canvas.tsx`, keeping the owner's marks under `marks` and the filter under `filter`.

- [ ] **Step 1: Write the check and see it catch a `Jawd`**

`mac/scripts/check-harness` (make it executable, `chmod +x`):

```sh
#!/bin/sh
# Fails when the harness names jawd's client: it draws from fixtures and asks jawd for nothing, so
# nothing in it may make a Jawd. Its other rules come with the harness's later phases (the Mac
# harness's design, "The checks").
cd "$(dirname "$0")/.."
found=$(grep -rnw 'Jawd' JawHarness/Sources)
if [ -n "$found" ]; then
    echo "$found"
    echo "check-harness: the harness draws from fixtures and never names Jawd" >&2
    exit 1
fi
```

Run: `printf '// Jawd\n' > mac/JawHarness/Sources/Probe.swift; mac/scripts/check-harness; echo "status $?"; rm mac/JawHarness/Sources/Probe.swift; mac/scripts/check-harness; echo "status $?"`
Expected: the probe's line, `check-harness: the harness draws from fixtures and never names Jawd`, `status 1`; then `status 0`.

In `mac/scripts/check`, after `scripts/check-components || exit 1` add:

```sh
scripts/check-harness || exit 1
```

- [ ] **Step 2: Write the canvas's data**

`mac/scripts/review-data` (make it executable):

```python
#!/usr/bin/env python3
"""Writes the review canvas's data, .atelier/mac-harness.data.ts, from the last sweep's manifest,
mac/build/review/manifest.json, so the canvas shows that sweep's pictures. Git ignores the data."""

import json
import pathlib
import sys

root = pathlib.Path(__file__).resolve().parents[2]
manifest = root / "mac" / "build" / "review" / "manifest.json"
out = root / ".atelier" / "mac-harness.data.ts"

if not manifest.exists():
    sys.exit(f"review-data: there's no sweep yet ({manifest.relative_to(root)} is missing): run mac/scripts/build-mac sweep")

sweep = json.loads(manifest.read_text(encoding="utf-8"))
for scene in sweep["scenes"]:
    for picture in scene["pictures"]:
        picture["path"] = f"mac/build/review/{picture['file']}"

out.parent.mkdir(exist_ok=True)
out.write_text(
    "// Written by mac/scripts/review-data from the last sweep's manifest; git ignores it.\n"
    "export type Sweep = {\n"
    "  made: string;\n"
    "  scenes: {\n"
    "    id: string;\n"
    "    section: string;\n"
    "    title: string;\n"
    "    synopsis: string;\n"
    "    notes: { title: string; note: string }[];\n"
    "    pictures: { variant: string; file: string; path: string; points: number[]; pixels: number[]; hash: string }[];\n"
    "  }[];\n"
    "};\n"
    f"export const sweep: Sweep = {json.dumps(sweep, ensure_ascii=False, indent=2)};\n",
    encoding="utf-8",
)
print(out.relative_to(root))
```

In `mac/scripts/build-mac`:

1. In the main `case`'s `sweep)` job, after the `rsync` line and before its `echo`, add:

```sh
        "$here/mac/scripts/review-data"
```

2. In the fallback `case`'s `sweep)` job, before its `exit`, add the same line.

3. In the `rsync` that copies the checkout to the build Mac, after `--exclude /.work`, add `--exclude /.atelier`.

In `.gitignore` (the repository's own, at its root), add:

```gitignore
.atelier/*.state.json
.atelier/*.events.jsonl
.atelier/mac-harness.data.ts
```

- [ ] **Step 3: Write the canvas**

`.atelier/mac-harness.canvas.tsx`:

```tsx
import { Badge, Button, Gallery, Panel, Radio, Row, Stack, Stat, Stats, Tabs, Text, TextArea, useCanvasState } from "atelier";
import { sweep, type Sweep } from "./mac-harness.data";

export const meta = { title: "Jaw for the Mac: the harness" };

type Scene = Sweep["scenes"][number];
/** A mark on a scene, with the hashes of the pictures it was made on. */
type Mark = { state: "right" | "needs-work"; note: string; hashes: string[]; at: string };
type Standing = "right" | "needs-work" | "changed" | "unseen";

const words: Record<Standing, string> = {
  right: "Right",
  "needs-work": "Needs work",
  changed: "Changed since your mark",
  unseen: "Not looked at",
};
const tones = { right: "success", "needs-work": "danger", changed: "warning", unseen: "muted" } as const;
const filters: { value: string; label: string }[] = [
  { value: "all", label: "Every scene" },
  { value: "needs-work", label: "Needs work" },
  { value: "changed", label: "Changed since marked" },
  { value: "unseen", label: "Not looked at" },
  { value: "right", label: "Right" },
];

const hashesOf = (scene: Scene) => scene.pictures.map((p) => p.hash);

/** Where a scene stands: unmarked, marked on the pictures it has now, or changed since its mark. */
function standing(scene: Scene, mark: Mark | null | undefined): Standing {
  if (!mark) return "unseen";
  const now = hashesOf(scene);
  const same = mark.hashes.length === now.length && mark.hashes.every((h, i) => h === now[i]);
  return same ? mark.state : "changed";
}

function ScenePanel({ scene, mark, onMark }: { scene: Scene; mark: Mark | null | undefined; onMark: (mark: Mark | null) => void }) {
  const now = standing(scene, mark);
  const keep = (state: Mark["state"], note: string) =>
    onMark({ state, note, hashes: hashesOf(scene), at: new Date().toISOString() });
  const picked = (state: Mark["state"]) => (mark?.state === state && now !== "changed" ? "solid" : "outline");
  return (
    <Panel title={scene.title} info={<Badge tone={tones[now]}>{words[now]}</Badge>}>
      <Stack gap={10}>
        <Text tone="secondary">{scene.synopsis}</Text>
        {scene.notes.map((n) => (
          <Text key={n.title} size="sm">
            {n.title}: {n.note}
          </Text>
        ))}
        <Gallery
          images={scene.pictures.map((p) => ({ path: p.path, alt: `${scene.title}, ${p.variant}`, caption: p.variant }))}
          columns={Math.min(Math.max(scene.pictures.length, 1), 4)}
        />
        <Row gap={8} align="center">
          <Button tone="success" variant={picked("right")} onClick={() => keep("right", mark?.note ?? "")}>
            Right
          </Button>
          <Button tone="danger" variant={picked("needs-work")} onClick={() => keep("needs-work", mark?.note ?? "")}>
            Needs work
          </Button>
          {mark ? (
            <Button variant="ghost" onClick={() => onMark(null)}>
              Clear
            </Button>
          ) : null}
        </Row>
        <TextArea
          label="What needs work"
          placeholder="What to change, and where"
          rows={2}
          value={mark?.note ?? ""}
          onChange={(note) => keep(mark?.state ?? "needs-work", note)}
        />
      </Stack>
    </Panel>
  );
}

export default function HarnessReview() {
  const [marks, setMarks] = useCanvasState<Record<string, Mark | null>>("marks", {});
  const [filter, setFilter] = useCanvasState<string>("filter", "all");
  const count = (s: Standing) => sweep.scenes.filter((scene) => standing(scene, marks[scene.id]) === s).length;
  const sections = [...new Set(sweep.scenes.map((s) => s.section))];
  const shown = (scene: Scene) => filter === "all" || standing(scene, marks[scene.id]) === filter;
  const mark = (id: string) => (m: Mark | null) => setMarks((previous) => ({ ...previous, [id]: m }));
  return (
    <Stack gap={16}>
      <Stats>
        <Stat label="Scenes" value={sweep.scenes.length} hint={`swept ${sweep.made}`} />
        <Stat label="Right" value={count("right")} tone="success" />
        <Stat label="Needs work" value={count("needs-work")} tone="danger" />
        <Stat label="Changed since marked" value={count("changed")} tone="warning" />
        <Stat label="Not looked at" value={count("unseen")} tone="muted" />
      </Stats>
      <Radio value={filter} onChange={setFilter} options={filters} />
      <Tabs
        stateKey="section"
        tabs={sections.map((section) => {
          const scenes = sweep.scenes.filter((s) => s.section === section);
          const open = scenes.filter((s) => standing(s, marks[s.id]) !== "right").length;
          return {
            id: section,
            label: section,
            badge: open > 0 ? open : undefined,
            content: (
              <Stack gap={16}>
                {scenes.filter(shown).map((scene) => (
                  <ScenePanel key={scene.id} scene={scene} mark={marks[scene.id]} onMark={mark(scene.id)} />
                ))}
                {scenes.filter(shown).length === 0 ? <Text tone="muted">Nothing in this section under this filter.</Text> : null}
              </Stack>
            ),
          };
        })}
      />
    </Stack>
  );
}
```

- [ ] **Step 4: Sweep, and check the canvas**

Run: `mac/scripts/build-mac sweep`
Expected: `swept 6 scenes into 18 pictures in build/review`, `.atelier/mac-harness.data.ts`, then `==> …/mac/build/review`.

Run: `atelier check .atelier/mac-harness.canvas.tsx`
Expected: no problems. Fix any it reports in the canvas, and run it again.

Run: `git status --short`
Expected: `.atelier/mac-harness.canvas.tsx` and the changed scripts and docs, but no `.atelier/mac-harness.data.ts`, no state files, nothing under `mac/build/`.

- [ ] **Step 5: Say how in the README**

In `mac/README.md`:

1. In the "Building" block, after the line `mac/scripts/build-mac test JawUITests       # the layout, and its reference pictures in both looks`, add:

```sh
mac/scripts/build-mac test JawHarnessTests  # the harness: its catalog, flags, pictures and scenes
```

2. Replace the sentence that starts "The harness (`JawHarness`) draws each component from made-up rows" with nothing, and add after the "Building" section a section:

````markdown
## The harness

Jaw Harness (`JawHarness`) draws the app's components and screens from made-up rows and the protocol's fixtures, in each theme, look and density, and asks jawd for nothing, as [the harness's design](../docs/plans/2026-10-27-jaw-mac-harness-design.md) has it. Its scenes are listed by section on the left, with the conditions they're drawn in under them; each says what to look at, and each band of specimens what would be wrong with it.

```sh
mac/scripts/build-mac harness [scene] [options]   # built on the build Mac and opened here
mac/scripts/build-mac shot <scene> <png> [options] # one scene drawn into a picture
mac/scripts/build-mac sweep                        # every scene in each variant, into mac/build/review
```

After a sweep, the review canvas, `.atelier/mac-harness.canvas.tsx`, shows each scene's pictures by section (`atelier open .atelier/mac-harness.canvas.tsx`). Mark each one Right, or Needs work with a note; the marks are kept under `marks`, by scene, with the pictures they were made on, and agents read them with `atelier state .atelier/mac-harness.canvas.tsx`. A scene whose pictures change after its mark asks again.

Its options: `--scene <id>`, `--theme <id>`, `--appearance dark|light`, `--look bubbles|lines`, `--density comfortable|compact`, `--text-size <11…17>`, `--font system|mono`, `--reduce-motion`, `--window <width>x<height>`, `--stage-only`, `--snapshot <png>` and `--sweep <folder>`.

To add a component, write a scene in `JawHarness/Sources/Scenes/` and register it in `BuiltInScenes.swift`. `scripts/check-harness`, which every build runs, fails if the harness names `Jawd`.
````

- [ ] **Step 6: Run the harness's tests once more**

Run: `mac/scripts/build-mac test JawHarnessTests`
Expected: `Test run with 13 tests` … `passed`, `** TEST SUCCEEDED **`, with `check-harness` run before the build.

- [ ] **Step 7: Commit**

```bash
git add mac/scripts/check-harness mac/scripts/review-data mac/scripts/check mac/scripts/build-mac .gitignore mac/README.md .atelier/mac-harness.canvas.tsx
git commit -F- <<'EOF'
The review canvas for Jaw Harness: the last sweep's pictures by section, each scene with its synopsis and its bands' notes, marked Right or Needs work with a note and kept under marks with the hashes of the pictures they were made on, so a scene whose pictures change asks again; review-data writes its data from the sweep's manifest, which git ignores with Atelier's state; check-harness fails every build in which the harness names Jawd; and the Mac README says how to open, shoot, sweep and review the harness
EOF
```
