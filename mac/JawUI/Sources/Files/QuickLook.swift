import AppKit
import Quartz

/// What Quick Look shows: the clones of a message's files, each titled by its name, an album's
/// paged through from the one asked for. The window controller that has the keys hands the panel
/// over when it asks, as `QLPreviewPanel` asks the responder chain.
@MainActor
public final class QuickLook: NSObject {
    public static let shared = QuickLook()

    public final class Item: NSObject, QLPreviewItem, @unchecked Sendable {
        public let previewItemURL: URL!
        public let previewItemTitle: String!

        init(_ url: URL, _ title: String) {
            previewItemURL = url
            previewItemTitle = title
        }
    }

    public private(set) var items: [Item] = []
    public private(set) var at = 0

    /// Shows `files`, each with its title, from `at`, in the panel or in place of what it shows.
    public func show(_ files: [(URL, String)], at: Int = 0) {
        items = files.map { Item($0.0, $0.1) }
        self.at = min(max(at, 0), max(items.count - 1, 0))
        guard !items.isEmpty, let panel = QLPreviewPanel.shared() else { return }
        if panel.isVisible, panel.dataSource === self {
            panel.reloadData()
            panel.currentPreviewItemIndex = self.at
        } else {
            panel.makeKeyAndOrderFront(nil)
        }
    }

    public func begin(_ panel: QLPreviewPanel) {
        panel.dataSource = self
        panel.delegate = self
        panel.currentPreviewItemIndex = at
    }

    public func end(_ panel: QLPreviewPanel) {
        panel.dataSource = nil
        panel.delegate = nil
    }
}

extension QuickLook: @preconcurrency QLPreviewPanelDataSource, QLPreviewPanelDelegate {
    public func numberOfPreviewItems(in _: QLPreviewPanel!) -> Int {
        items.count
    }

    public func previewPanel(_: QLPreviewPanel!, previewItemAt index: Int) -> (any QLPreviewItem)! {
        items.indices.contains(index) ? items[index] : nil
    }
}
