import CoreGraphics
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

@MainActor @Test func `a fixture that isn't in the bundle, or doesn't decode as what's asked, says which file it was reading`() {
    let missing = #expect(throws: Fixtures.FixtureError.self) { try Fixtures.read("nope", as: Found.self) }
    #expect(missing?.description == "protocol/fixtures/nope.json isn't in the harness's bundle")
    let wrong = #expect(throws: Fixtures.FixtureError.self) { try Fixtures.read("find.result", as: ChatInfo.self) }
    #expect(wrong?.description.hasPrefix("protocol/fixtures/find.result.json doesn't decode as ChatInfo: ") == true)
}

@MainActor @Test func `the harness draws as of Samples' now, so a picture taken today compares with last week's`() {
    #expect(Format.now() == Samples.now)
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
