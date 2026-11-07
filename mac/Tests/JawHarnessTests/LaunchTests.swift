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
        "JawHarness", "--appearance", "sepia", "--look", "tiles", "--window", "big", "--text-size", "huge", "--scene", "--theme",
    ])
    #expect(launch == HarnessLaunch.parse(["JawHarness"]))
}

@MainActor @Test func `a value that isn't one is refused rather than half read`() {
    let defaults = HarnessLaunch.parse(["JawHarness"])
    #expect(HarnessLaunch.parse(["JawHarness", "--text-size", "nan"]) == defaults)
    #expect(HarnessLaunch.parse(["JawHarness", "--text-size", "14.6"]).conditions.textSize == 15, "whole points, as Settings' slider steps")
    #expect(HarnessLaunch.parse(["JawHarness", "--text-size", "3"]).conditions.textSize == 11)
    for window in ["1200xfoox800", "0x0", "-5x-5", "nanxnan", "1200x", "x800"] {
        #expect(HarnessLaunch.parse(["JawHarness", "--window", window]).window == nil, "\(window)")
    }
    #expect(HarnessLaunch.parse(["JawHarness", "--snapshot", "--sweep", "/x"]).snapshot == nil, "a flag isn't a value")
    #expect(HarnessLaunch.parse(["JawHarness", "--density", "roomy", "--font", "comic"]) == defaults)
}

@MainActor @Test func `--theme takes any theme's ID, Jaw's, Omarchy's or Redlamp's, and stops the harness at one that names none, saying which it takes`() {
    let gruvbox = HarnessLaunch.parse(["JawHarness", "--theme", "omarchy:gruvbox"])
    #expect(gruvbox.conditions.theme == "omarchy:gruvbox" && gruvbox.problem == nil)
    #expect(HarnessLaunch.parse(["JawHarness", "--theme", "redlamp:nord"]).conditions.theme == "redlamp:nord")
    let slip = HarnessLaunch.parse(["JawHarness", "--theme", "nord"])
    #expect(slip.conditions.theme == "jaw", "an ID names its source")
    #expect(slip.problem?.hasPrefix("there's no theme called nord; --theme takes jaw, omarchy:catppuccin, omarchy:catppuccin-latte,") == true
        && slip.problem?.hasSuffix(", redlamp:mc-velvet-static") == true, "and one that names none is said, with all 46 it takes")
}
