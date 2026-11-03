@testable import JawUI
import Testing

/// Chats started with `chat.start` open where their own start said, in the order jawd answers them,
/// however many wait at once: the roster's Open in New Pane, the palette's starts by person or by
/// number, and a contact card's Message, each answered with its chat or why not.

@Test func `a chat the palette starts opens in place, and one the roster starts in a new pane meanwhile opens there`() {
    var starts = ChatStarts()
    starts.sent(inNewPane: false)
    starts.sent(inNewPane: true)
    let palette = starts.answered(), roster = starts.answered()
    #expect(!palette, "the palette's, by a number WhatsApp is still checking, opens in place")
    #expect(roster, "and the roster's, answered after it, in the new pane")
}

@Test func `two starts from the roster each open where they said, a plain Open after Open in New Pane among them`() {
    var starts = ChatStarts()
    starts.sent(inNewPane: true)
    starts.sent(inNewPane: false)
    let first = starts.answered(), second = starts.answered()
    #expect(first, "the first opens in the new pane, which the second start doesn't take back")
    #expect(!second, "the second in place")
}

@Test func `a start jawd turns down takes only its own place, and a contact card's Message waiting opens in place`() {
    var starts = ChatStarts()
    starts.sent(inNewPane: false)
    starts.sent(inNewPane: true)
    starts.sent(inNewPane: false)
    let card = starts.answered(), roster = starts.answered(), palette = starts.answered(), none = starts.answered()
    #expect(!card, "the card's Message, turned down by jawd, was in place")
    #expect(roster, "so the roster's start after it still opens in the new pane")
    #expect(!palette, "and the palette's in place")
    #expect(!none, "an answer with no start waiting opens its chat in place")
    #expect(starts == ChatStarts())
}
