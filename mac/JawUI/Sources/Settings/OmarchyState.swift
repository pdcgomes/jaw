import Foundation

/// What the gallery's Omarchy section says while jawd hasn't served its themes: nothing while the app
/// connects or asks, as jawd answers at once; "Starting jawd" while the jawd the app started starts;
/// that jawd isn't answering while it can't be reached or couldn't start; that it's older and serves
/// no themes when it doesn't know `themes`; and any other refusal in jawd's words.
public enum OmarchyState: Sendable, Hashable {
    case connecting, asking, starting, unreachable, older
    case refused(String)

    /// From how jawd is reached, before its answer to `themes`.
    public init(_ reach: Situation.Reach) {
        switch reach {
        case .waiting: self = .connecting
        case .starting(failed: nil): self = .starting
        case .starting: self = .unreachable
        case .up: self = .asking
        case .unreachable: self = .unreachable
        }
    }

    /// The line the section shows in place of the themes it hasn't, or nil.
    public var line: String? {
        switch self {
        case .connecting, .asking: nil
        case .starting: "Starting jawd"
        case .unreachable: "Omarchy's themes come from jawd, which isn't answering; they show once it does"
        case .older: "This jawd is older and serves no themes; restart it"
        case let .refused(why): "Omarchy's themes didn't come: \(why)"
        }
    }
}
