import Foundation
import JawKit
import JawUI

/// The made-up account the scenes draw: `Samples`' chats and messages, in `JawUI`, and the protocol's
/// own fixtures, read from the harness's bundle and decoded as jawd sends them, so a scene shows
/// jawd's shapes.
enum Fixtures {
    /// `find`'s answer in `protocol/fixtures/find.result.json`: a chat, a person, a folder and a message.
    static let found: Found = result("find.result")

    /// `folders.watch`'s answer in `protocol/fixtures/folders.watch.result.json`: the built-in folders
    /// All and Unread, Family with Cousins in it, Mentions me, and three slots.
    static let tree: FoldersWatched = result("folders.watch.result")

    /// The members of `Samples.group`, for `@` in its box.
    static let members: [ChatInfo.Member] = decode("""
    [{"person":"person:4","name":"Ana Ribeiro","admin":false},
     {"person":"person:7","name":"Rui Costa","admin":true},
     {"person":"person:9","name":"Inês Gomes","admin":false}]
    """, from: "Fixtures.members")

    /// The palette with "ar" typed and `found` as jawd's answer, as the palette scene and the palette
    /// over a screen draw it.
    @MainActor static func palette() -> PaletteModel {
        let model = PaletteModel()
        model.query = "ar"
        model.answered(id: 1, found)
        return model
    }

    /// `find`'s answer to "sintra": a chat and two links, each a link's row as the protocol's Finding
    /// has it, since the protocol's fixture finds no link.
    static let foundLinks: Found = decode("""
    {"results":[
     {"kind":"chat","score":40,"text":[{"t":"mark","s":"Sintra"},{"t":"text","s":" crew"}],"field":"name",
      "row":{"id":"120363000000000003@g.us","v":1,"sort":"0~1792140000000~5","kind":"group","name":"Sintra crew","unread":0}},
     {"kind":"link","score":0,"text":[{"t":"mark","s":"Sintra"},{"t":"text","s":": the routes · climb.pt"}],
      "row":{"id":"3EB0A1B2C3D4E5F6~0","kind":"link","message":"3EB0A1B2C3D4E5F6","chat":"120363012345678901@g.us","chat_name":"Lisbon Climbing",
             "from":{"id":"person:7","name":"Rui","colour":2},"mine":false,"at":\(Samples.now - 3_600_000),
             "link":{"url":"https://climb.pt/sintra","host":"climb.pt","title":"Sintra: the routes","description":"Granite slabs above the sea"}}},
     {"kind":"link","score":0,"text":[{"t":"text","s":"Previsão para "},{"t":"mark","s":"Sintra"},{"t":"text","s":" · IPMA · www.ipma.pt"}],
      "row":{"id":"3EB0C431F2A9B8E7D6~0","kind":"link","message":"3EB0C431F2A9B8E7D6","chat":"351912345678@s.whatsapp.net",
             "chat_name":"Ana Ribeiro","mine":true,"at":\(Samples.now - 86_400_000),
             "link":{"url":"https://www.ipma.pt/pt/otempo/","host":"www.ipma.pt","title":"Previsão para Sintra · IPMA"}}}
    ],"counts":{"chat":1,"link":2}}
    """, from: "Fixtures.foundLinks")

    /// The palette with "sintra" typed and `foundLinks` as jawd's answer.
    @MainActor static func paletteLinks() -> PaletteModel {
        let model = PaletteModel()
        model.query = "sintra"
        model.answered(id: 1, foundLinks)
        return model
    }

    /// The `result` of an answer in `protocol/fixtures/<name>.json`; a fixture that can't be read stops
    /// the harness, saying which file and why.
    static func result<T: Decodable>(_ name: String, as _: T.Type = T.self) -> T {
        do {
            return try read(name)
        } catch {
            fatalError("\(error)")
        }
    }

    /// The `result` of an answer in `protocol/fixtures/<name>.json`, or an error naming the file.
    static func read<T: Decodable>(_ name: String, as _: T.Type = T.self) throws -> T {
        let file = "protocol/fixtures/\(name).json"
        guard let url = Bundle.main.url(forResource: name, withExtension: "json"),
              let text = try? String(contentsOf: url, encoding: .utf8)
        else { throw FixtureError.missing(file) }
        do {
            return try JSONDecoder().decode(Answer<T>.self, from: Data(text.utf8)).result
        } catch {
            throw FixtureError.undecodable(file, as: "\(T.self)", error)
        }
    }

    /// Why a fixture can't be had, naming where it was read from.
    enum FixtureError: Error, CustomStringConvertible {
        case missing(String)
        case undecodable(String, as: String, any Error)

        var description: String {
            switch self {
            case let .missing(file): "\(file) isn't in the harness's bundle"
            case let .undecodable(source, type, error): "\(source) doesn't decode as \(type): \(error)"
            }
        }
    }

    private struct Answer<T: Decodable>: Decodable {
        let result: T
    }

    private static func decode<T: Decodable>(_ text: String, as _: T.Type = T.self, from source: String) -> T {
        do {
            return try JSONDecoder().decode(T.self, from: Data(text.utf8))
        } catch {
            fatalError("\(FixtureError.undecodable(source, as: "\(T.self)", error))")
        }
    }
}
