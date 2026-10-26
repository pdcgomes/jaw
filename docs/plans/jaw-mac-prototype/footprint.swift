// What an AppKit window hosting SwiftUI costs in memory before jaw draws anything in it: a
// view-based table of 100 text rows laid out and a SwiftUI view beside it, never put on screen.
// `swiftc -O -parse-as-library footprint.swift -o footprint && ./footprint`

import AppKit
import SwiftUI

func footprint() -> Double {
    var info = task_vm_info_data_t()
    var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<integer_t>.size)
    let kr = withUnsafeMutablePointer(to: &info) {
        $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count) }
    }
    return kr == KERN_SUCCESS ? Double(info.phys_footprint) / 1_048_576 : -1
}

final class Rows: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    func numberOfRows(in tableView: NSTableView) -> Int { 10_000 }
    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let id = NSUserInterfaceItemIdentifier("chat")
        let cell = (tableView.makeView(withIdentifier: id, owner: nil) as? NSTableCellView) ?? {
            let c = NSTableCellView()
            c.identifier = id
            let t = NSTextField(labelWithString: "")
            c.textField = t
            c.addSubview(t)
            t.frame = NSRect(x: 8, y: 4, width: 300, height: 40)
            return c
        }()
        cell.textField?.stringValue = "Ana Ribeiro · did you get the photos from Sunday? \(row)"
        return cell
    }
}

struct Chrome: View {
    var body: some View {
        VStack(alignment: .leading) {
            Text("Lisbon Climbing").font(.headline)
            Text("Marta: Saturday at 9?").foregroundStyle(.secondary)
        }.padding()
    }
}

@main
struct Footprint {
    @MainActor static func main() {
        let start = footprint()
        let app = NSApplication.shared
        app.setActivationPolicy(.prohibited)
        let loaded = footprint()
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800), styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        let rows = Rows()
        let table = NSTableView()
        table.addTableColumn(NSTableColumn(identifier: .init("c")))
        table.rowHeight = 48
        table.dataSource = rows
        table.delegate = rows
        let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: 360, height: 800))
        scroll.documentView = table
        let host = NSHostingView(rootView: Chrome())
        host.frame = NSRect(x: 360, y: 0, width: 840, height: 800)
        let content = NSView(frame: window.contentRect(forFrameRect: window.frame))
        content.addSubview(scroll)
        content.addSubview(host)
        window.contentView = content
        table.reloadData()
        content.layoutSubtreeIfNeeded()
        table.scrollRowToVisible(5_000)
        content.layoutSubtreeIfNeeded()
        _ = window.contentView?.bitmapImageRepForCachingDisplay(in: content.bounds).map { rep in
            content.cacheDisplay(in: content.bounds, to: rep)
        }
        let drawn = footprint()
        print(String(format: "phys_footprint: %.1f MB at start, %.1f MB with AppKit and SwiftUI loaded, %.1f MB with a 1200x800 window of a table and a SwiftUI view drawn (offscreen)", start, loaded, drawn))
        var l = [Double](repeating: 0, count: 3)
        getloadavg(&l, 3)
        print(String(format: "load averages: %.1f %.1f %.1f", l[0], l[1], l[2]))
    }
}
