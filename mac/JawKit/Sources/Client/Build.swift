import Foundation

/// Where this Jaw was built from, as `make mac` records it in the bundle's Info.plist (`JawBuiltFrom`
/// and `JawBuiltAt`, from `mac/scripts/built-from`): a commit on origin/main, a release's tag on one,
/// a branch, or a tree with uncommitted changes to what jawd and jaw-engine are built from. Only the
/// first two start a jawd on an account's folder, jaw's own or one holding a `session.db`, where a
/// branch's jawd could move jaw.db to a version main's can't open.
public struct Build: Equatable, Sendable {
    /// `main`, `release`, `branch` or `changed`; nil when the bundle doesn't say, as one `make mac`
    /// didn't make doesn't.
    public var from: String?
    /// The commit, tag or branch, for what the app says: "c50a2ba3 on main", "v0.2.0" or
    /// "mac-start-jawd at 1a2b3c4, not on origin/main".
    public var at: String?

    public init(from: String?, at: String?) {
        self.from = from
        self.at = at
    }

    /// The bundle's, from its Info.plist's keys.
    public init(info: [String: Any]) {
        self.init(from: info["JawBuiltFrom"] as? String, at: info["JawBuiltAt"] as? String)
    }

    /// Whether the jawd inside it may start on an account's folder.
    public var startsOnAccounts: Bool {
        from == "main" || from == "release"
    }

    /// Why the jawd inside it doesn't start on an account's folder, and how to start one there.
    public var refusal: String {
        let built = at.map { "this Jaw was built from \($0)" } ?? "this Jaw doesn't say what it was built from, as make mac's builds do"
        return "\(built), so it doesn't start jawd on jaw's own folder or another account's, where a jawd from anything but origin/main or a "
            + "release's tag could move jaw.db to a version origin/main's can't open: start one with jaw in a terminal, or open a Jaw built from origin/main"
    }
}
