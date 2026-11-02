import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The grid outliving its pane, as the bench's did as a fling ended: its last scroll left rows to be
/// made at the next layout, and the pane was let go of while its view was still in a window.
@MainActor @Suite struct LibraryGridOutlivingItsPane {
    @Test func `a grid whose pane has gone makes the rows it scrolled to empty, rather than asking no one for them`() {
        let w = NSWindow(contentRect: CGRect(x: -30000, y: -30000, width: 560, height: 700), styleMask: .borderless, backing: .buffered,
                         defer: false)
        w.isReleasedWhenClosed = false
        defer { w.close() }
        weak var gone: LibraryController?
        let collection: NSCollectionView = autoreleasepool {
            let c = LibraryController(scope: .every, style: libraryStyle(), kind: .photo)
            c.pictures = MessagePictures(files: .memory(SamplePictures.files), mode: .here)
            w.contentView = c.view
            w.orderFront(nil)
            w.layoutIfNeeded()
            c.apply(Samples.libraryUpdate(of: photos(300), kind: .photo))
            w.layoutIfNeeded()
            c.scrollGrid(to: 3000)
            gone = c
            return c.collection
        }
        #expect(gone == nil, "the pane let go of, its view still in the window")
        w.layoutIfNeeded()
        collection.layoutSubtreeIfNeeded()
        let shown = collection.visibleItems().compactMap { ($0 as? LibraryGridItem)?.cell }
        #expect(!shown.isEmpty && shown.allSatisfy { $0.element == nil }, "the rows scrolled to made, with nothing to show")
    }
}
