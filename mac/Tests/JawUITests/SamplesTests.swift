@testable import JawUI
import Testing

/// The made-up rows the harness and the snapshot tests draw, in the order a window holds them.

@Test func `the samples' messages sort in the order of their times, as jawd's sorts follow a message's time`() {
    let bySort = Samples.messages.sorted { $0.sort.utf8.lexicographicallyPrecedes($1.sort.utf8) }
    let byTime = Samples.messages.sorted { $0.at < $1.at }
    #expect(bySort.map(\.id) == byTime.map(\.id))
}
