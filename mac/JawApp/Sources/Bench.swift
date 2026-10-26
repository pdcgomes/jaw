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

    /// Opens the first chat once the list's rows come, then draws the window into a PNG and quits.
    static func snapshot(to path: String, model: AppModel) {
        deadline(30)
        model.firstRows = {
            if let first = model.window?.chatList.all.first(where: { $0.kind != .community && $0.last != nil }) {
                model.open(first, opened: false)
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                guard let view = model.window?.window?.contentView else { return finish(["error": 2]) }
                let rect = view.bounds
                guard let rep = view.bitmapImageRepForCachingDisplay(in: rect) else { return finish(["error": 3]) }
                view.cacheDisplay(in: rect, to: rep)
                try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
                finish(["snapshot": 1])
            }
        }
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
        NSApp.terminate(nil)
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
