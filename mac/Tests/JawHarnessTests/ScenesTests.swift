import CoreGraphics
@testable import JawHarness
import JawKit
import JawUI
import SwiftUI
import Testing

/// The harness's own scenes: each draws as wide as it says, says what to look at, draws what it's
/// there to show and the same picture each time, and draws from fixtures that decode.

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

/// A chat list with no chats in the chat list scene's band, `band`, at the frame that scene gives its
/// list and at its width: the scene as it would be drawn if its chats never arrived.
func emptyChatList(_ band: Note, width: CGFloat) -> HarnessScene {
    HarnessScene(id: "no-chats", title: "No chats", symbol: "list.bullet.rectangle", synopsis: "", section: .chatList,
                 covers: [], width: width) { c in
        SpecimenGroup(title: band.title, note: band.note, roles: c.roles) {
            ControllerSpecimen {
                let list = ChatListController(folder: "all", roles: c.roles)
                _ = list.view
                return list
            }
            .frame(width: 300, height: 260)
        }
    }
}

/// The scenes drawn as a sweep draws them, one test at a time: a draw holds the app-wide
/// `Motion.reduced` for as long as it takes, so two mustn't overlap.
@MainActor
@Suite(.serialized) struct SceneDrawingTests {
    @Test func `every scene draws as wide as it says, says what to look for, and is named for a script`() async throws {
        #expect(HarnessCatalog.shared.scenes.map(\.id) == [
            "roles", "mimic", "chat-list", "conversation", "palette", "states",
            "screens-jawd", "screens-connection", "screens-linking", "screens-empty",
            "screens-chat", "screens-panes", "screens-focus", "screens-folded", "screens-overlays",
        ])
        for scene in HarnessCatalog.shared.scenes {
            let drawn = try await Pictures.draw(scene, scene.variants[0].applied(to: Conditions()))
            #expect(drawn.points.width == scene.width + 2 * Space.xxl, "\(scene.id)")
            #expect(!drawn.notes.isEmpty, "\(scene.id) has a band saying what to look for")
            #expect(!scene.synopsis.isEmpty, "\(scene.id) says what to look at")
            #expect(scene.id.allSatisfy { $0.isLowercase || $0.isNumber || $0 == "-" }, "\(scene.id)")
        }
    }

    @Test func `every scene draws the same picture twice, so a mark on the canvas holds until the scene changes`() async throws {
        for scene in HarnessCatalog.shared.scenes {
            let conditions = scene.variants[0].applied(to: Conditions())
            let first = try await Pictures.draw(scene, conditions)
            let again = try await Pictures.draw(scene, conditions)
            #expect(again.hash == first.hash, "\(scene.id) draws the same picture each time")
        }
    }

    @Test func `the conversation draws its messages, so its bubbles and its lines differ, as an empty conversation's don't`() async throws {
        let scene = try #require(HarnessCatalog.shared.scene(id: "conversation"))
        let bubbles = try #require(scene.variants.first { $0.name == "dark-bubbles" })
        let lines = try #require(scene.variants.first { $0.name == "dark-lines" })
        let inBubbles = try await Pictures.draw(scene, bubbles.applied(to: Conditions()))
        let inLines = try await Pictures.draw(scene, lines.applied(to: Conditions()))
        #expect(inBubbles.hash != inLines.hash)
    }

    @Test func `the chat list draws its chats, so it differs from an empty chat list drawn the same way`() async throws {
        let scene = try #require(HarnessCatalog.shared.scene(id: "chat-list"))
        let dark = try #require(scene.variants.first { $0.name == "dark" }).applied(to: Conditions())
        let chats = try await Pictures.draw(scene, dark)
        let band = try #require(chats.notes.first)
        let none = try await Pictures.draw(emptyChatList(band, width: scene.width), dark)
        #expect(none.points == chats.points, "drawn the same way")
        #expect(none.hash != chats.hash)
    }
}
