import AppKit
import CoreServices
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
        deadline(what == "idle" ? 120 : what == "pictures" ? 150 : 30)
        switch what {
        case "pictures":
            BenchPictures.run(model)
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
                || key.hasPrefix("menu:") || key == "terminal-panes" || key.hasPrefix("vote:") || key.hasPrefix("poll") || key.hasPrefix("click:")
                || key.hasPrefix("menu-key:")
            if key.hasPrefix("click:") { click(String(key.dropFirst(6)), in: model.window?.pollSheet ?? model.window?.window) }
            if key.hasPrefix("menu-key:") { MenuKey.arm(String(key.dropFirst(9)), model: model) }
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
            if key == "terminal-panes" { model.openTerminalPanes() }
            if key.hasPrefix("vote:"), let c = model.conversation, let poll = c.newestPoll { c.vote(String(key.dropFirst(5)), in: poll.id) }
            if key.hasPrefix("poll"), let c = model.conversation {
                let form = pollForm(key, chat: c.chat.name)
                if key.hasPrefix("poll-form") {
                    model.window?.showPollForm(form, chat: c.chat.id)
                } else if let poll = form.send() {
                    model.sendPoll(chat: c.chat.id, poll, key: form.key)
                } else {
                    FileHandle.standardError.write(Data("\(key): not sent: \(form.problem ?? "")\n".utf8))
                }
            }
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
                                           "left": 123, "right": 124, "down": 125, "up": 126, "return": 36, "tab": 48, "space": 49]
            let arrows: [String: Int] = ["left": NSLeftArrowFunctionKey, "right": NSRightArrowFunctionKey, "up": NSUpArrowFunctionKey,
                                         "down": NSDownArrowFunctionKey]
            if arrows[char] != nil { mods.insert([.numericPad, .function]) }
            let typed = arrows[char].map { String(UnicodeScalar($0)!) }
                ?? (char == "return" ? "\r" : char == "tab" ? "\t" : char == "space" ? " " : mods.contains(.shift) ? char.uppercased() : char)
            if !step, let event = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: mods, timestamp: ProcessInfo.processInfo.systemUptime,
                                            windowNumber: (NSApp.keyWindow ?? model.window?.window)?.windowNumber ?? 0, context: nil,
                                            characters: typed, charactersIgnoringModifiers: typed, isARepeat: false, keyCode: codes[char] ?? 0)
            {
                NSApp.sendEvent(event)
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + (key.hasPrefix("vote:") || key.hasPrefix("poll:") ? 0.8 : 0.3)) {
                if key.hasPrefix("menu:") {
                    FileHandle.standardError.write(Data("\(key): \(menuSaid(String(key.dropFirst(5)), model: model))\n".utf8))
                } else if key.hasPrefix("vote:") || key.hasPrefix("poll:") {
                    FileHandle.standardError.write(Data("\(key): \(pollSaid(model.conversation?.newestPoll))\(model.notice.map { " · said \"\($0.0)\"" } ?? "")\n".utf8))
                } else {
                    let open = model.window.flatMap { w in w.palette.isVisible ? w.palette.model : nil }
                    let palette = open.map { p in
                        " · palette at \(p.index) of \(p.items.count)"
                            + (p.items.indices.contains(p.index) ? " (\(p.items[p.index].group): \(p.items[p.index].title.map(\.s).joined()))" : "")
                    } ?? ""
                    let glance = model.glance.isOpen ? model.window?.window.map { w in
                        " · glance centre \(Int(model.glance.panel.frame.midX - w.frame.midX)),\(Int(model.glance.panel.frame.midY - w.frame.midY)) from the window's"
                    } ?? "" : ""
                    let form = model.window?.pollForm.map { " · poll form " + formSaid($0, sheet: model.window?.pollSheet) } ?? ""
                    let selected = model.conversation?.selectedRow.map {
                        " · selected \($0.kind.rawValue) \"\($0.plain.prefix(24).replacingOccurrences(of: "\n", with: " "))\""
                    } ?? ""
                    let looking = QuickLook.shared.items.isEmpty ? "" : " · looking at " + QuickLook.shared.items.map { item in
                        (item.previewItemTitle ?? "") + handedOver(item.previewItemURL)
                    }.joined(separator: ", ")
                    let said = palette + glance + form + selected + looking + (model.notice.map { " · said \"\($0.0)\"" } ?? "") + " · panes " + model.panes.values.map { "\(Int($0.view.frame.width))×\(Int($0.view.frame.height))" }.joined(separator: " ")
                    let grid = model.grid.map { g in g.reading.map { "\($0.id):\($0.chat?.prefix(6) ?? "empty")@\($0.c0)-\($0.c1)x\($0.r0)-\($0.r1)" }.joined(separator: " ") } ?? "no panes"
                    FileHandle.standardError.write(Data("\(key): \(grid) · focus \(model.focusedSlot ?? 0) · layout \(model.book.current ?? "unsaved")\(model.book.isEdited ? " •" : "")\(said)\n".utf8))
                }
                press(Array(keys.dropFirst()), model: model, then: then)
            }
        }
    }

    /// How a clone handed to another app stands, for the snapshot's log: its mode and who
    /// quarantined it as what.
    static func handedOver(_ url: URL?) -> String {
        guard let url, let mode = (try? FileManager.default.attributesOfItem(atPath: url.path))?[.posixPermissions] as? Int else { return "" }
        let quarantine = (try? url.resourceValues(forKeys: [.quarantinePropertiesKey]))?.quarantineProperties
        let type = (quarantine?[kLSQuarantineTypeKey as String] as? String).map { $0.replacingOccurrences(of: "LSQuarantineType", with: "") }
        let agent = quarantine?[kLSQuarantineAgentNameKey as String] as? String
        return " (\(String(mode, radix: 8)), \(type.map { "quarantined by \(agent ?? "?") as \($0)" } ?? "not quarantined"))"
    }

    /// The chat list's row `i`, counting from 0, or nil when it has no such row.
    static func listRow(_ i: Int, model: AppModel) -> ChatRow? {
        guard let list = model.window?.chatList.all, list.indices.contains(i) else { return nil }
        return list[i]
    }

    /// A menu's items for the snapshot's log: a right-click menu, as `menu:list:3` (the chat list's
    /// fourth row), `menu:header` (the focused pane's) or `menu:message:5` (the sixth row of its
    /// transcript) makes it, or one of the menu bar's, as `menu:bar:Message`, as it opens; anything
    /// else is an unknown step.
    static func menuSaid(_ what: String, model: AppModel) -> String {
        let parts = what.split(separator: ":", omittingEmptySubsequences: false).map(String.init)
        let row = parts.count == 2 ? Int(parts[1]) : nil
        if parts.count == 2, parts[0] == "bar" {
            guard let menu = NSApp.mainMenu?.item(withTitle: parts[1])?.submenu else { return "no \(parts[1]) menu" }
            menu.update()
            return words(menu.items)
        }
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
            return "unknown step: menu:\(what) (the menu steps are menu:list:<row>, menu:header, menu:message:<row> and menu:bar:<menu>)"
        }
    }

    /// The poll a `poll:` or `poll-form:` step writes, as `poll:Jantar?|Sábado|Domingo|several` has it:
    /// its question, its options, and whether it allows several answers.
    static func pollForm(_ key: String, chat: String) -> PollForm {
        let written = key.split(separator: ":", maxSplits: 1).dropFirst().first.map(String.init) ?? ""
        var parts = written.split(separator: "|", omittingEmptySubsequences: false).map(String.init)
        let several = parts.count > 1 && parts.last == "several"
        if several { parts.removeLast() }
        return PollForm(chat: chat, question: parts.first ?? "", options: Array(parts.dropFirst()), several: several)
    }

    /// A key for the next menu that pops up, as `menu-key:2` arms it before the step that opens one.
    /// A menu that's open holds the main queue until it closes, so the key is posted to the app's
    /// event queue from a timer once the menu tracks, as a key pressed on the keyboard reaches an open
    /// menu, and Esc a second after it if the menu is still open; the log says the menu's items, how
    /// it closed, and the focused pane's newest poll after. `menu-key:t.return` presses each in turn.
    @MainActor
    final class MenuKey: NSObject {
        let key: String
        let model: AppModel
        var escaped = false
        var open = false
        /// Those armed, kept until their menu closes, since the notifications don't keep them.
        static var armed: [MenuKey] = []

        init(key: String, model: AppModel) {
            self.key = key
            self.model = model
        }

        static func arm(_ key: String, model: AppModel) {
            let armed = MenuKey(key: key, model: model)
            Self.armed.append(armed)
            let center = NotificationCenter.default
            center.addObserver(armed, selector: #selector(began(_:)), name: NSMenu.didBeginTrackingNotification, object: nil)
            center.addObserver(armed, selector: #selector(ended(_:)), name: NSMenu.didEndTrackingNotification, object: nil)
        }

        @objc func began(_ note: Notification) {
            guard !open else { return }
            open = true
            say("a menu opened: \(Bench.words((note.object as? NSMenu)?.items ?? []))")
            for (i, pressed) in key.split(separator: ".").enumerated() {
                later(0.4 + 0.3 * Double(i)) { Self.post(String(pressed)) }
            }
            later(1.4 + 0.3 * Double(key.split(separator: ".").count)) { [self] in
                guard open else { return }
                escaped = true
                Self.post("escape")
            }
        }

        @objc func ended(_: Notification) {
            guard open else { return }
            open = false
            NotificationCenter.default.removeObserver(self)
            Self.armed.removeAll { $0 === self }
            say(escaped ? "the menu was still open, and Esc closed it" : "the menu closed")
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { [self] in say("then \(Bench.pollSaid(model.conversation?.newestPoll))") }
        }

        func say(_ what: String) {
            FileHandle.standardError.write(Data("menu-key:\(key): \(what)\n".utf8))
        }

        /// Runs `then` after `seconds` from the run loop in every mode, a menu's tracking included.
        func later(_ seconds: TimeInterval, _ then: @escaping @MainActor () -> Void) {
            RunLoop.main.add(Timer(timeInterval: seconds, repeats: false) { _ in MainActor.assumeIsolated(then) }, forMode: .common)
        }

        /// A key down and up, as `2`, `t`, `down`, `return` or `escape`, posted to the app's event queue.
        static func post(_ key: String) {
            let codes: [String: UInt16] = ["1": 18, "2": 19, "3": 20, "4": 21, "5": 23, "6": 22, "7": 26, "8": 28, "9": 25, "escape": 53,
                                           "return": 36, "down": 125, "up": 126, "t": 17, "p": 35, "r": 15]
            let named = ["escape": "\u{1b}", "return": "\r", "down": String(UnicodeScalar(NSDownArrowFunctionKey)!),
                         "up": String(UnicodeScalar(NSUpArrowFunctionKey)!)]
            let typed = named[key] ?? key
            let flags: NSEvent.ModifierFlags = key == "down" || key == "up" ? [.numericPad, .function] : []
            for type in [NSEvent.EventType.keyDown, .keyUp] {
                if let event = NSEvent.keyEvent(with: type, location: .zero, modifierFlags: flags, timestamp: ProcessInfo.processInfo.systemUptime,
                                                windowNumber: 0, context: nil, characters: typed, charactersIgnoringModifiers: typed,
                                                isARepeat: false, keyCode: codes[key] ?? 0)
                {
                    NSApp.postEvent(event, atStart: false)
                }
            }
        }
    }

    /// A click, as `click:120:240` has it: the left button down and up at that point in the key
    /// window, or in `window` when the app isn't active and has none, in points from its content's
    /// top left, through the app's own event handling.
    static func click(_ at: String, in window: NSWindow?) {
        let xy = at.split(separator: ":").compactMap { Double($0) }
        guard xy.count == 2, let window = NSApp.keyWindow ?? window, let content = window.contentView else {
            return FileHandle.standardError.write(Data("click:\(at): no point, or no window\n".utf8))
        }
        let point = NSPoint(x: content.frame.minX + xy[0], y: content.frame.maxY - xy[1])
        for (type, pressure) in [(NSEvent.EventType.leftMouseDown, Float(1)), (.leftMouseUp, 0)] {
            if let event = NSEvent.mouseEvent(with: type, location: point, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                              windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: pressure)
            {
                NSApp.sendEvent(event)
            }
        }
    }

    /// The poll form for the snapshot's log: its question, each option with its number, the field
    /// with the keys as the form has it (an option by its number), and what has them in the key
    /// window: the text of the field being edited, or the responder.
    static func formSaid(_ form: PollForm, sheet: NSWindow?) -> String {
        let focus = switch form.focus {
        case .question?: "the question"
        case let .option(i)?: "option \(i)"
        case nil: "none"
        }
        let window = NSApp.keyWindow, responder = window?.firstResponder
        let keys = window == nil ? "no key window" : (window === sheet ? "the sheet's " : "another window's ")
            + ((responder as? NSTextView).map { "field, editing \"\($0.string)\"" } ?? responder.map { "\(type(of: $0))" } ?? "nothing")
        return ([form.question] + zip(form.ids, form.options).map { "\($0):\($1)" }).joined(separator: " | ") + " · keys in \(focus), \(keys)"
    }

    /// A poll as its row is, for the snapshot's log: its question, each option's tally with yours
    /// ticked, how many voted, and its state when it's yours.
    static func pollSaid(_ m: MessageRow?) -> String {
        guard let m, let poll = m.poll else { return "no poll" }
        let options = poll.options.map { "\($0.name) \($0.votes)\($0.mine ? " ✓" : "")" }
        let status = m.mine ? m.status.map { " · \($0.rawValue)" } ?? "" : ""
        return ([poll.question] + options + ["\(poll.voters) voted"]).joined(separator: " · ") + status
    }

    /// Menu items as words, as the menu shows them: hidden ones left out, greyed ones in brackets, a
    /// submenu's items after it, made as it would be on opening.
    static func words(_ items: [NSMenuItem]) -> String {
        items.filter { !$0.isHidden }.map { item in
            if item.isSeparatorItem { return "—" }
            let title = item.isEnabled ? item.title : "[\(item.title)]"
            guard let sub = item.submenu else { return title }
            sub.delegate?.menuNeedsUpdate?(sub)
            if sub.autoenablesItems { sub.update() }
            return "\(title) ▸ (\(words(sub.items)))"
        }.joined(separator: " | ")
    }

    static func draw(to path: String, model: AppModel) {
        // The window's frame view, so the toolbar is drawn too.
        let glance = model.glance.isOpen ? model.glance.panel.contentView : EmojiPicker.shared.panel.isVisible ? EmojiPicker.shared.panel.contentView : nil
        guard let view = glance ?? model.window?.editorSheet?.contentView ?? model.window?.pollSheet?.contentView ?? model.window?.settings?.contentView
            ?? model.window?.window?.contentView?.superview else { return finish(["error": 2]) }
        let rect = view.bounds
        guard let rep = view.bitmapImageRepForCachingDisplay(in: rect) else { return finish(["error": 3]) }
        view.cacheDisplay(in: rect, to: rep)
        try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
        finish(["snapshot": 1, "panes": Double(model.panes.count), "typing": Double(model.typing.count)])
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
