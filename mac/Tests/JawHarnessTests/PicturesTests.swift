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
