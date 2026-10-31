import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import SwiftUI
import Testing

/// Who's typing as it's drawn, compared with reference pictures: a pane's header naming them and the
/// bubble after the newest message, in bubbles and lines, dark and light; and the chat list's rows
/// saying who, a busy group's names cut short, and the status line short of room, dark and light,
/// which are the same in either look.

/// `view` drawn at 2x, as the harness draws its pictures whatever the screen's scale, over `ground`,
/// which views drawn into a picture leave out.
@MainActor
func picture(_ view: NSView, on ground: RGBA) throws -> CGImage {
    let bounds = view.bounds
    let wide = Int((bounds.width * 2).rounded()), high = Int((bounds.height * 2).rounded())
    let rep = try #require(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: wide, pixelsHigh: high, bitsPerSample: 8,
                                           samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                           bytesPerRow: wide * 4, bitsPerPixel: 32))
    rep.size = bounds.size
    view.cacheDisplay(in: bounds, to: rep)
    let drawn = try #require(rep.cgImage)
    let ctx = try #require(CGContext(data: nil, width: wide, height: high, bitsPerComponent: 8, bytesPerRow: 0,
                                     space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                     bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
    let all = CGRect(x: 0, y: 0, width: wide, height: high)
    ctx.setFillColor(ground.cgColor)
    ctx.fill(all)
    ctx.draw(drawn, in: all)
    return try #require(ctx.makeImage())
}

/// Runs as of the samples' now, so the days and times drawn don't depend on when it runs.
@MainActor
func asOfSamples(_ body: () async throws -> Void) async rethrows {
    let before = Format.currentTime
    Format.currentTime = { Samples.now }
    defer { Format.currentTime = before }
    try await body()
}

@MainActor @Test(.timeLimit(.minutes(2))) func `a pane draws who's typing in its header and the bubble after the newest message, in both looks, dark and light`() async throws {
    try await stillMotion {
        try await asOfSamples {
            for look in Look.allCases {
                for dark in [true, false] {
                    let layout = spec(look, dark: dark)
                    let (c, _) = await pane(typing: [anaTyping, ruiRecording], layout: layout, width: 480, height: 300)
                    c.scroll.verticalScroller?.isHidden = true
                    #expect(c.showsTyping && c.isAtBottom && c.header.typingLabel.phrase == "Ana, Rui typing")
                    try compare(try picture(c.view, on: layout.style.roles.ground), "typing-pane-\(look.rawValue)-\(dark ? "dark" : "light")")
                }
            }
        }
    }
}

@MainActor @Test(.timeLimit(.minutes(1))) func `the chat list's rows say who's typing in the online colour, a busy group's names cut short, dark and light`() async throws {
    try await asOfSamples {
        for dark in [true, false] {
            let roles = Catalog.jaw.roles(dark: dark)
            let list = ChatListController(folder: "all", roles: roles)
            let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 300), styleMask: [.borderless], backing: .buffered,
                             defer: true)
            w.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
            w.contentView = list.view
            let chats = Samples.chats + [Samples.busyGroup]
            list.apply(Update(seq: 1, upsert: chats, total: UInt64(chats.count)))
            list.typing = [Samples.direct.id: [Typist(id: "person:12", name: "Ana")], Samples.group.id: [ruiRecording],
                           Samples.busyGroup.id: Samples.busy]
            list.select(Samples.direct.id)
            list.view.layoutSubtreeIfNeeded()
            list.scroll.verticalScroller?.isHidden = true
            try compare(try picture(list.view, on: roles.surface), "typing-list-\(dark ? "dark" : "light")")
        }
    }
}

@MainActor @Test(.timeLimit(.minutes(1))) func `the status line short of room cuts a busy group's names, and keeps what they're doing and muted, dark and light`() async throws {
    for dark in [true, false] {
        let roles = Catalog.jaw.roles(dark: dark)
        let host = NSHostingView(rootView: StatusLine(state: nil, warn: false, who: Samples.busy.map(\.name).joined(separator: ", "),
                                                      typing: "typing…", chat: "muted", roles: roles))
        let w = NSWindow(contentRect: NSRect(x: -30000, y: -30000, width: 300, height: 24), styleMask: [.borderless], backing: .buffered,
                         defer: false)
        w.isReleasedWhenClosed = false
        w.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        w.contentView = host
        w.orderFront(nil)
        defer { w.close() }
        try await Task.sleep(for: .milliseconds(100))
        host.layoutSubtreeIfNeeded()
        let image = try picture(host, on: roles.surface)
        try compare(image, "typing-status-\(dark ? "dark" : "light")")
        let p = pixels(image)
        /// Pixels drawn in `colour`, within a few steps a channel, as text's smoothing leaves its strokes' middles.
        func inked(_ colour: RGBA) -> Int {
            stride(from: 0, to: p.count, by: 4).count { i in
                max(abs(Double(p[i]) / 255 - colour.r), abs(Double(p[i + 1]) / 255 - colour.g), abs(Double(p[i + 2]) / 255 - colour.b)) <= 6.0 / 255
            }
        }
        #expect(inked(roles.online) > 40, "the names and the words, in the online colour")
        #expect(inked(roles.secondary) > 20, "\"· muted\" after the words, which a line cut at its end would lose first")
    }
}
