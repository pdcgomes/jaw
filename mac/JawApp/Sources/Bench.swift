import AppKit
import Darwin
import JawKit
import JawUI
import QuartzCore

/// `jaw --bench <what>`: measures the app against the jawd it's pointed at, as `jaw bench` measures
/// the terminal, and prints one JSON line before it quits.
///
/// - `launch`: from the process's start, as the kernel records it, to the first frame with the
///   chat list's rows.
/// - `open`: opening each of the first 20 chats, from the open to its messages drawn, p50 and p99.
/// - `memory`: `phys_footprint` once the chat list and a chat are drawn and the app is quiet.
/// - `idle`: wake-ups over a quiet minute, once the window has settled.
@MainActor
enum Bench {
    static func run(_ what: String, model: AppModel) {
        deadline(what == "idle" ? 120 : 30)
        switch what {
        case "launch":
            model.firstRows = {
                nextFrame {
                    let start = Launch.processStart() ?? Date()
                    finish(["launch_ms": ms(Date().timeIntervalSince(start))])
                }
            }
        case "open":
            model.firstRows = {
                DispatchQueue.main.asyncAfter(deadline: .now() + 1) { opens(model, Array(model.window?.chatList.all.prefix(20) ?? []), []) }
            }
        case "memory":
            model.firstRows = {
                if let first = model.window?.chatList.all.first(where: { $0.kind != .community }) { model.open(first, opened: false) }
                DispatchQueue.main.asyncAfter(deadline: .now() + 3) {
                    finish(["footprint_mb": Double(footprint()) / 1_048_576])
                }
            }
        case "idle":
            model.firstRows = {
                DispatchQueue.main.asyncAfter(deadline: .now() + 5) {
                    let before = wakeups()
                    DispatchQueue.main.asyncAfter(deadline: .now() + 60) {
                        finish(["wakeups_per_minute": Double(wakeups() - before)])
                    }
                }
            }
        default:
            finish(["error": 1])
        }
    }

