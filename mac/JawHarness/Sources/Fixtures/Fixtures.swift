import Foundation
import JawKit

/// The made-up account the scenes draw: `Samples`' chats and messages, in `JawUI`, and the protocol's
/// own fixtures, read from the harness's bundle and decoded as jawd sends them, so a scene shows
/// jawd's shapes.
enum Fixtures {
    /// `find`'s answer in `protocol/fixtures/find.result.json`: a chat, a person, a folder and a message.
    static var found: Found {
        result("find.result")
    }

    /// The members of `Samples.group`, for `@` in its box.
    static let members: [ChatInfo.Member] = decode("""
    [{"person":"person:4","name":"Ana Ribeiro","admin":false},
     {"person":"person:7","name":"Rui Costa","admin":true},
     {"person":"person:9","name":"Inês Gomes","admin":false}]
    """)

    /// The `result` of an answer in `protocol/fixtures/<name>.json`.
    static func result<T: Decodable>(_ name: String, as _: T.Type = T.self) -> T {
        decode(file(name), as: Answer<T>.self).result
    }

    private struct Answer<T: Decodable>: Decodable {
        let result: T
    }

    private static func file(_ name: String) -> String {
        guard let url = Bundle.main.url(forResource: name, withExtension: "json"),
              let text = try? String(contentsOf: url, encoding: .utf8)
        else { fatalError("protocol/fixtures/\(name).json isn't in the harness's bundle") }
        return text
    }

    private static func decode<T: Decodable>(_ text: String, as _: T.Type = T.self) -> T {
        do {
            return try JSONDecoder().decode(T.self, from: Data(text.utf8))
        } catch {
            fatalError("a fixture doesn't decode as \(T.self): \(error)")
        }
    }
}
