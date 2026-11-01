import AppKit
import JawKit
import JawThemes

/// One element of the library's grid: a heading, a tile or a row, drawn from its value. A tile asks
/// for its picture as it's given an item and lets it go when it's reused or leaves the window, so
/// what a fling passes isn't decoded; a preview that comes after it has drawn fades in on its own
/// layer, as a conversation's pictures do.
final class LibraryCell: NSView {
    var element: LibraryLaid.Element.What?
    var item: LibraryRow?
    var context: LibraryDrawing.Context? {
        didSet { needsDisplay = true }
    }

    var selected = false {
        didSet { if selected != oldValue { needsDisplay = true } }
    }

    /// The pane's pictures, and this cell's claim on them.
    var pictures: MessagePictures? {
        didSet {
            guard pictures !== oldValue else { return }
            if let token { oldValue?.letGo(token) }
            token = pictures?.token()
        }
    }

    var token: PictureToken?
    var askedScale: CGFloat?
    /// The size its picture was last asked for at, so a cell given its frame after it came into its
    /// window asks at its own.
    var askedSize: CGSize?
    /// What its picture was last drawn from: 0 nothing, 1 the blurred thumbnail, 2 a preview.
    var drawnFrom = 0
    var drewOnce = false
    var clicked: ((LibraryCell, Int) -> Void)?
    var menuFor: ((LibraryCell) -> NSMenu?)?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layerContentsRedrawPolicy = .onSetNeedsDisplay
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var isFlipped: Bool {
        true
    }

    override var wantsUpdateLayer: Bool {
        false
    }

    var scale: CGFloat {
        window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 2
    }

    /// Gives the cell what it draws; a new item, or a new size, is asked for again.
    func show(_ element: LibraryLaid.Element.What, item: LibraryRow?, context: LibraryDrawing.Context, selected: Bool) {
        let changed = element != self.element || item?.id != self.item?.id || item?.v != self.item?.v
        self.element = element
        self.item = item
        self.context = context
        self.selected = selected
        if changed {
            drawnFrom = 0
            drewOnce = false
            letGo()
            askPictures()
        }
        needsDisplay = true
    }

    var drawn: LibraryDrawn? {
        guard let element, let context else { return nil }
        return LibraryDrawing.drawn(element, item: item, size: bounds.size, selected: selected, context)
    }

    override func draw(_: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext, let drawn else { return }
        let drawable = drawn.slot.flatMap { pictures?.drawable($0, scale: scale) }
        drawn.draw(in: ctx, picture: drawable?.image)
        drawnFrom = drawable.map { $0.isPreview ? 2 : 1 } ?? 0
        drewOnce = true
        // Drawn at a size it hasn't asked at, as a cell given its frame after it came into its window
        // is: it asks at this one once it's drawn.
        if drawn.slot != nil, window != nil, askedSize != bounds.size {
            DispatchQueue.main.async { [weak self] in self?.askAgain() }
        }
    }

    /// Asks for its picture at this cell's pixels, once it's in a window, whose scale says them, and
    /// has a size.
    func askPictures() {
        guard window != nil, bounds.width >= 1, bounds.height >= 1, let pictures, let token, let slot = drawn?.slot else { return }
        askedScale = scale
        askedSize = bounds.size
        pictures.ask(slot, scale: scale, for: token) { [weak self] in self?.pictureCame() }
    }

    /// What it asked for at another size let go, and its own asked for.
    func askAgain() {
        guard window != nil, askedSize != bounds.size else { return }
        letGo()
        askPictures()
    }

    func letGo() {
        if let token { pictures?.letGo(token) }
        askedSize = nil
    }

    /// A picture came: drawn again, fading in when it's better than what was drawn, shorter under
    /// Reduce Motion.
    func pictureCame() {
        guard let slot = drawn?.slot, let pictures else { return }
        let now = pictures.drawable(slot, scale: scale).map { $0.isPreview ? 2 : 1 } ?? 0
        if drewOnce, now > drawnFrom {
            let fade = CATransition()
            fade.type = .fade
            fade.duration = Motion.reduced() ? Motion.quick / 2 : Motion.quick
            layer?.add(fade, forKey: "picture")
        }
        needsDisplay = true
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil { letGo() } else { askPictures() }
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        guard window != nil, askedScale != nil, scale != askedScale else { return }
        letGo()
        askPictures()
        needsDisplay = true
    }

    override func setFrameSize(_ size: NSSize) {
        let changed = size != frame.size
        super.setFrameSize(size)
        // A tile drawn at another size takes a decode at its own pixels.
        if changed, window != nil, drawn?.slot != nil { askAgain() }
    }

    override func mouseDown(with event: NSEvent) {
        clicked?(self, event.clickCount)
    }

    override func menu(for _: NSEvent) -> NSMenu? {
        menuFor?(self)
    }

    override func isAccessibilityElement() -> Bool {
        true
    }

    override func accessibilityRole() -> NSAccessibility.Role? {
        element.map { if case .heading = $0 { .staticText } else { .button } }
    }

    override func accessibilityLabel() -> String? {
        switch element {
        case let .heading(title, count, _)?: return [title, count].compactMap(\.self).joined(separator: ", ")
        case let .said(words)?: return words
        case .tile?, .row?: return item.map(Self.spoken)
        case nil: return nil
        }
    }

    /// An item as VoiceOver says it: what it is, its name or length, who shared it and when.
    static func spoken(_ item: LibraryRow) -> String {
        let what: String = switch item.kind {
        case .photo: "Photo"
        case .video: "Video" + (item.media?.seconds.map { ", \(Format.length($0))" } ?? "")
        case .voice: "Voice message" + (item.voice?.seconds.map { ", \(Format.length($0))" } ?? "")
        case .file: item.file?.name ?? "File"
        case .link: item.link?.title ?? item.link?.url ?? "Link"
        case .other: "Item"
        }
        let who = item.mine ? "You" : item.from?.name ?? ""
        return [what, who, item.chatName, Format.clock(item.at)].filter { !$0.isEmpty }.joined(separator: ", ")
    }
}

/// The grid's item: its view is a `LibraryCell`, which lets its picture go as it's reused.
final class LibraryGridItem: NSCollectionViewItem {
    static let id = NSUserInterfaceItemIdentifier("library")

    var cell: LibraryCell {
        view as! LibraryCell
    }

    override func loadView() {
        view = LibraryCell(frame: .zero)
    }

    override func prepareForReuse() {
        super.prepareForReuse()
        cell.letGo()
    }
}

/// The grid's layout: each element where `LibraryLayout` put it, so the collection view places what
/// was laid out and lays out nothing itself.
final class LibraryGridLayout: NSCollectionViewLayout {
    var laid = LibraryLaid.empty

    override var collectionViewContentSize: NSSize {
        NSSize(width: laid.width, height: laid.height)
    }

    override func layoutAttributesForElements(in rect: NSRect) -> [NSCollectionViewLayoutAttributes] {
        laid.indices(in: rect).map { attributes(at: $0) }
    }

    override func layoutAttributesForItem(at indexPath: IndexPath) -> NSCollectionViewLayoutAttributes? {
        laid.elements.indices.contains(indexPath.item) ? attributes(at: indexPath.item) : nil
    }

    func attributes(at i: Int) -> NSCollectionViewLayoutAttributes {
        let a = NSCollectionViewLayoutAttributes(forItemWith: IndexPath(item: i, section: 0))
        a.frame = laid.elements[i].frame
        return a
    }

    override func shouldInvalidateLayout(forBoundsChange newBounds: NSRect) -> Bool {
        abs(newBounds.width - laid.width) >= 1
    }
}
