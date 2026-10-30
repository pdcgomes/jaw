import AppKit
@testable import JawKit
import JawThemes
@testable import JawUI
import QuartzCore
import Testing

/// Someone typing: the bubble of three dots after the newest message, as WhatsApp draws it, and the
/// header's "typing" with its dots counting under a shimmer; when they play, and the bubble's row,
/// which comes and goes keeping your place.

@MainActor @Test func `it plays only where it's looked at, in a key window on screen, with motion allowed`() {
    #expect(LiveStateView.plays(allowed: true, key: true, visible: true, reduced: false))
    for (allowed, key, visible, reduced) in [(false, true, true, false), (true, false, true, false), (true, true, false, false),
                                             (true, true, true, true)]
    {
        #expect(!LiveStateView.plays(allowed: allowed, key: key, visible: visible, reduced: reduced))
    }
    let dots = TypingDotsView()
    dots.allowed = true
    #expect(!dots.playing, "out of a window it stays still")
}

@MainActor @Test func `the bubble's dots rise and brighten one after another, keeping time from when it began`() throws {
    let dots = TypingDotsView()
    dots.since = 1000
    dots.play()
    #expect(dots.playing)
    for (i, d) in dots.dotLayers.enumerated() {
        let brighten = try #require(d.animation(forKey: "brighten") as? CAKeyframeAnimation)
        let rise = try #require(d.animation(forKey: "rise") as? CAKeyframeAnimation)
        for a in [brighten, rise] {
            #expect(a.keyTimes == [0, 0.25, 0.5, 1] && a.timingFunctions?.count == 3)
            #expect(a.duration == TypingBubble.round && a.repeatCount == .infinity)
            #expect(a.beginTime == d.convertTime(1000, from: nil) + TypingBubble.lag * Double(i), "a fifth of a second behind the one before")
        }
        #expect((brighten.values as? [Float]) == [TypingBubble.dim, 1, TypingBubble.dim, TypingBubble.dim])
        #expect((rise.values as? [CGFloat]) == [0, -TypingBubble.rise, 0, 0], "up, then back")
        #expect(!d.isHidden)
    }
    dots.stop()
    #expect(!dots.playing && dots.dotLayers.allSatisfy { $0.isHidden && $0.animationKeys() == nil })
}

@MainActor @Test func `still, the bubble has its dots in their messages' colour, and lines the dots alone`() throws {
    let roles = Catalog.jaw.dark
    func drawn(_ look: Look) throws -> (Int, Int) -> RGBA? {
        let dots = TypingDotsView()
        dots.style = Style(roles: roles, look: look)
        let rep = try #require(dots.bitmapImageRepForCachingDisplay(in: dots.bounds))
        dots.cacheDisplay(in: dots.bounds, to: rep)
        let k = CGFloat(rep.pixelsWide) / dots.bounds.width
        return { x, y in
            guard let c = rep.colorAt(x: Int(CGFloat(x) * k), y: Int(CGFloat(y) * k))?.usingColorSpace(.sRGB), c.alphaComponent > 0.5
            else { return nil }
            return RGBA(r: c.redComponent, g: c.greenComponent, b: c.blueComponent)
        }
    }
    /// Within two steps a channel, as a snapshot's colour conversion rounds.
    func near(_ a: RGBA?, _ b: RGBA) -> Bool {
        guard let a else { return false }
        return max(abs(a.r - b.r), abs(a.g - b.g), abs(a.b - b.b)) <= 2.0 / 255
    }
    let bubble = try drawn(.bubbles)
    #expect(near(bubble(6, 15), roles.surface), "the bubble")
    for i in 0 ..< 3 {
        let r = TypingBubble.dotRect(i, look: .bubbles)
        #expect(near(bubble(Int(r.midX), Int(r.midY)), roles.secondary), "dot \(i)")
    }
    let lines = try drawn(.lines)
    #expect(lines(50, 15) == nil, "no bubble in lines")
    #expect(near(lines(Int(TypingBubble.dotRect(0, look: .lines).midX), 15), roles.secondary), "the first dot at the margin")
}

