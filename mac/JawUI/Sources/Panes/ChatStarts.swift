/// The chats started with `chat.start` that jawd hasn't answered yet, oldest first, each with
/// whether its chat opens in a pane of its own. jawd answers starts in the order they were sent,
/// each with its chat or why not, so every start goes in here as it's sent and each answer takes
/// the oldest out: a chat the roster starts in a new pane opens there, and one started from the
/// palette or a contact card meanwhile opens in place, however many wait on jawd at once.
public struct ChatStarts: Equatable, Sendable {
    var waiting: [Bool] = []

    public init() {}

    /// A start sent, its chat to open in a new pane or in place.
    public mutating func sent(inNewPane: Bool) {
        waiting.append(inNewPane)
    }

    /// jawd's answer to the oldest start waiting, its chat or why not: whether that chat opens in a
    /// pane of its own. An answer with no start waiting opens its chat in place.
    @discardableResult
    public mutating func answered() -> Bool {
        waiting.isEmpty ? false : waiting.removeFirst()
    }
}
