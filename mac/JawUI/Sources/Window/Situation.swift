import Foundation
import JawKit

/// What the app knows of jawd and the account, as a value: how jawd is reached, whether the account
/// is linked, the connection, the link step and what's been said. The app fills one from jawd's
/// pushes and the harness makes one for each state, and what the screen says comes from it alone.
public struct Situation: Equatable, Sendable {
    /// How jawd is reached.
    public enum Reach: Equatable, Sendable {
        case waiting
        case starting(failed: String?)
        case up
        case unreachable(why: String, retry: Int)
    }

    /// Something said in the status line for a few seconds: a refusal, a hint.
    public struct Said: Equatable, Sendable {
        public var text: String
        public var until: Date
        public var warn: Bool

        public init(_ text: String, until: Date, warn: Bool = false) {
            self.text = text
            self.until = until
            self.warn = warn
        }
    }

    public var reach: Reach
    public var linked: Bool
    public var connection: Connection
    public var link: LinkStep?
    public var said: Said?

    public init(reach: Reach = .waiting, linked: Bool = true, connection: Connection = Connection(state: .connecting),
                link: LinkStep? = nil, said: Said? = nil)
    {
        self.reach = reach
        self.linked = linked
        self.connection = connection
        self.link = link
        self.said = said
    }

    /// The status line's left half: jawd's state when it isn't online, as the terminal's header says
    /// it, and whether it warns.
    public var stateLine: (text: String, warn: Bool)? {
        switch reach {
        case .waiting: return ("connecting to jawd…", false)
        case .starting(nil): return ("starting jawd…", false)
        case .starting: return ("jawd didn't start", true)
        case let .unreachable(_, retry): return ("can't reach jawd · trying again in \(retry) s", true)
        case .up: break
        }
        if !linked { return ("not linked", true) }
        switch connection.state {
        case .online: return nil
        case .connecting: return ("connecting", false)
        case .catchingUp:
            guard let done = connection.done, let total = connection.total else { return ("catching up", false) }
            return ("catching up \(Format.num(done)) of \(Format.num(total))", false)
        case .phoneOffline: return ("phone offline: photos and older history wait for it", true)
        case .offline: return ("offline: what you send waits until it's back", true)
        case .updateNeeded: return ("update needed: WhatsApp wants a newer Jaw · reading still works", true)
        case .banned: return ("temporarily banned until \(connection.until.map { Format.until($0) } ?? "later")", true)
        case .engineRestarting: return ("reconnecting: the engine restarted", true)
        case .loggedOut: return ("logged out", true)
        case .other: return ("connection: \(connection.reason ?? "a state this Jaw doesn't know")", true)
        }
    }

    /// The status line's word on the focused chat: who's typing there, which it draws in the online
    /// colour, and the rest, quieter.
    public struct ChatLine: Equatable, Sendable {
        public var typing: String?
        public var rest: String?

        public init(typing: String? = nil, rest: String? = nil) {
            self.typing = typing
            self.rest = rest
        }
    }

    /// How the focused chat is, without its name, which its box and header already say: who's typing
    /// in it, `typing`, and whether it's muted; or what was said, while it lasts, in their place.
    public func chatLine(focused chat: ChatRow?, typing: [Typist] = [], now: Date) -> ChatLine? {
        if let said, said.until > now { return ChatLine(rest: said.text) }
        guard let chat else { return nil }
        let line = ChatLine(typing: TypingWords.line(typing, group: chat.kind.isGroup), rest: chat.muted ? "muted" : nil)
        return line == ChatLine() ? nil : line
    }

    /// What fills the panes' place when it isn't chats: jawd starting or out of reach, linking, or
    /// nothing open.
    public func stateView(hasChats: Bool, hasPanes: Bool) -> StateView.Kind? {
        switch reach {
        case let .starting(failed): return .starting(failed: failed)
        case let .unreachable(why, retry) where !hasChats: return .asleep(why: why, retryIn: retry)
        default: break
        }
        if reach == .up, !linked {
            return .linking(step: link, loggedOut: connection.state == .loggedOut ? (connection.reason ?? "logged out from the phone") : nil)
        }
        if !hasPanes {
            return !hasChats && reach == .up ? .noChats : .noChat
        }
        return nil
    }

    /// The chats that need you that no pane shows: mentions first, then people, then groups, each by
    /// its unread, as the spike's status line reads them from the chat list.
    public static func needs(_ chats: [ChatRow], shown: Set<String>) -> [ChatRow] {
        let waiting = chats.filter {
            !shown.contains($0.id) && !$0.muted && !$0.archived && $0.kind != .community && ($0.unread > 0 || $0.markedUnread)
        }
        func rank(_ c: ChatRow) -> Int {
            c.mentions > 0 ? 0 : c.kind.isGroup ? 2 : 1
        }
        return waiting.sorted { (rank($0), -$0.unread) < (rank($1), -$1.unread) }
    }
}
