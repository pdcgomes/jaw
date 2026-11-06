import AppKit
import JawKit
import JawThemes
import QuickLookThumbnailing

/// The box's strip: what's attached, a tile each, in a row that scrolls across, with SD or HD at its
/// end while there's a photo or a video among them, and what's in the way under it. A photo's or a
/// video's tile is its picture, Quick Look's, made off the main thread; a file's is its icon and its
/// name; each has an × that takes it out.
@MainActor
public final class AttachmentStrip: NSView {
    /// A tile's ×, by its place.
    public var removed: ((Int) -> Void)?
    /// SD or HD clicked.
    public var switched: (() -> Void)?
    /// Makes a file's picture at a size in points and gives it back on the main thread: Quick Look's,
    /// or the harness's own.
    public var thumbnail: @MainActor (URL, CGSize, @escaping @MainActor (CGImage?) -> Void) -> Void = AttachmentStrip.quickLook
    public var roles: Roles? {
        didSet { colour() }
    }

    let scroll = NSScrollView()
    let row = NSStackView()
    public private(set) var tiles: [AttachmentTile] = []
    lazy var quality = IconButton(symbol: "", label: "SD or HD", shortcut: Shortcut("⇥", []), size: .small, tone: .secondary,
                                  title: "SD") { [weak self] in self?.switched?() }
    let note = Labels.make("", .caption)
    var height: NSLayoutConstraint!
    /// The pictures made, by file, so a file's tile made again keeps its picture.
    var made: [URL: CGImage] = [:]

    public static let side: CGFloat = 56

    override public init(frame: NSRect) {
        super.init(frame: frame)
        row.orientation = .horizontal
        row.spacing = Space.s
        row.alignment = .centerY
        row.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = row
        scroll.drawsBackground = false
        scroll.hasHorizontalScroller = false
        scroll.hasVerticalScroller = false
        scroll.verticalScrollElasticity = .none
        note.isHidden = true
        for v in [scroll, quality, note] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            addSubview(v)
        }
        height = heightAnchor.constraint(equalToConstant: Self.side)
        NSLayoutConstraint.activate([
            scroll.topAnchor.constraint(equalTo: topAnchor),
            scroll.leadingAnchor.constraint(equalTo: leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: quality.leadingAnchor, constant: -Space.s),
            scroll.heightAnchor.constraint(equalToConstant: Self.side),
            row.topAnchor.constraint(equalTo: scroll.contentView.topAnchor),
            row.bottomAnchor.constraint(equalTo: scroll.contentView.bottomAnchor),
            row.leadingAnchor.constraint(equalTo: scroll.contentView.leadingAnchor),
            quality.trailingAnchor.constraint(equalTo: trailingAnchor),
            quality.centerYAnchor.constraint(equalTo: scroll.centerYAnchor),
            note.topAnchor.constraint(equalTo: scroll.bottomAnchor, constant: Space.xs),
            note.leadingAnchor.constraint(equalTo: leadingAnchor),
            note.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor),
            height,
        ])
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel("Attached")
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// Shows `attachments`, each file's tile in its place, SD or HD while it's for something there,
    /// and `words` under them when there's something in the way.
    public func show(_ attachments: Attachments, note words: String?) {
        let files = attachments.files
        if tiles.map(\.file) != files {
            for tile in tiles {
                tile.removeFromSuperview()
            }
            tiles = files.enumerated().map { i, file in
                let tile = AttachmentTile(file: file) { [weak self] in self?.removed?(i) }
                tile.roles = roles
                if tile.square { picture(for: tile) }
                return tile
            }
            for tile in tiles {
                row.addArrangedSubview(tile)
            }
            made = made.filter { files.map(\.url).contains($0.key) }
        }
        quality.isHidden = !attachments.showsQuality
        quality.title = attachments.hd ? "HD" : "SD"
        quality.setAccessibilityValue(attachments.hd ? "HD" : "SD")
        note.stringValue = words ?? ""
        note.isHidden = words == nil
        height.constant = Self.side + (words == nil ? 0 : Space.xs + ceil(note.intrinsicContentSize.height))
        setAccessibilityValue(files.count == 1 ? files[0].name : "\(files.count) files")
    }

    /// A tile's picture: made once for its file, in the meantime its icon.
    func picture(for tile: AttachmentTile) {
        let url = tile.file.url
        if let image = made[url] { return tile.show(image) }
        thumbnail(url, CGSize(width: Self.side, height: Self.side)) { [weak self, weak tile] image in
            guard let image else { return }
            self?.made[url] = image
            tile?.show(image)
        }
    }

    func colour() {
        guard let roles else { return }
        Labels.style(note, .warn, roles: roles)
        quality.roles = roles
        for tile in tiles {
            tile.roles = roles
        }
    }

    /// Quick Look's picture of the file, made off the main thread at the screen's scale; Quick Look
    /// calls back on a queue of its own, so the handler is `@Sendable` and hops to the main queue.
    public static func quickLook(_ url: URL, _ size: CGSize, _ done: @escaping @MainActor (CGImage?) -> Void) {
        let request = QLThumbnailGenerator.Request(fileAt: url, size: size, scale: NSScreen.main?.backingScaleFactor ?? 2,
                                                   representationTypes: .thumbnail)
        QLThumbnailGenerator.shared.generateBestRepresentation(for: request) { @Sendable made, _ in
            let image = made?.cgImage
            DispatchQueue.main.async { MainActor.assumeIsolated { done(image) } }
        }
    }
}

