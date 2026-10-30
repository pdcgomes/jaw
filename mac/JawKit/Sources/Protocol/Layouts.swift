import Foundation

/// `layout.get`'s params: whose panes, by the name its interface gives in hello; the connection's
/// own when nil.
public struct LayoutGet: Encodable, Sendable, Hashable {
    public var interface: String?
}

/// What jawd keeps of an interface's panes, as `layout.get` answers: the interface, the version
/// kept (0 for none), the panes, unless none are kept, none are left or they're in a format this
/// Jaw doesn't read, and the chats of the panes jawd left out, which are gone or shown by a pane
/// before them.
public struct KeptLayout: Decodable, Sendable, Hashable {
    public var interface: String
    public var v: UInt64
    public var layout: SplitLayout?
    public var dropped: [String]

    public init(interface: String, v: UInt64, layout: SplitLayout? = nil, dropped: [String] = []) {
        self.interface = interface
        self.v = v
        self.layout = layout
        self.dropped = dropped
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        interface = try c.decode(String.self, forKey: .interface)
        v = try c.decode(UInt64.self, forKey: .v)
        dropped = try c.decodeIfPresent([String].self, forKey: .dropped) ?? []
        if try c.decodeIfPresent(Format.self, forKey: .layout)?.format == 1 {
            layout = try c.decode(SplitLayout.self, forKey: .layout)
        } else {
            layout = nil
        }
    }

    /// A layout's format alone, read before the rest: format 1's tree is read whole, failing the
    /// answer if it's malformed, and any other format is one this Jaw doesn't read.
    struct Format: Decodable {
        let format: Int
    }

    enum Keys: String, CodingKey {
        case interface, v, layout, dropped
    }
}

/// Panes as a tree of splits, format 1, as the terminal keeps them, and the pane with the keys,
/// counted from 0 in the tree's order, a split's first side before its second.
public struct SplitLayout: Decodable, Sendable, Hashable {
    public var focused: Int
    public var tree: SplitTree

    public init(focused: Int, tree: SplitTree) {
        self.focused = focused
        self.tree = tree
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        let format = try c.decode(Int.self, forKey: .format)
        guard format == 1 else {
            throw DecodingError.dataCorruptedError(forKey: .format, in: c,
                                                   debugDescription: "this Jaw reads format 1's tree, not format \(format)")
        }
        focused = try c.decode(Int.self, forKey: .focused)
        tree = try c.decode(SplitTree.self, forKey: .tree)
    }

    enum Keys: String, CodingKey {
        case format, focused, tree
    }
}

/// A pane, by the JID of the chat it shows, or a split of two: `across` puts its first side on the
/// left, `down` on top, and `first` is that side's share in thousandths, 1 to 999.
public indirect enum SplitTree: Decodable, Sendable, Hashable {
    case chat(String)
    case split(How, first: Int, SplitTree, SplitTree)

    public enum How: String, Decodable, Sendable, Hashable {
        case across, down
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        if let chat = try c.decodeIfPresent(String.self, forKey: .chat) {
            self = .chat(chat)
            return
        }
        let split = try c.nestedContainer(keyedBy: SplitKeys.self, forKey: .split)
        self = try .split(split.decode(How.self, forKey: .how), first: split.decode(Int.self, forKey: .first),
                          split.decode(SplitTree.self, forKey: .a), split.decode(SplitTree.self, forKey: .b))
    }

    enum Keys: String, CodingKey {
        case chat, split
    }

    enum SplitKeys: String, CodingKey {
        case how, first, a, b
    }
}
