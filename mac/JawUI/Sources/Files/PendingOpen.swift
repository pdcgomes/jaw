import JawKit

/// A file asked to be looked at or opened before it was here, to be once it is: what each row that
/// comes for its message says to do. jawd clears a file's `error` when it's asked for, leaving its
/// state as it was, and sets one again whichever way it gives up, so expired, gone or a download
/// that failed can't come only with an error, and only on a row newer than the one it was asked
/// from, since the rows in flight when it was asked still carry the error it was asked again for.
public struct PendingOpen: Sendable, Hashable {
    /// The row it's of: the album's, for one of its items.
    public let row: String
    /// The file's own message: an album item's ID, for one of its items.
    public let message: String
    public let act: MediaOffer.Act
    /// The row's version when it was asked for.
    public let v: UInt64

    public init(_ act: MediaOffer.Act, _ offer: MediaOffer, of row: MessageRow) {
        self.row = row.id
        message = offer.message
        self.act = act
        v = row.v
    }

    public enum Next: Sendable, Hashable {
        /// It's still coming.
        case wait
        /// It's here: looked at or opened from the row's files, the one asked for at `at`.
        case open([MediaOffer], at: Int)
        /// It can't come, as its card says now, or the row no longer has it.
        case stop
    }

    /// What a row that came says to do; one of another message says to wait.
    public func next(_ came: MessageRow) -> Next {
        guard came.id == row else { return .wait }
        let offers = MediaOffer.all(came)
        guard let at = offers.firstIndex(where: { $0.message == message }) else { return .stop }
        if offers[at].isHere { return .open(offers, at: at) }
        let media = offers[at].media
        if came.v > v, media.error != nil, [.remote, .expired, .gone].contains(media.state) { return .stop }
        return .wait
    }
}
