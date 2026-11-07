import Foundation
@testable import JawKit
import Testing

/// Logging out asks jawd once, with nothing to say, and a refusal is said in jawd's own words.
@MainActor
@Suite(.serialized) struct LogoutTests {
    @Test func `logging out asks jawd for link.logout, and its refusal is said in its words`() async throws {
        let why = "can't log out: error sending logout request: websocket not connected"
        let fake = try FakeJawd { method, params, id in
            method == "link.logout" ? [FakeJawd.refusal(id, code: ErrorCode.engineUnavailable, why)] : ordinary(method, params, id)
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        jawd.linkLogout()
        let said = await drain(jawd) { !notices($0).isEmpty }
        #expect(requests(fake, "link.logout").count == 1)
        #expect(notices(said) == [why])
    }
}
