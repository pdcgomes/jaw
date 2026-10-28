import JawKit
import SwiftUI

/// jawd's status as it answered `status`, in full: what the Status window shows.
public struct StatusView: View {
    let status: Status

    public init(status: Status) {
        self.status = status
    }

    public var body: some View {
        ScrollView { Text(Self.text(status)).textStyle(.mono).textSelection(.enabled).padding() }
            .frame(minWidth: 520, minHeight: 420)
    }

    /// The answer as jawd sent it, pretty-printed with its keys in order.
    static func text(_ status: Status) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        return (try? String(data: encoder.encode(status.raw), encoding: .utf8)) ?? ""
    }
}
