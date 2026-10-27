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

@MainActor @Test func `a sweep leaves alone a folder whose manifest isn't a sweep's`() async throws {
    let root = scratch("foreign")
    defer { try? FileManager.default.removeItem(at: root) }
    let folder = root.appending(path: "review")
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    try Data(#"{"name": "a web app"}"#.utf8).write(to: folder.appending(path: "manifest.json"))
    try Data("<html>".utf8).write(to: folder.appending(path: "index.html"))
    var catalog = HarnessCatalog()
    catalog.register(block())
    await #expect(throws: Pictures.PictureError.self) {
        try await Pictures.sweep(catalog, conditions: Conditions(), to: folder)
    }
    let manifest = try String(contentsOf: folder.appending(path: "manifest.json"), encoding: .utf8)
    #expect(manifest == #"{"name": "a web app"}"#)
    #expect(FileManager.default.fileExists(atPath: folder.appending(path: "index.html").path))
    let beside = try FileManager.default.contentsOfDirectory(atPath: root.path)
    #expect(beside == ["review"], "nothing is made beside it either")
}

@MainActor @Test func `a sweep leaves alone a folder it made once something else is put in it`() async throws {
    let root = scratch("added")
    defer { try? FileManager.default.removeItem(at: root) }
    let folder = root.appending(path: "review")
    var catalog = HarnessCatalog()
    catalog.register(block())
    let first = try await Pictures.sweep(catalog, conditions: Conditions(), to: folder)
    try Data("mine".utf8).write(to: folder.appending(path: "block/notes.txt"))
    await #expect(throws: Pictures.PictureError.self) {
        try await Pictures.sweep(catalog, conditions: Conditions(), to: folder)
    }
    #expect(FileManager.default.fileExists(atPath: folder.appending(path: "block/notes.txt").path))
    let kept = try? JSONDecoder().decode(Pictures.Manifest.self, from: Data(contentsOf: folder.appending(path: "manifest.json")))
    #expect(kept == first, "the sweep it made is as it was")
}

@MainActor @Test func `a sweep clears what a stopped sweep left beside the folder, and leaves nothing there itself`() async throws {
    let root = scratch("left")
    defer { try? FileManager.default.removeItem(at: root) }
    let folder = root.appending(path: "review")
    var catalog = HarnessCatalog()
    catalog.register(block())
    try await Pictures.sweep(catalog, conditions: Conditions(), to: folder)
    let left = root.appending(path: ".review.sweeping")
    try FileManager.default.createDirectory(at: left.appending(path: "block"), withIntermediateDirectories: true)
    try Data("stray".utf8).write(to: left.appending(path: "block/stray.txt"))
    var next = HarnessCatalog()
    next.register(block("looks", axes: .look))
    let manifest = try await Pictures.sweep(next, conditions: Conditions(), to: folder)
    let beside = try FileManager.default.contentsOfDirectory(atPath: root.path)
    #expect(beside == ["review"], "the stopped sweep's folder is gone, and the sweep left none of its own")
    let written = try JSONDecoder().decode(Pictures.Manifest.self, from: Data(contentsOf: folder.appending(path: "manifest.json")))
    #expect(written == manifest && written.scenes.map(\.id) == ["looks"])
    let inside = try FileManager.default.contentsOfDirectory(atPath: folder.path)
    #expect(Set(inside) == ["looks", "manifest.json"])
    let looks = try FileManager.default.contentsOfDirectory(atPath: folder.appending(path: "looks").path)
    #expect(Set(looks) == ["dark-bubbles.png", "dark-lines.png", "light-bubbles.png", "light-lines.png"])
}

@MainActor @Test func `a sweep that's stopped leaves the last whole one where it was, and one that finishes leaves nothing beside it`() async throws {
    let root = scratch("stopped")
    defer { try? FileManager.default.removeItem(at: root) }
    let folder = root.appending(path: "review")
    var catalog = HarnessCatalog()
    catalog.register(block())
    let first = try await Pictures.sweep(catalog, conditions: Conditions(), to: folder)
    let beside = try FileManager.default.contentsOfDirectory(atPath: root.path)
    #expect(beside == ["review"], "a sweep that finishes leaves nothing beside the folder")
    let stopped = Task { try await Pictures.sweep(catalog, conditions: Conditions(), to: folder) }
    stopped.cancel()
    await #expect(throws: CancellationError.self) { try await stopped.value }
    let kept = try? JSONDecoder().decode(Pictures.Manifest.self, from: Data(contentsOf: folder.appending(path: "manifest.json")))
    #expect(kept == first, "the last whole sweep is where it was")
    for picture in first.scenes.flatMap(\.pictures) {
        #expect(FileManager.default.fileExists(atPath: folder.appending(path: picture.file).path), "\(picture.file)")
    }
    let next = try await Pictures.sweep(catalog, conditions: Conditions(), to: folder)
    #expect(next.scenes == first.scenes)
    let after = try FileManager.default.contentsOfDirectory(atPath: root.path)
    #expect(after == ["review"], "nor does one after a sweep that was stopped")
}
