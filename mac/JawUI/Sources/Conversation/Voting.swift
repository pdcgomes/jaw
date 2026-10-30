import JawKit

/// What a click on a poll's option votes for, as WhatsApp's apps have it: in a poll of one choice,
/// that option alone, or none when it's your choice already, which takes your vote back; in a poll of
/// several, your choices with that one added or taken out, and never more than the poll takes, which
/// jawd would refuse.
public enum Voting {
    public enum Choice: Equatable, Sendable {
        /// Every option chosen now, by name, in the poll's order; none takes the vote back.
        case vote([String])
        /// One more than the poll takes, which it takes this many of at most.
        case tooMany(Int)
    }

    public static func choosing(_ name: String, in poll: Poll) -> Choice {
        let mine = poll.mine
        if poll.max <= 1 { return .vote(mine == [name] ? [] : [name]) }
        if mine.contains(name) { return .vote(mine.filter { $0 != name }) }
        guard mine.count < poll.max else { return .tooMany(poll.max) }
        return .vote(poll.options.map(\.name).filter { mine.contains($0) || $0 == name })
    }

    /// What's said of a choice one more than the poll takes.
    public static func tooMany(_ most: Int) -> String {
        "this poll takes \(most) at most, so take one back first"
    }
}
