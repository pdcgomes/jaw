import JawKit

/// A line of the roster: its title, a group's heading under All, someone, or the people folded under
/// "N offline", open or not.
public enum RosterLine: Hashable, Sendable {
    case title(String)
    case heading(String, around: Int, of: Int)
    case person(RosterRow, group: String)
    case offline(group: String, count: Int, open: Bool)
}

/// The roster as the sidebar draws it, and how someone is, in words.
public enum RosterLines {
    /// Under All, grouped by your top folders in their order, then everyone else under Others, each
    /// group's people who are around listed and the rest folded, as the terminal groups them; under
    /// any other folder, its people as jawd sorts them, those jawd has no last seen for folded.
    /// Someone is around while online, or while their chat says they're typing or recording, which
    /// `typing` holds by person id, since jawd keeps no typing on people's rows; someone typing is
    /// never folded. A fold in `open` lists its people after it.
    public static func lines(_ people: [RosterRow], folder: String, tree: FolderTree, open: Set<String>,
                             typing: Set<String> = []) -> [RosterLine]
    {
        var out: [RosterLine] = [.title(folder == "all" ? "People" : "People in \(tree.label(folder))")]
        let around = { (p: RosterRow) in p.presence.around || typing.contains(p.id) }
        func group(_ key: String, _ members: [RosterRow], listed: (RosterRow) -> Bool) {
            out += members.filter(listed).map { .person($0, group: key) }
            let folded = members.filter { !listed($0) }
            guard !folded.isEmpty else { return }
            out.append(.offline(group: key, count: folded.count, open: open.contains(key)))
            if open.contains(key) { out += folded.map { .person($0, group: key) } }
        }
        guard folder == "all" else {
            group(folder, people) { $0.presence.state != .hidden || typing.contains($0.id) }
            return out
        }
        var placed: Set<String> = []
        for top in tree.children(of: nil) {
            let under = tree.subtree(top.id)
            let members = people.filter { !under.isDisjoint(with: $0.folders) }
            guard !members.isEmpty else { continue }
            placed.formUnion(members.map(\.id))
            out.append(.heading(top.name, around: members.count(where: around), of: members.count))
            group(top.id, members, listed: around)
        }
        let rest = people.filter { !placed.contains($0.id) }
        if !rest.isEmpty {
            out.append(.heading("Others", around: rest.count(where: around), of: rest.count))
            group("others", rest, listed: around)
        }
        return out
    }

    /// How someone is, in words, and whether they're drawn in the online colour.
    public struct Said: Equatable, Sendable {
        public var text: String
        public var online: Bool
    }

    /// Typing…, in the online colour, while their chat says they're typing or recording: `typing`
    /// holds who, not which, so recording reads typing… too. Otherwise online; when last seen, as a
    /// time ago; nothing when they hide it.
    public static func said(_ presence: Presence, typing: Bool = false, now: Int64) -> Said {
        if typing { return Said(text: "typing…", online: true) }
        return switch presence.state {
        case .online: Said(text: "online", online: false)
        case .seen: Said(text: presence.at.map { Format.ago($0, now: now) } ?? "", online: false)
        case .hidden: Said(text: "", online: false)
        }
    }
}

/// Someone's menu in the roster.
public enum RosterMenu {
    public static let open = "person.open"
    public static let inNewPane = "person.pane"

    /// Open Chat, or Start Chat when they have none yet, and Open in New Pane; then the folder items,
    /// and Favourite for their chat when they have one.
    public static func entries(_ person: RosterRow, tree: FolderTree, list: String) -> [MenuEntry] {
        let opening: [MenuEntry] = [.command(open, title: person.chat == nil ? "Start Chat" : "Open Chat"), .command(inNewPane)]
        let folders = ChatMenu.with(FolderMenu.entries(Filed(person), tree: tree, list: list), FolderMenu.favourites(person.chat, tree: tree))
        return ChatMenu.with(opening, folders)
    }
}
