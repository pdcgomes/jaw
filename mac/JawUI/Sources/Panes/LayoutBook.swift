import Foundation

/// The named layouts, and the one on screen: each keeps its shape and the chat in each pane. A
/// change of chats is saved at once; a change of shape (a split, a close, a line moved) marks the
/// layout edited, kept apart from the saved shape until Save Layout or Revert Layout, and kept
/// through switching and quitting. Before any is saved, the panes are the unnamed layout.
///
/// Kept as version 1 JSON in the app's preferences until jawd keeps an interface's state (P1).
public struct LayoutBook: Codable, Hashable, Sendable {
    public struct Saved: Codable, Hashable, Sendable {
        public var name: String
        public var grid: PaneGrid
        public var edited: PaneGrid?
        public var focus: Int?
    }

    public enum Problem: Error, Equatable, Sendable {
        case noName, taken, most, version

        public var said: String {
            switch self {
            case .noName: "a layout needs a name"
            case .taken: "there's a layout by that name already"
            case .most: "\(LayoutBook.most) layouts at most"
            case .version: "these layouts are a newer Jaw's"
            }
        }
    }

    public static let most = 30

    public private(set) var v = 1
    /// The layout on screen, by name; nil for the unnamed one.
    public private(set) var current: String?
    public private(set) var unnamed: PaneGrid?
    public private(set) var unnamedFocus: Int?
    public private(set) var layouts: [Saved] = []

    public init() {}

    var index: Int? {
        current.flatMap { name in layouts.firstIndex { $0.name == name } }
    }

    public var names: [String] {
        layouts.map(\.name)
    }

    /// What's on screen: the edited shape, the saved one, or the unnamed layout.
    public var grid: PaneGrid? {
        index.map { layouts[$0].edited ?? layouts[$0].grid } ?? unnamed
    }

    public var focus: Int? {
        get { index.map { layouts[$0].focus } ?? unnamedFocus }
        set {
            if let i = index { layouts[i].focus = newValue } else { unnamedFocus = newValue }
        }
    }

    /// Whether the layout on screen has a shape it wasn't saved with.
    public var isEdited: Bool {
        index.map { layouts[$0].edited != nil } ?? false
    }

    /// The layout on screen changed. Its chats are saved at once, pane by pane; a shape unlike the
    /// saved one is kept as the edit, and one like it clears the edit.
    public mutating func update(_ new: PaneGrid?) {
        guard let i = index else {
            unnamed = new
            return
        }
        guard let new else { return }
        var saved = layouts[i].grid
        for slot in saved.slots {
            if let now = new.slot(slot.id), now.chat != slot.chat { saved = saved.with(chat: now.chat, in: slot.id) }
        }
        layouts[i].grid = saved
        layouts[i].edited = new.shape == saved.shape ? nil : new
        if layouts[i].edited == nil { layouts[i].grid = new }
    }

    /// Keeps the edited shape as the layout's.
    public mutating func save() {
        guard let i = index, let edited = layouts[i].edited else { return }
        layouts[i].grid = edited
        layouts[i].edited = nil
    }

    /// Back to the saved shape.
    public mutating func revert() {
        guard let i = index else { return }
        layouts[i].edited = nil
    }

    /// Saves what's on screen under a new name and goes to it; an edited layout it came from keeps
    /// its saved shape.
    public mutating func saveAs(_ name: String) throws(Problem) {
        let name = try check(name)
        guard layouts.count < Self.most else { throw .most }
        guard let grid else { return }
        let focus = self.focus
        if let i = index { layouts[i].edited = nil }
        layouts.append(Saved(name: name, grid: grid, edited: nil, focus: focus))
        current = name
    }

    public mutating func rename(_ old: String, to new: String) throws(Problem) {
        guard let i = layouts.firstIndex(where: { $0.name == old }) else { return }
        let name = new.trimmingCharacters(in: .whitespacesAndNewlines)
        if name == old { return }
        layouts[i].name = try check(name)
        if current == old { current = layouts[i].name }
    }

    /// Deletes a layout; deleting the one on screen keeps its panes as the unnamed layout.
    public mutating func delete(_ name: String) {
        guard let i = layouts.firstIndex(where: { $0.name == name }) else { return }
        if current == name {
            unnamed = layouts[i].edited ?? layouts[i].grid
            unnamedFocus = layouts[i].focus
            current = nil
        }
        layouts.remove(at: i)
    }

    /// Goes to a layout by name, or to the unnamed one; each keeps its edit.
    public mutating func switchTo(_ name: String?) {
        guard name == nil || layouts.contains(where: { $0.name == name }) else { return }
        current = name
    }

    func check(_ name: String) throws(Problem) -> String {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { throw .noName }
        guard !layouts.contains(where: { $0.name.caseInsensitiveCompare(name) == .orderedSame }) else { throw .taken }
        return name
    }

    enum Keys: String, CodingKey {
        case v, current, unnamed, unnamedFocus, layouts
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        v = try c.decode(Int.self, forKey: .v)
        guard v == 1 else { throw Problem.version }
        current = try c.decodeIfPresent(String.self, forKey: .current)
        unnamed = try c.decodeIfPresent(PaneGrid.self, forKey: .unnamed)
        unnamedFocus = try c.decodeIfPresent(Int.self, forKey: .unnamedFocus)
        layouts = try c.decode([Saved].self, forKey: .layouts)
        if index == nil { current = nil }
    }
}
