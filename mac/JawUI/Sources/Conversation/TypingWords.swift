import JawKit

/// Who's typing in a chat, in the terminal's words (`who_types`): "typing…", or "recording…" while
/// everyone typing is recording; in a group with their names first, as their messages name them.
public enum TypingWords {
    /// As the chat list's row and the status line say it, and VoiceOver the typing bubble: "typing…",
    /// "Ana, Rui typing…". Nil while nobody's typing.
    public static func line(_ typists: [Typist], group: Bool) -> String? {
        guard let doing = doing(typists) else { return nil }
        let names = who(typists, group: group)
        return (names.isEmpty ? doing : "\(names) \(doing)") + "…"
    }

    /// What they're doing, as the header says it before its counting dots: "typing", or "recording"
    /// while everyone is. Nil while nobody's typing.
    public static func doing(_ typists: [Typist]) -> String? {
        guard !typists.isEmpty else { return nil }
        return typists.allSatisfy { $0.state == .recording } ? "recording" : "typing"
    }

    /// Who, in a group: their names, oldest first, "Ana, Rui". A one-to-one chat names nobody.
    public static func who(_ typists: [Typist], group: Bool) -> String {
        group ? typists.map(\.name).joined(separator: ", ") : ""
    }
}
