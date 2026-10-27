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