/// One attached file in the strip: a photo's or a video's picture filling a square, its icon until
/// that's made; a file's icon and its name; and an × at its corner.
@MainActor
public final class AttachmentTile: NSView {
    public let file: AttachedFile
    /// The picture, a view under the rest so its × stays over it.
    let picture = NSView()
    /// A disc behind the × over a picture, as the pills over a message's pictures have.
    let scrim = NSView()
    let icon = NSImageView()
    let label = Labels.make("", .caption)
    lazy var close = IconButton(symbol: "xmark.circle.fill", label: "Take out \(file.name)", size: .small, tone: .primary)
    { [weak self] in self?.removed() }
    let removed: () -> Void
    var roles: Roles? {
        didSet { colour() }
    }

    public var name: String {
        file.name
    }

    /// Whether it's drawn as a picture: a photo, a video or a GIF going as what it is.
    var square: Bool {
        file.isVisual || file.kind != .document && file.type?.conforms(to: .gif) == true
    }

    init(file: AttachedFile, removed: @escaping () -> Void) {
        self.file = file
        self.removed = removed
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = Radius.small
        layer?.masksToBounds = true
        picture.wantsLayer = true
        picture.layer?.contentsGravity = .resizeAspectFill
        picture.isHidden = true
        icon.image = NSWorkspace.shared.icon(forFile: file.url.path)
        icon.imageScaling = .scaleProportionallyUpOrDown
        label.stringValue = file.name
        label.lineBreakMode = .byTruncatingMiddle
        label.isHidden = square
        let disc: CGFloat = 16
        scrim.wantsLayer = true
        scrim.layer?.cornerRadius = disc / 2
        scrim.isHidden = !square
        for v in [picture, icon, label, scrim, close] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            addSubview(v)
        }
        NSLayoutConstraint.activate([
            scrim.widthAnchor.constraint(equalToConstant: disc),
            scrim.heightAnchor.constraint(equalToConstant: disc),
            scrim.centerXAnchor.constraint(equalTo: close.centerXAnchor),
            scrim.centerYAnchor.constraint(equalTo: close.centerYAnchor),
        ])
        let side = AttachmentStrip.side
        NSLayoutConstraint.activate([
            picture.topAnchor.constraint(equalTo: topAnchor),
            picture.bottomAnchor.constraint(equalTo: bottomAnchor),
            picture.leadingAnchor.constraint(equalTo: leadingAnchor),
            picture.trailingAnchor.constraint(equalTo: trailingAnchor),
            heightAnchor.constraint(equalToConstant: side),
            widthAnchor.constraint(equalToConstant: square ? side : 168),
            icon.widthAnchor.constraint(equalToConstant: 32),
            icon.heightAnchor.constraint(equalToConstant: 32),
            icon.centerYAnchor.constraint(equalTo: centerYAnchor),
            close.topAnchor.constraint(equalTo: topAnchor, constant: Space.xxs),
            close.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.xxs),
        ] + (square ? [icon.centerXAnchor.constraint(equalTo: centerXAnchor)] : [
            icon.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.m),
            label.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: Space.s),
            label.trailingAnchor.constraint(equalTo: close.leadingAnchor, constant: -Space.xxs),
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
        ]))
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel(file.name)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ image: CGImage) {
        picture.layer?.contents = image
        picture.isHidden = false
        icon.isHidden = true
    }

    override public var wantsUpdateLayer: Bool {
        true
    }

    override public func updateLayer() {
        guard let roles else { return }
        layer?.backgroundColor = roles.surface.cgColor
        layer?.borderColor = roles.separator.cgColor
        layer?.borderWidth = Stroke.hairline
    }

    func colour() {
        guard let roles else { return }
        Labels.style(label, .primary, roles: roles)
        close.roles = roles
        scrim.layer?.backgroundColor = roles.ground.opacity(0.72).cgColor
        needsDisplay = true
    }
}