    /// Opens the first chat once the list's rows have come and settled, then draws the window into a
    /// PNG and quits.
    static func snapshot(to path: String, model: AppModel) {
        deadline(30)
        model.window?.window?.setContentSize(NSSize(width: 1440, height: 900))
        model.firstRows = {
            settled(model) {
                if let first = model.window?.chatList.all.first(where: { $0.kind != .community && $0.last != nil }) {
                    model.open(first, opened: false)
                }
                if CommandLine.arguments.contains("--split") { model.split(.across) }
                let keys = CommandLine.arguments.firstIndex(of: "--keys").flatMap { i in
                    i + 1 < CommandLine.arguments.count ? CommandLine.arguments[i + 1].split(separator: ",").map(String.init) : nil
                } ?? []
                press(keys, model: model) {
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1) { draw(to: path, model: model) }
                }
            }
        }
    }

    /// Runs `then` once the chat list's rows have stayed as they are for half a second, or after five
    /// seconds whatever they do: a fresh demo's messages and its group's name come over its first
    /// moments, so a picture taken at the first rows would catch some of them and not others.
    static func settled(_ model: AppModel, looked: Int = 0, still: Int = 0, since: [String] = [],
                        then: @escaping @MainActor () -> Void)
    {
        let rows = (model.window?.chatList.all ?? []).map { "\($0.id)@\($0.v)" }
        let quiet = rows == since ? still + 1 : 0
        if quiet >= 5 || looked >= 50 { return then() }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
            settled(model, looked: looked + 1, still: quiet, since: rows, then: then)
        }
    }

    /// Presses each shortcut, as `cmd+shift+d`, through the app's own event handling half a second
    /// apart, and says the panes after each, or a menu step's menu in their place.
    static func press(_ keys: [String], model: AppModel, then: @escaping @MainActor () -> Void) {
        guard let key = keys.first else { return then() }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
            NSApp.activate()
            if NSApp.keyWindow == nil { model.window?.window?.makeKeyAndOrderFront(nil) }
            let step = key.hasPrefix("save:") || key == "revert" || key == "settings" || key.hasPrefix("size:") || key.hasPrefix("drop:")
                || key.hasPrefix("menu:")
            if key.hasPrefix("drop:") {
                let parts = key.split(separator: ":")
                let zones: [String: GridView.DropZone] = ["middle": .middle, "left": .left, "right": .right, "top": .top, "bottom": .bottom]
                if parts.count == 3, let i = Int(parts[1]), let slot = model.focusedSlot, let zone = zones[String(parts[2])] {
                    if let row = listRow(i, model: model) {
                        model.drop(row.id, on: slot, zone)
                    } else {
                        FileHandle.standardError.write(Data("\(key): no row \(i)\n".utf8))
                    }
                }
            }
            if key.hasPrefix("save:") { model.saveLayoutAs(String(key.dropFirst(5))) }
            if key == "revert" { model.revertLayout() }
            if key == "settings" { model.window?.showSettings() }
            if key.hasPrefix("size:") {
                let wh = key.dropFirst(5).split(separator: "x").compactMap { Double($0) }
                if wh.count == 2 { model.window?.window?.setContentSize(NSSize(width: wh[0], height: wh[1])) }
            }
            var parts = key.lowercased().split(separator: "+").map(String.init)
            let char = parts.popLast() ?? ""
            var mods: NSEvent.ModifierFlags = []
            for m in parts {
                switch m {
                case "cmd": mods.insert(.command)
                case "shift": mods.insert(.shift)
                case "opt": mods.insert(.option)
                case "ctrl": mods.insert(.control)
                default: break
                }
            }
            let codes: [String: UInt16] = ["d": 2, "w": 13, "k": 40, "f": 3, "j": 38, "s": 1, "e": 14, "1": 18, "2": 19, "3": 20,
                                           "left": 123, "right": 124, "down": 125, "up": 126, "return": 36]
            let arrows: [String: Int] = ["left": NSLeftArrowFunctionKey, "right": NSRightArrowFunctionKey, "up": NSUpArrowFunctionKey,
                                         "down": NSDownArrowFunctionKey]
            if arrows[char] != nil { mods.insert([.numericPad, .function]) }
            let typed = arrows[char].map { String(UnicodeScalar($0)!) } ?? (char == "return" ? "\r" : mods.contains(.shift) ? char.uppercased() : char)
            if !step, let event = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: mods, timestamp: ProcessInfo.processInfo.systemUptime,
                                            windowNumber: (NSApp.keyWindow ?? model.window?.window)?.windowNumber ?? 0, context: nil,
                                            characters: typed, charactersIgnoringModifiers: typed, isARepeat: false, keyCode: codes[char] ?? 0)
            {
                NSApp.sendEvent(event)
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                if key.hasPrefix("menu:") {
                    FileHandle.standardError.write(Data("\(key): \(menuSaid(String(key.dropFirst(5)), model: model))\n".utf8))
                } else {
                    let palette = (model.window?.palette.isVisible ?? false) ? " · palette at \(model.window?.palette.model.index ?? -1) of \(model.window?.palette.model.items.count ?? 0)" : ""
                    let glance = model.glance.isOpen ? model.window?.window.map { w in
                        " · glance centre \(Int(model.glance.panel.frame.midX - w.frame.midX)),\(Int(model.glance.panel.frame.midY - w.frame.midY)) from the window's"
                    } ?? "" : ""
                    let said = palette + glance + (model.notice.map { " · said \"\($0.0)\"" } ?? "") + " · panes " + model.panes.values.map { "\(Int($0.view.frame.width))×\(Int($0.view.frame.height))" }.joined(separator: " ")
                    let grid = model.grid.map { g in g.reading.map { "\($0.id):\($0.chat?.prefix(6) ?? "empty")@\($0.c0)-\($0.c1)x\($0.r0)-\($0.r1)" }.joined(separator: " ") } ?? "no panes"
                    FileHandle.standardError.write(Data("\(key): \(grid) · focus \(model.focusedSlot ?? 0) · layout \(model.book.current ?? "unsaved")\(model.book.isEdited ? " •" : "")\(said)\n".utf8))
                }
                press(Array(keys.dropFirst()), model: model, then: then)
            }
        }
    }

    /// The chat list's row `i`, counting from 0, or nil when it has no such row.
    static func listRow(_ i: Int, model: AppModel) -> ChatRow? {
        guard let list = model.window?.chatList.all, list.indices.contains(i) else { return nil }
        return list[i]
    }

    /// A right-click menu's items for the snapshot's log, as `menu:list:3` (the chat list's fourth
    /// row), `menu:header` (the focused pane's) or `menu:message:5` (the sixth row of its
    /// transcript) makes it; anything else is an unknown step.
    static func menuSaid(_ what: String, model: AppModel) -> String {
        let parts = what.split(separator: ":", omittingEmptySubsequences: false).map(String.init)
        let row = parts.count == 2 ? Int(parts[1]) : nil
        switch (parts.first, row) {
        case let ("list", i?):
            guard let chat = listRow(i, model: model) else { return "no row \(i)" }
            return words(model.chatMenuItems(for: chat))
        case ("header", _) where parts.count == 1:
            guard let c = model.conversation else { return "no pane" }
            return words(model.paneMenu(for: c).items)
        case let ("message", i?):
            guard let menu = model.conversation?.menu(forRow: i) else { return "no message \(i)" }
            return words(menu.items)
        default:
            return "unknown step: menu:\(what) (the menu steps are menu:list:<row>, menu:header and menu:message:<row>)"
        }
    }

    /// Menu items as words, as the menu shows them: hidden ones left out, greyed ones in brackets, a
    /// submenu's items after it.
    static func words(_ items: [NSMenuItem]) -> String {
        items.filter { !$0.isHidden }.map { item in
            if item.isSeparatorItem { return "—" }
            let title = item.isEnabled ? item.title : "[\(item.title)]"
            guard let sub = item.submenu else { return title }
            return "\(title) ▸ (\(words(sub.items)))"
        }.joined(separator: " | ")
    }

    static func draw(to path: String, model: AppModel) {
        // The window's frame view, so the toolbar is drawn too.
        let glance = model.glance.isOpen ? model.glance.panel.contentView : EmojiPicker.shared.panel.isVisible ? EmojiPicker.shared.panel.contentView : nil
        guard let view = glance ?? model.window?.editorSheet?.contentView ?? model.window?.settings?.contentView ?? model.window?.window?.contentView?.superview else { return finish(["error": 2]) }
        let rect = view.bounds
        guard let rep = view.bitmapImageRepForCachingDisplay(in: rect) else { return finish(["error": 3]) }
        view.cacheDisplay(in: rect, to: rep)
        try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
        finish(["snapshot": 1, "panes": Double(model.panes.count)])
    }

    static func opens(_ model: AppModel, _ chats: [ChatRow], _ times: [Double]) {
        guard let chat = chats.first else {
            let sorted = times.sorted()
            func p(_ q: Double) -> Double {
                sorted.isEmpty ? 0 : sorted[min(Int(Double(sorted.count) * q), sorted.count - 1)]
            }
            finish(["opens": Double(times.count), "open_p50_ms": p(0.5), "open_p99_ms": p(0.99)])
            return
        }
        let start = Date()
        model.open(chat, opened: false)
        model.conversation?.drawn = {
            nextFrame {
                let took = ms(Date().timeIntervalSince(start))
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { opens(model, Array(chats.dropFirst()), times + [took]) }
            }
        }
    }

    /// A run that hasn't finished by then says so and quits, stopping the jawd it started.
    static func deadline(_ seconds: Double) {
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { finish(["timed_out_s": seconds]) }
    }

    static func ms(_ seconds: TimeInterval) -> Double {
        (seconds * 10000).rounded() / 10
    }

    /// Runs `then` once the next frame is on screen.
    static func nextFrame(_ then: @escaping @MainActor () -> Void) {
        CATransaction.begin()
        CATransaction.setCompletionBlock { DispatchQueue.main.async { MainActor.assumeIsolated { then() } } }
        CATransaction.commit()
    }

    static func finish(_ result: [String: Double]) {
        let line = result.sorted { $0.key < $1.key }.map { "\"\($0.key)\":\($0.value)" }.joined(separator: ",")
        print("{\(line)}")
        fflush(stdout)
        for w in NSApp.windows {
            if let sheet = w.attachedSheet { w.endSheet(sheet) }
        }
        NSApp.terminate(nil)
        // A sheet or an alert can hold termination back; the demo jawd is stopped either way.
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
            AppDelegate.shared?.model.stop()
            exit(0)
        }
    }

    /// What the process costs the system, as jaw-client's `usage` reads it.
    static func footprint() -> UInt64 {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
        let r = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count) }
        }
        return r == KERN_SUCCESS ? info.phys_footprint : 0
    }

    static func wakeups() -> UInt64 {
        var info = task_power_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_power_info_data_t>.size / MemoryLayout<natural_t>.size)
        let r = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { task_info(mach_task_self_, task_flavor_t(TASK_POWER_INFO), $0, &count) }
        }
        return r == KERN_SUCCESS ? info.task_interrupt_wakeups + info.task_platform_idle_wakeups : 0
    }
}