@MainActor @Test func `playing, the dots' layers sit where the still dots are drawn`() throws {
    let dots = TypingDotsView()
    dots.style = Style(roles: Catalog.jaw.dark, look: .lines)
    let size = dots.bounds.size
    func bitmap(_ draw: (CGContext) -> Void) -> CGImage? {
        guard let ctx = CGContext(data: nil, width: Int(size.width) * 2, height: Int(size.height) * 2, bitsPerComponent: 8,
                                  bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        ctx.scaleBy(x: 2, y: 2)
        draw(ctx)
        return ctx.makeImage()
    }
    let drawn = try #require(bitmap { ctx in
        ctx.translateBy(x: 0, y: size.height)
        ctx.scaleBy(x: 1, y: -1)
        TypingBubble.draw(in: ctx, at: .zero, style: dots.style)
    })
    dots.play()
    let backing = try #require(dots.layer)
    backing.contents = nil
    for d in dots.dotLayers {
        d.opacity = 1
    }
    // Under a layer whose y runs up, as a window's does, since a layer's flip is how it sits in another.
    let window = CALayer()
    window.frame = dots.bounds
    window.addSublayer(backing)
    let layers = try #require(bitmap { ctx in window.render(in: ctx) })
    // Each dot's edge, at 2x, may be smoothed differently; a dot a point out of place differs by more.
    let edges = 3 * Int((TypingBubble.dot * 2 * .pi).rounded(.up))
    #expect(differing(pixels(drawn), pixels(layers)) <= edges, "the dots where they're drawn, but for their edges' smoothing")
}

@MainActor @Test func `still, the header's typing has all three dots and no shimmer, and keeps room for them`() {
    let label = TypingLabel()
    label.rest = " · muted"
    #expect(!label.playing && label.layer?.mask == nil && label.dotLayers.allSatisfy(\.isHidden))
    #expect(label.intrinsicContentSize.width == ceil(label.width("typing... · muted")), "the dots' room, whatever the count")
    #expect(label.dotX(0) == label.width("typing") && label.dotX(3) == label.width("typing..."))
}

@MainActor @Test func `playing, the header's dots count up under a shimmer, keeping time from when they began`() throws {
    let label = TypingLabel()
    label.words = "someone's typing"
    label.since = 500
    label.play()
    let counts: [[Float]] = [[1, 1, 1], [0, 1, 1], [0, 0, 1]]
    for (i, d) in label.dotLayers.enumerated() {
        #expect(d.frame.minX == label.dotX(i) && !d.isHidden && d.contents != nil, "dot \(i) after the words")
        let count = try #require(d.animation(forKey: "count") as? CAKeyframeAnimation)
        #expect(count.calculationMode == .discrete && count.keyTimes?.count == 4 && (count.values as? [Float]) == counts[i],
                "\".\", \"..\", \"...\"")
        #expect(count.duration == 3 * TypingLabel.step && count.repeatCount == .infinity)
        #expect(count.beginTime == d.convertTime(500, from: nil))
    }
    let shimmer = try #require(label.layer?.mask as? CAGradientLayer)
    #expect(shimmer === label.shimmer && (shimmer.colors as? [CGColor])?.map(\.alpha) == [TypingLabel.shade, 1, TypingLabel.shade])
    let sweep = try #require(shimmer.animation(forKey: "sweep") as? CAKeyframeAnimation)
    #expect(sweep.values?.count == 3 && sweep.duration == TypingLabel.sweep + TypingLabel.pause && sweep.repeatCount == .infinity)
    #expect(sweep.beginTime == shimmer.convertTime(500, from: nil))
    label.stop()
    #expect(!label.playing && label.layer?.mask == nil && label.dotLayers.allSatisfy { $0.isHidden && $0.animationKeys() == nil })
}

// MARK: - In a conversation

/// A host that says whether the pane is the one you're reading, and counts clicks that focus it.
@MainActor
final class Reader: ConversationHost {
    var reading = true
    var focused = 0

