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
