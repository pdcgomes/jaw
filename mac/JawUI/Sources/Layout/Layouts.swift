import Foundation
import JawKit
import Synchronization

/// What every layout in a pane shares: the style, its fonts, the IDs you go by, so a mention of you
/// stands out, and where the messages' pictures are read from.
public final class LayoutSpec: Sendable, Hashable {
    public let style: Style
    public let fonts: FontSet
    public let me: Set<String>
    public let pictures: (any ThumbnailSource)?

    public init(style: Style, me: Set<String>, pictures: (any ThumbnailSource)? = nil) {
        self.style = style
        fonts = FontSet(size: style.textSize, mono: style.font == .mono)
        self.me = me
        self.pictures = pictures
    }

    public static func == (a: LayoutSpec, b: LayoutSpec) -> Bool {
        a.style == b.style && a.me == b.me && a.pictures?.id == b.pictures?.id
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(style)
        hasher.combine(me)
        hasher.combine(pictures?.id)
    }
}

/// The layout cache: each message's layout per version, width, look and context, as the terminal
/// keeps its lines per message, version and width. A message's `v` changes when anything it shows
/// changes, so an edit, a reaction or a sender's new name lays it out again, and nothing else does.
/// Layouts are made on the layout queue ahead of drawing, and on the main thread only for a row
/// asked for before its layout is ready.
public final class Layouts: Sendable {
    struct Key: Hashable {
        let id: String
        let v: UInt64
        let width: Int
        let spec: LayoutSpec
        let context: MessageContext
    }

    let cache = Mutex<[Key: Laid]>([:])
    let queue = DispatchQueue(label: "dev.badcoffee.jaw.layout", qos: .userInitiated, attributes: .concurrent)

    public init() {}

    /// Widths are rounded down to whole points, so a resize lays out once a point.
    static func key(_ m: MessageRow, _ width: CGFloat, _ spec: LayoutSpec, _ context: MessageContext) -> Key {
        Key(id: m.id, v: m.v, width: Int(width), spec: spec, context: context)
    }

    /// The message's layout, made now if it isn't ready.
    public func laid(_ m: MessageRow, width: CGFloat, spec: LayoutSpec, context: MessageContext) -> Laid {
        let key = Self.key(m, width, spec, context)
        if let laid = cache.withLock({ $0[key] }) { return laid }
        let laid = MessageLayout.lay(m, width: CGFloat(key.width), context: context, style: spec.style,
                                     fonts: spec.fonts, me: spec.me, pictures: spec.pictures)
        cache.withLock { $0[key] = laid }
        return laid
    }

    public func has(_ m: MessageRow, width: CGFloat, spec: LayoutSpec, context: MessageContext) -> Bool {
        let key = Self.key(m, width, spec, context)
        return cache.withLock { $0[key] != nil }
    }

    /// Lays out the messages not yet laid out on the layout queue, those listed first before the
    /// rest, then calls `done` on the main thread.
    public func prepare(_ items: [(MessageRow, MessageContext)], width: CGFloat, spec: LayoutSpec,
                        done: @escaping @MainActor @Sendable () -> Void)
    {
        let missing = items.filter { !has($0.0, width: width, spec: spec, context: $0.1) }
        guard !missing.isEmpty else {
            DispatchQueue.main.async { MainActor.assumeIsolated { done() } }
            return
        }
        queue.async { [self] in
            DispatchQueue.concurrentPerform(iterations: missing.count) { i in
                _ = laid(missing[i].0, width: width, spec: spec, context: missing[i].1)
            }
            DispatchQueue.main.async { MainActor.assumeIsolated { done() } }
        }
    }

    /// Keeps only what's laid out for these messages at this width and spec, so the cache can't
    /// outgrow the windows.
    public func keep(_ ids: Set<String>, width: CGFloat, spec: LayoutSpec) {
        let w = Int(width)
        cache.withLock { cache in
            cache = cache.filter { ids.contains($0.key.id) && $0.key.width == w && $0.key.spec == spec }
        }
    }

    public var count: Int {
        cache.withLock { $0.count }
    }
}
