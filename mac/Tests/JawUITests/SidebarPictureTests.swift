import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The sidebar whole, the tree over the roster, Family picked and open with its people under it, Inês
/// selected among them, drawn at 2× by its own views and compared with its reference, dark and light,
/// as LayoutTests compares the conversation's. The sidebar is the same in either look, so it has a
/// picture for each appearance. It's 760 points high, so the tree's 60% holds its fifteen rows whole.
/// A picture can't draw the selection's material, which comes out black, so its rows draw the selection
/// as the flat grey AppKit gives a list without the keys, as the window on the build Mac never has them.

/// A row that draws its selection as a flat fill of the grey a source list's selection takes without
/// the keys, where AppKit puts the selection's material: a source list's row has a visual effect
/// view for it, never calling `drawSelection(in:)`, so that view is hidden and the row fills its place.
final class FlatSelection: SidebarRowView {
    override func didAddSubview(_ subview: NSView) {
        super.didAddSubview(subview)
        if subview is NSVisualEffectView { subview.isHidden = true }
    }

    override func layout() {
        super.layout()
        for case let material as NSVisualEffectView in subviews {
            material.isHidden = true
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard isSelected else { return }
        NSColor.unemphasizedSelectedContentBackgroundColor.setFill()
        NSBezierPath(roundedRect: bounds.insetBy(dx: 10, dy: 0), xRadius: 5, yRadius: 5).fill()
    }
}

@MainActor
@Test(arguments: [true, false])
func `the sidebar draws as its reference, the tree over Family's people`(dark: Bool) async throws {
    let s = SidebarController()
    s.makeRow = { FlatSelection() }
    s.roster.makeRow = { FlatSelection() }
    s.roles = Catalog.jaw.roles(dark: dark)
    s.roster.now = { Samples.now }
    let window = NSWindow(contentRect: NSRect(x: -30000, y: -30000, width: 220, height: 760), styleMask: [.borderless],
                          backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
    window.contentView = s.view
    defer { window.close() }
    s.apply(Samples.folderUpdate)
    s.expand("folder:1")
    s.select("folder:1")
    s.roster.show(folder: "folder:1")
    s.roster.apply(Update(seq: 1, upsert: familyPeople, total: UInt64(familyPeople.count)))
    s.roster.typing = ["person:1": .typing]
    #expect(s.roster.said[3] == "Inês Gomes 2 h ago")
    s.roster.table.selectRowIndexes([3], byExtendingSelection: false)
    s.view.layoutSubtreeIfNeeded()
    try await Task.sleep(for: .milliseconds(100))
    let bounds = s.view.bounds
    let rep = try #require(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(bounds.width * 2), pixelsHigh: Int(bounds.height * 2),
                                            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                            bytesPerRow: Int(bounds.width * 2) * 4, bitsPerPixel: 32))
    rep.size = bounds.size
    s.view.cacheDisplay(in: bounds, to: rep)
    try compare(try #require(rep.cgImage), "sidebar-\(dark ? "dark" : "light")")
}
