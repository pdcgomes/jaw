import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The sidebar whole, the tree over the roster, Family picked and open with its people under it, drawn
/// at 2× by its own views and compared with its reference, dark and light, as LayoutTests compares the
/// conversation's. The sidebar is the same in either look, so it has a picture for each appearance. It's
/// 760 points high, so the tree's 60% holds its fifteen rows whole.

@MainActor
@Test(arguments: [true, false])
func `the sidebar draws as its reference, the tree over Family's people`(dark: Bool) async throws {
    let s = SidebarController()
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
    s.roster.typing = ["person:1"]
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