    func conversation(_: ConversationController, move _: Place) {}
    func conversation(_: ConversationController, send _: String, key _: String, reply _: String?, mentions _: [String]) {}
    func conversation(_: ConversationController, react _: String, emoji _: String) {}
    func conversation(_: ConversationController, edit _: String, text _: String) {}
    func conversation(_: ConversationController, delete _: String, everyone _: Bool) {}
    func conversation(_: ConversationController, info _: String) {}
    func conversation(_: ConversationController, retry _: String) {}
    func conversation(_: ConversationController, read _: String) {}
    func conversation(_: ConversationController, typing _: TypingState) {}
    func conversation(_: ConversationController, draft _: String) {}
    func conversationWantsHistory(_: ConversationController) {}
    func conversation(_: ConversationController, forward _: MessageRow) {}
    func conversation(_: ConversationController, reactTo _: MessageRow) {}
    func conversation(_: ConversationController, open _: String) {}
    func conversation(_: ConversationController, vote _: String, options _: [String]) {}

    func conversationFocused(_: ConversationController) {
        focused += 1
    }

    func conversationIsRead(_: ConversationController) -> Bool {
        reading
    }
}

/// The group's samples in a pane 640 by 480 in a window of its own, laid out and drawn.
@MainActor
func pane(typing: [String], host: Reader? = nil) async -> (ConversationController, NSWindow) {
    var chat = Samples.group
    chat.typing = typing
    chat.unread = 0
    let c = ConversationController(chat: chat, spec: spec(.bubbles, dark: true), layouts: Layouts(), place: .newest(100))
    c.host = host
    let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 640, height: 480), styleMask: [.borderless], backing: .buffered,
                     defer: true)
    w.contentView = c.view
    c.view.layoutSubtreeIfNeeded()
    await applied(c, Samples.update)
    return (c, w)
}

@MainActor
func applied(_ c: ConversationController, _ update: Update<MessageRow>) async {
    await withCheckedContinuation { (k: CheckedContinuation<Void, Never>) in
        c.drawn = { k.resume() }
        c.apply(update)
    }
    c.view.layoutSubtreeIfNeeded()
}

/// Runs with Reduce Motion on, so the row comes and goes at once.
@MainActor
func stillMotion(_ body: () async throws -> Void) async rethrows {
    let before = Motion.reduced
    Motion.reduced = { true }
    defer { Motion.reduced = before }
    try await body()
}

@MainActor @Test(.timeLimit(.minutes(1))) func `the typing bubble waits after the newest message while someone types, and goes when they stop`() async throws {
    try await stillMotion {
        let reader = Reader()
        let (c, w) = await pane(typing: ["person:7"], host: reader)
        #expect(c.showsTyping && c.table.numberOfRows == c.rows.count + 1)
        #expect(c.table.view(atColumn: 0, row: c.rows.count, makeIfNecessary: true) === c.typingRow)
        #expect(c.isAtBottom, "the chat opens at the bottom, the bubble in view")
        #expect(c.typingRow.accessibilityLabel() == "someone's typing…")
        #expect(c.typingRow.dots.allowed && !c.typingRow.dots.playing, "allowed, but still in a window that isn't key")
        reader.reading = false
        c.lookingChanged()
        #expect(!c.typingRow.dots.allowed, "not while another pane is the one you're reading")
        c.select(c.rows.last?.id)
        let click = try #require(NSEvent.mouseEvent(with: .leftMouseDown, location: .zero, modifierFlags: [], timestamp: 0,
                                                    windowNumber: w.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                                                    pressure: 1))
        c.typingRow.mouseDown(with: click)
        #expect(reader.focused == 1 && c.selected == c.rows.last?.id, "a click gives the pane the keys and selects nothing")
        c.typing = []
        #expect(!c.showsTyping && c.table.numberOfRows == c.rows.count && c.isAtBottom, "gone, the bottom kept in view")
        c.typing = ["person:7"]
        #expect(c.showsTyping && c.isAtBottom, "back, and still at the bottom")
    }
}

