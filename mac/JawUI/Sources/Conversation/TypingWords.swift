import Foundation
import JawKit

/// Who's typing in a chat, in the terminal's words (`who_types`): "typing…", or "recording…" while
/// everyone typing is recording; in a group with their names first, as their messages name them.
/// Where there isn't room for them all, the names are cut short and the words never are.
public enum TypingWords {
    /// As the chat list's row and the status line say it, and VoiceOver the typing bubble: "typing…",
    /// "Ana, Rui typing…". Nil while nobody's typing.
    public static func line(_ typists: [Typist], group: Bool) -> String? {
        guard let words = words(typists) else { return nil }
        let names = who(typists, group: group)
        return names.isEmpty ? words : "\(names) \(words)"
    }

    /// `line` as it fits in `room`, measured by `width`: the names cut short before what they're
    /// doing, which always shows.
    static func line(_ typists: [Typist], group: Bool, in room: CGFloat, width: (String) -> CGFloat) -> String? {
        guard let words = words(typists) else { return nil }
        return fitted(who(typists, group: group), before: words, in: room, width: width) + words
    }

    /// What they're doing, as the header says it before its counting dots: "typing", or "recording"
    /// while everyone is. Nil while nobody's typing.
    public static func doing(_ typists: [Typist]) -> String? {
        guard !typists.isEmpty else { return nil }
        return typists.allSatisfy { $0.state == .recording } ? "recording" : "typing"
    }

    /// What they're doing, as a line says it after their names: "typing…" or "recording…".
    public static func words(_ typists: [Typist]) -> String? {
        doing(typists).map { $0 + "…" }
    }

    /// Who, in a group: their names, oldest first, "Ana, Rui". A one-to-one chat names nobody.
    public static func who(_ typists: [Typist], group: Bool) -> String {
        group ? typists.map(\.name).joined(separator: ", ") : ""
    }

    /// The characters a name cut short loses from its end before its "…".
    static let trailing = CharacterSet(charactersIn: ", ")

    /// `who` and a space, as they fit in `room` before `after`, measured with it by `width`: whole,
    /// or cut short with "…", or nothing when even "…" doesn't fit.
    static func fitted(_ who: String, before after: String, in room: CGFloat, width: (String) -> CGFloat) -> String {
        guard !who.isEmpty else { return "" }
        guard width(who + " " + after) > room else { return who + " " }
        // The longest start of the names that fits, found by halves, since a longer start is no narrower.
        let characters = Array(who)
        var lo = 1, hi = characters.count - 1, best = ""
        while lo <= hi {
            let n = (lo + hi) / 2
            let cut = String(characters[..<n]).trimmingCharacters(in: trailing) + "… "
            if width(cut + after) <= room {
                best = cut
                lo = n + 1
            } else {
                hi = n - 1
            }
        }
        return best
    }
}
