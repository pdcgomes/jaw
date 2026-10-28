import Foundation
import JawKit
import JawUI

/// The made-up account the scenes draw: `Samples`' chats and messages, in `JawUI`, and the protocol's
/// own fixtures, read from the harness's bundle and decoded as jawd sends them, so a scene shows
/// jawd's shapes.
enum Fixtures {
    /// `find`'s answer in `protocol/fixtures/find.result.json`: a chat, a person, a folder and a message.
    static let found: Found = result("find.result")

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
