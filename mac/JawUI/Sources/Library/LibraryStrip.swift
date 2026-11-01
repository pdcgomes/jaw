import AppKit
import CoreText
import JawKit
import JawThemes

/// The strip of kinds over the library, as the terminal's: All, then each kind with jawd's count,
/// `1,000+` where jawd counted only so far, the one shown marked; Tab and ⇧Tab step through them.
public enum LibraryKinds {
    /// The strip's order: All first, then each kind.
    public static let order: [LibraryKind?] = [nil] + LibraryKind.all.map(Optional.some)

    /// A kind's name on the strip.
    public static func name(_ kind: LibraryKind?) -> String {
        switch kind {
        case nil: "All"
        case .photo?: "Photos"
        case .video?: "Videos"
        case .voice?: "Voice"
        case .file?: "Files"
        case .link?: "Links"
        case .other?: "Other"
        }
    }

    /// A kind's plural, as the count at the strip's end says it: "214 photos".
    public static func plural(_ kind: LibraryKind?, _ n: Int) -> String {
        switch kind {
        case nil: n == 1 ? "item" : "items"
        case .photo?: n == 1 ? "photo" : "photos"
        case .video?: n == 1 ? "video" : "videos"
        case .voice?: n == 1 ? "voice message" : "voice messages"
        case .file?: n == 1 ? "file" : "files"
        case .link?: n == 1 ? "link" : "links"
        case .other?: n == 1 ? "item" : "items"
        }
    }

    /// Each kind's label: its name, and its count when jawd has one above nothing.
    public static func labels(_ counts: LibraryCounts?, capped: Bool) -> [(kind: LibraryKind?, label: String)] {
        order.map { kind in
            guard let kind, let n = counts?[kind], n > 0 else { return (kind, name(kind)) }
            return (kind, "\(name(kind)) \(LibraryLayout.counted(n, capped: capped))")
        }
    }

    /// The kind after `kind` on the strip, or before it, going round.
    public static func step(_ kind: LibraryKind?, by: Int) -> LibraryKind? {
        let at = order.firstIndex(of: kind) ?? 0
        return order[((at + by) % order.count + order.count) % order.count]
    }

    /// What the strip's end says of what's shown: that jawd's being asked, or how many there are.
    public static func said(_ kind: LibraryKind?, loaded: Bool, total: UInt64, capped: Bool, filtered: Bool) -> String {
        guard loaded else { return "finding…" }
        if kind == nil, !filtered { return "newest first" }
        let n = Int(total)
        return "\(LibraryLayout.counted(n, capped: capped)) \(plural(kind, n))"
    }

    /// The labels' places across `width`, from its left, each as wide as its words and padding; with
    /// no room for the counts, the names alone.
    static func segments(_ labels: [(kind: LibraryKind?, label: String)], width: CGFloat) -> [(kind: LibraryKind?, label: String, frame: CGRect)] {
        let font = TextStyle.callout.emphasised as CTFont
        func lay(_ these: [(kind: LibraryKind?, label: String)]) -> [(kind: LibraryKind?, label: String, frame: CGRect)] {
            var x: CGFloat = 0
            return these.map { kind, label in
                let w = ceil(LibraryDrawing.width(LibraryDrawing.line(label, font, RGBA(0))) + 2 * Space.m)
                defer { x += w + Space.xxs }
                return (kind, label, CGRect(x: x, y: 0, width: w, height: height))
            }
        }
        let full = lay(labels)
        guard let last = full.last, last.frame.maxX > width else { return full }
        return lay(labels.map { ($0.kind, name($0.kind)) })
    }

    static let height: CGFloat = 24
}

/// The strip drawn: each kind's label, the one shown on the selection's fill, and at its right what's
/// shown; a click on a label picks its kind.
final class LibraryStrip: NSView {
    var labels: [(kind: LibraryKind?, label: String)] = [] {
        didSet { needsDisplay = true }
    }

    var shown: LibraryKind? {
        didSet { needsDisplay = true }
    }

    /// What's shown, at the strip's right: "finding…", "214 photos".
    var said = "" {
        didSet { needsDisplay = true }
    }

    var roles: Roles? {
        didSet { needsDisplay = true }
    }

    var picked: ((LibraryKind?) -> Void)?

    override var isFlipped: Bool {
        true
    }

    var segments: [(kind: LibraryKind?, label: String, frame: CGRect)] {
        let room = max(bounds.width - 2 * Space.l - 120, 0)
        let top = ((bounds.height - LibraryKinds.height) / 2).rounded()
        return LibraryKinds.segments(labels, width: room).map { ($0.kind, $0.label, $0.frame.offsetBy(dx: Space.l, dy: top)) }
    }

    override func draw(_: NSRect) {
        guard let roles, let ctx = NSGraphicsContext.current?.cgContext else { return }
        LibraryStrip.parts(segments, shown: shown, said: said, width: bounds.width, height: bounds.height, roles: roles)
            .forEach { Laid.draw([$0], in: ctx, y: 0) }
    }

    /// What the strip draws, as parts, for the cell and the tests alike.
    nonisolated static func parts(_ segments: [(kind: LibraryKind?, label: String, frame: CGRect)], shown: LibraryKind?, said: String, width: CGFloat,
                      height: CGFloat, roles: Roles) -> [Laid.Part]
    {
        var out: [Laid.Part] = []
        let bold = TextStyle.callout.emphasised as CTFont, quiet = TextStyle.caption.nsFont as CTFont
        for s in segments {
            let on = s.kind == shown
            if on { out.append(.fill(s.frame, roles.selection.cgColor, Radius.control)) }
            let l = LibraryDrawing.line(s.label, bold, on ? roles.text : roles.secondary)
            out.append(.line(l, CGPoint(x: (s.frame.midX - LibraryDrawing.width(l) / 2).rounded(), y: LibraryDrawing.baseline(bold, mid: s.frame.midY))))
        }
        let end = LibraryDrawing.line(said, quiet, roles.tertiary)
        let x = width - Space.l - LibraryDrawing.width(end)
        if x > (segments.last?.frame.maxX ?? 0) + Space.m {
            out.append(.line(end, CGPoint(x: x.rounded(), y: LibraryDrawing.baseline(quiet, mid: height / 2))))
        }
        return out
    }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        if let s = segments.first(where: { $0.frame.contains(p) }) { picked?(s.kind) }
    }

    override func isAccessibilityElement() -> Bool {
        true
    }

    override func accessibilityRole() -> NSAccessibility.Role? {
        .tabGroup
    }

    override func accessibilityLabel() -> String? {
        "Kinds: " + labels.map { $0.kind == shown ? "\($0.label), shown" : $0.label }.joined(separator: "; ") + (said.isEmpty ? "" : ". \(said)")
    }
}