@MainActor @Test(.timeLimit(.minutes(1))) func `the header says who's typing with its dots counting, and goes back to its line when they stop`() async {
    await stillMotion {
        let reader = Reader()
        let (c, _) = await pane(typing: ["person:7"], host: reader)
        let header = c.header
        #expect(!header.typingLabel.isHidden && header.detail.isHidden && header.typingLabel.words == "someone's typing")
        #expect(header.accessibilityLabel()?.contains("someone's typing…") == true, "VoiceOver hears it as before")
        #expect(header.typingLabel.allowed, "let play in the pane you're reading")
        reader.reading = false
        c.lookingChanged()
        #expect(!header.typingLabel.allowed)
        reader.reading = true
        c.typing = []
        #expect(header.typingLabel.isHidden && !header.detail.isHidden && header.detail.stringValue == "group")
        #expect(!header.typingLabel.allowed, "nothing to play once nobody's typing")
    }
}

@MainActor @Test(.timeLimit(.minutes(1))) func `reading further up, the bubble coming and going moves nothing`() async throws {
    try await stillMotion {
        let reader = Reader()
        let (c, _) = await pane(typing: [], host: reader)
        c.scroll(to: 0)
        let before = try #require(c.currentAnchor())
        #expect(!c.isAtBottom)
        c.typing = ["person:7"]
        #expect(c.showsTyping)
        let during = try #require(c.currentAnchor())
        #expect(during.id == before.id && during.offset == before.offset)
        #expect(!c.typingRow.dots.allowed, "off screen it isn't let play")
        c.typing = []
        let after = try #require(c.currentAnchor())
        #expect(!c.showsTyping && after.id == before.id && after.offset == before.offset)
    }
}

@MainActor @Test(.timeLimit(.minutes(1))) func `their message lands where the bubble was, and someone else's leaves it`() async {
    await stillMotion {
        let (c, _) = await pane(typing: ["person:7"])
        let count = c.rows.count
        // After the samples' cards, the newest, whose sorts start with their times, 1792159300000 to 1792159307000.
        var ana = Samples.m(98, [Span(.text, "I'm in")], at: Samples.now + 30000, from: Samples.ana)
        ana.sort = "9~1"
        await applied(c, Update(seq: 300, upsert: [ana], total: UInt64(count + 1), newer: 0))
        #expect(c.rows.last?.id == ana.id && c.typing == ["person:7"] && c.showsTyping, "Ana's message leaves Rui typing")
        var rui = Samples.m(99, [Span(.text, "Saturday at 9, then")], at: Samples.now + 60000, from: Samples.rui)
        rui.sort = "9~2"
        await applied(c, Update(seq: 301, upsert: [rui], total: UInt64(count + 2), newer: 0))
        #expect(c.rows.last?.id == rui.id && c.typing.isEmpty, "Rui's message ends his typing")
        #expect(!c.showsTyping && c.table.numberOfRows == c.rows.count && c.isAtBottom)
    }
}

@MainActor @Test(.timeLimit(.minutes(1))) func `no bubble away from the newest messages, nor in an empty chat`() async throws {
    try await stillMotion {
        let (c, _) = await pane(typing: ["person:7"])
        let first = try #require(c.rows.first)
        await applied(c, Update(seq: 300, older: true, newer: 40, moved: Moved(place: .around(first.id, before: 100, after: 100), newer: 40)))
        #expect(!c.window.holdsNewest && !c.showsTyping && c.table.numberOfRows == c.rows.count)

        var chat = Samples.group
        chat.unread = 0
        let empty = ConversationController(chat: chat, spec: spec(.bubbles, dark: true), layouts: Layouts(), place: .newest(100))
        _ = empty.view
        empty.apply(Update(seq: 1, total: 0, older: false, newer: 0, moved: Moved(place: .newest(100), newer: 0)))
        try await Task.sleep(for: .milliseconds(100))
        #expect(!empty.typing.isEmpty && !empty.wantsTyping && !empty.showsTyping && empty.table.numberOfRows == 0)
    }
}
