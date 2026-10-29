import JawKit
@testable import JawUI
import Testing

/// The made-up rows the harness and the snapshot tests draw, in the order a window holds them.

@Test func `the samples' messages sort in the order of their times, as jawd's sorts follow a message's time`() {
    let bySort = Samples.messages.sorted { $0.sort.utf8.lexicographicallyPrecedes($1.sort.utf8) }
    let byTime = Samples.messages.sorted { $0.at < $1.at }
    #expect(bySort.map(\.id) == byTime.map(\.id))
}

@Test func `a sample chat or folder on a favourite slot carries it, as jawd's rows carry the slots the tree has them on`() {
    for chat in Samples.chats {
        #expect(chat.favourite == Samples.tree.slot(of: chat.id), "\(chat.name)")
    }
    for folder in Samples.folders {
        #expect(folder.favourite == Samples.tree.slot(of: folder.id), "\(folder.name)")
    }
}
