import AppKit
import CoreGraphics
@testable import JawKit
@testable import JawUI
import Testing

/// Pictures in their places: each kind's size from the row before any pixel, albums laid out by how
/// many they hold, a sticker with no bubble, a cell asking for its pictures once and letting them go
/// when it's reused, and the chat of pictures drawn in both looks, dark and light.

func laid(_ m: MessageRow, look: Look = .bubbles, width: CGFloat = 640, group: Bool = false) -> Laid {
    let s = spec(look, dark: true)
    let context = MessageContext(group: group, firstOfRun: true, showTime: true, day: nil)
    return MessageLayout.lay(m, width: width, context: context, style: s.style, fonts: s.fonts, me: s.me)
}

/// A photo of `w` by `h` pixels, here with its previews.
func photo(_ w: Int?, _ h: Int?, kind: MessageKind = .image) -> MessageRow {
    var m = MessageRow(id: "PH", v: 1, sort: "1", chat: "c", at: Samples.now, from: Sender(id: "person:4", name: "Ana", colour: 4),
                       kind: kind, spans: [])
    m.media = Media(w: w, h: h, previews: ["256": "a.256.jpg", "768": "a.768.jpg"], thumbnail: "a.jpg", path: "a")
    return m
}

func picture(_ m: MessageRow, look: Look = .bubbles, width: CGFloat = 640) -> CGSize? {
    laid(m, look: look, width: width).pictures.first?.rect.size
}

@Test func `a photo keeps its shape inside 320 by 400 points, 120 at least, from its size alone`() {
    #expect(picture(photo(4032, 3024)) == CGSize(width: 320, height: 240))
    #expect(picture(photo(3024, 4032)) == CGSize(width: 300, height: 400))
    #expect(picture(photo(4000, 1500)) == CGSize(width: 320, height: 120))
    #expect(picture(photo(1000, 4000)) == CGSize(width: 120, height: 400), "cropped to 120 wide rather than thinner")
    #expect(picture(photo(8000, 400)) == CGSize(width: 320, height: 120), "and to 120 tall")
    #expect(picture(photo(2000, 2000)) == CGSize(width: 320, height: 320))
    #expect(picture(photo(nil, nil)) == CGSize(width: 320, height: 240), "a photo with no size is drawn 4:3")
    #expect(picture(photo(4032, 3024), width: 260)!.width < 320, "narrower in a narrow pane, still in its shape")
    #expect(picture(photo(4032, 3024), look: .lines) == CGSize(width: 320, height: 240), "the same in lines")
}

@Test func `a preview coming changes no height, and every picture is a place a click lands on`() {
    var bare = photo(4032, 3024)
    bare.media = Media(w: 4032, h: 3024)
    for look in Look.allCases {
        let here = laid(photo(4032, 3024), look: look), coming = laid(bare, look: look)
        #expect(here.height == coming.height, "\(look)")
        #expect(here.pictures.count == 1 && coming.pictures.isEmpty, "nothing to draw from yet: its well alone")
        #expect(here.regions.contains { $0.1 == .media(item: nil) && $0.0 == here.pictures[0].rect })
    }
}

@Test func `a sticker is 160 points on its own, no bubble, and a video message a 192-point circle`() {
    let sticker = laid(photo(512, 512, kind: .sticker))
    let slot = try! #require(sticker.pictures.first)
    #expect(slot.rect.size == CGSize(width: 160, height: 160) && slot.fit == .fit)
    #expect(sticker.body == slot.rect, "the sticker is its own body: no bubble around it")
    #expect(picture(photo(512, 256, kind: .sticker)) == CGSize(width: 160, height: 80), "inside 160, in its shape")
    let note = laid(photo(640, 640, kind: .videoNote))
    #expect(note.pictures.first?.rect.size == CGSize(width: 192, height: 192) && note.pictures.first?.radius == 96)
    let poster = laid(photo(1080, 1920, kind: .video))
    #expect(poster.pictures.first?.rect.size == CGSize(width: 225, height: 400))
}

@Test func `your own sticker or video message that didn't go says so with Retry, on its own as in a bubble`() {
    for kind in [MessageKind.sticker, .videoNote] {
        for look in Look.allCases {
            var m = photo(512, 512, kind: kind)
            m.mine = true
            m.status = .failed
            let failed = laid(m, look: look)
            #expect(failed.regions.contains { $0.1 == .retry }, "\(kind) in \(look)")
            m.status = .sent
            #expect(laid(m, look: look).height < failed.height, "\(kind) in \(look): the line's own room")
        }
    }
}

func album(_ n: Int) -> MessageRow {
    var m = MessageRow(id: "AL", v: 1, sort: "1", chat: "c", at: Samples.now, kind: .album, spans: [])
    m.album = Album(items: (0 ..< n).map { Album.Item(id: "i\($0)", kind: $0 == 2 ? "video" : "image",
                                                      media: Media(w: 1000, h: 800, previews: ["256": "\($0).256", "768": "\($0).768"])) })
    return m
}

@Test func `albums lay out as the design draws them: two across, then one wide, then two by two with how many more`() {
    let two = laid(album(2)).pictures.map(\.rect)
    #expect(two.count == 2 && two[0].size == CGSize(width: 159, height: 159) && two[1].minX == two[0].maxX + 2 && two[0].minY == two[1].minY)
    let three = laid(album(3)).pictures.map(\.rect)
    #expect(three.count == 3 && three[2].width == 320 && three[2].minY == three[0].maxY + 2)
    let four = laid(album(4)).pictures.map(\.rect)
    #expect(four.count == 4 && four[3].minY == four[1].maxY + 2 && four[3].minX == four[1].minX)
    let eight = laid(album(8))
    #expect(eight.pictures.count == 4 && eight.pictures.map(\.item) == [0, 1, 2, 3], "the first four, the fourth saying +5")
    #expect(eight.regions.filter { if case .media = $0.1 { true } else { false } }.count == 4)
    #expect(laid(album(4)).height == eight.height, "five or more take what four do")
    #expect(eight.pictures[0].keys(2).previews.first?.path == "0.768", "a tile 159 points across at 2x takes the 768")
    #expect(eight.pictures[0].keys(1).previews.first?.path == "0.256", "and at 1x the 256")
}

@Test func `an album media in's jawd sends as counts is a line of them, its items drawn as the messages they come as`() {
    var counted = MessageRow(id: "AL2", v: 1, sort: "1", chat: "c", at: Samples.now, kind: .album, spans: [])
    counted.album = Album(items: [], photos: 3, videos: 1)
    let l = laid(counted)
    #expect(l.pictures.isEmpty)
    #expect(MessageLayout.Builder.kindLine(counted) == "Album · 3 photos and 1 video")
    #expect(MessageLayout.Builder.albumLine(Album(items: [], photos: 1)) == "Album · 1 photo")
    #expect(MessageLayout.Builder.albumLine(album(5).album) == "Album · 4 photos and 1 video")
}

/// The photo's pictures, behind four slow decodes that keep every worker busy, so what a cell asks
/// for waits, and letting it go can drop it before it starts.
@MainActor
func busyPictures() -> MessagePictures {
    var files = ["a.768.jpg": jpeg(768, 576), "a.256.jpg": jpeg(256, 192), "a.jpg": jpeg(96, 72)]
    let noise = noisy(2400, 1800)
    for i in 0 ..< PictureQueue.most {
        files["n\(i)"] = noise
    }
    let pictures = MessagePictures(files: .memory(files))
    let busy = pictures.token()
    for i in 0 ..< PictureQueue.most {
        pictures.queue.ask(key("n\(i)", 2400, 1800), for: busy) { _ in }
    }
    return pictures
}

/// A cell in its row in a window of its own, as the transcript's are; the window is kept until the
/// test ends, as a cell leaving its window lets go of its pictures too.
@MainActor
func shownCell(_ pictures: MessagePictures) -> (MessageView, NSTableRowView, NSWindow) {
    let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 640, height: 300), styleMask: .borderless, backing: .buffered, defer: true)
    let row = NSTableRowView(frame: CGRect(x: 0, y: 0, width: 640, height: 300))
    let cell = MessageView(frame: row.bounds)
    cell.pictures = pictures
    window.contentView?.addSubview(row)
    row.addSubview(cell)
    return (cell, row, window)
}

@MainActor @Test func `a cell asks for its pictures once, and lets them go when it's reused or its row leaves the transcript`() async {
    let l = laid(photo(4032, 3024))
    let pictures = busyPictures()
    let (cell, _, window) = shownCell(pictures)
    cell.laid = l
    cell.laid = l
    #expect(pictures.queue.counts.asked == PictureQueue.most + 2, "its thumbnail and its preview, asked once each")
    cell.prepareForReuse()
    #expect(await eventually(seconds: 20) { !pictures.queue.busy })
    #expect(pictures.queue.counts.dropped == 2 && pictures.queue.counts.decoded == PictureQueue.most,
            "let go before they started, so never decoded nor handed over: \(pictures.queue.counts)")

    let more = busyPictures()
    let c = ConversationController(chat: Samples.direct, spec: spec(.bubbles, dark: true), layouts: Layouts(), place: .newest(10))
    c.pictures = more
    let (gone, row, other) = shownCell(more)
    gone.laid = l
    #expect(more.queue.counts.asked == PictureQueue.most + 2)
    c.tableView(c.table, didRemove: row, forRow: 0)
    #expect(await eventually(seconds: 20) { !more.queue.busy })
    #expect(more.queue.counts.dropped == 2 && more.queue.counts.decoded == PictureQueue.most,
            "its row out of the transcript, what it asked for is let go: \(more.queue.counts)")
    #expect(gone.window === other && cell.window === window, "each still in its window, so neither let go by leaving it")
}

@MainActor @Test func `a preview decoded before the cell first draws is just there, and one coming after its thumbnail fades in`() async {
    let pictures = MessagePictures(files: .memory(["a.768.jpg": jpeg(768, 576), "a.256.jpg": jpeg(256, 192), "a.jpg": jpeg(96, 72)]), mode: .here)
    let (cell, _, window) = shownCell(pictures)
    cell.laid = laid(photo(4032, 3024))
    #expect(!cell.drewOnce && !cell.pictureCame(), "decoded before it was drawn: no fade")
    #expect(pictures.drawable(cell.laid!.pictures[0], scale: cell.scale)?.isPreview == true)
    cell.drewOnce = true
    cell.drawn = [0: 2]
    #expect(!cell.pictureCame(), "drawn from its preview already: no fade")
    cell.drawn = [0: 1]
    #expect(cell.pictureCame(), "a preview taking its thumbnail's place fades in")
    cell.drawn = [:]
    #expect(cell.pictureCame(), "and one taking its well's")
    #expect(cell.window === window)
}

@MainActor @Test func `a new cell asks for its pictures once, as it comes into its window, and at a new scale only having let go of the old`() async {
    let l = laid(photo(4032, 3024))
    let pictures = busyPictures()
    let cell = MessageView(frame: CGRect(x: 0, y: 0, width: 640, height: 300))
    cell.pictures = pictures
    cell.laid = l
    #expect(pictures.queue.counts.asked == PictureQueue.most, "nothing asked before it's in a window, whose scale says its pixels")
    let window = NSWindow(contentRect: cell.frame, styleMask: .borderless, backing: .buffered, defer: true)
    let row = NSTableRowView(frame: cell.frame)
    window.contentView?.addSubview(row)
    row.addSubview(cell)
    #expect(pictures.queue.counts.asked == PictureQueue.most + 2, "asked once as it comes into its window: \(pictures.queue.counts)")
    cell.viewDidChangeBackingProperties()
    #expect(pictures.queue.counts.asked == PictureQueue.most + 2, "nothing again at the scale it asked at")
    cell.askedScale = cell.scale + 1
    cell.viewDidChangeBackingProperties()
    #expect(pictures.queue.counts.asked == PictureQueue.most + 4 && pictures.queue.counts.dropped == 2,
            "on a display of another scale, what it asked for at the old one is let go first: \(pictures.queue.counts)")
    #expect(cell.window === window)
}

@Test func `the chat of pictures draws as its references, in both looks, dark and light`() throws {
    for look in Look.allCases {
        for dark in [true, false] {
            let name = "pictures-\(look.rawValue)-\(dark ? "dark" : "light")"
            let image = try #require(render(Samples.pictures, spec(look, dark: dark), group: false, typing: false))
            try compare(image, name)
        }
    }
}

/// A sample of `Samples.downloadStates` by its id.
func state(_ id: String) -> MessageRow {
    Samples.downloadStates.first { $0.id == id }!
}

func rings(_ l: Laid) -> [CGFloat] {
    (l.parts + l.over).compactMap { if case let .ring(_, _, share, _, _) = $0 { share } else { nil } }
}

func stops(_ l: Laid) -> [Region] {
    l.regions.map(\.1).filter { if case .stop = $0 { true } else { false } }
}

@Test func `every state says what it is and offers what can be done, as the protocol has them`() {
    #expect(Samples.downloadStates.count == 15)
    // Remote, coming, failed, waiting, expired, gone: each drawn from its thumbnail, or its well.
    let remote = laid(state("D1")), coming = laid(state("D2")), failed = laid(state("D3"))
    #expect(remote.pictures.count == 1 && rings(remote).isEmpty && stops(remote).isEmpty)
    #expect(rings(coming) == [1, 0.4] && stops(coming) == [.stop(item: nil)], "a track and its share, and × to stop it")
    #expect(coming.regions.first?.1 == .stop(item: nil), "the ring is found before the picture under it")
    #expect(MessageLayout.Builder.comingWords(state("D3").media!)! == ("the download failed: the network connection was lost", "Try again"))
    #expect(MessageLayout.Builder.comingWords(state("D4").media!)! == ("Waiting for your phone", nil))
    #expect(MessageLayout.Builder.comingWords(state("D5").media!)!.act == "Ask the phone again")
    #expect(MessageLayout.Builder.comingWords(state("D6").media!)! == ("your phone no longer has this file", "Ask the phone again"))
    #expect(MessageLayout.Builder.comingWords(state("D5").media!, short: true)! == (nil, "Ask the phone again"), "a tile says what's done, not why")
    #expect(MessageLayout.Builder.comingWords(state("D2").media!) == nil && MessageLayout.Builder.comingWords(state("D1").media!) == nil)
    var sameFailed = state("D1"), sameComing = state("D1")
    sameFailed.media?.error = "the download failed"
    sameComing.media?.state = .downloading
    #expect(laid(sameFailed).height == remote.height && laid(sameComing).height == remote.height, "a state changes no height")
    #expect(failed.pictures.count == 1)
    #expect(laid(state("D6")).pictures.isEmpty, "gone with no thumbnail: its well")
    for id in ["D1", "D3", "D4", "D5", "D6"] {
        #expect(laid(state(id)).regions.contains { $0.1 == .media(item: nil) }, "\(id): a click asks for it")
    }
}

@Test func `a video shows ↓ in its ▶'s place until it's here, and a sticker and an album's tiles their own states`() {
    let remote = laid(state("D7")), coming = laid(state("D8"))
    #expect(rings(remote).isEmpty && rings(coming) == [1, 0.7])
    let sticker = laid(state("D9"))
    #expect(rings(sticker) == [1, 0.2] && sticker.body == sticker.pictures.first?.rect, "on its own, coming")
    let album = laid(state("D10"))
    #expect(album.pictures.count == 4)
    #expect(rings(album) == [1, 0.5] && stops(album) == [.stop(item: 1)], "the second tile coming, stopped by itself")
    #expect(album.regions.filter { if case .media = $0.1 { true } else { false } }.map(\.1) == (0 ..< 4).map { .media(item: $0) })
}

@Test func `a document says its state at its right, and why it isn't here in its second line`() {
    let here = laid(state("D15")), remote = laid(state("D11")), coming = laid(state("D12")), expired = laid(state("D13"))
    #expect(here.height == remote.height && remote.height == expired.height)
    #expect(rings(coming) == [1, 0.3] && stops(coming) == [.stop(item: nil)])
    #expect(rings(here).isEmpty && stops(here).isEmpty && stops(remote).isEmpty)
    for l in [here, remote, coming, expired] {
        #expect(l.regions.contains { $0.1 == .media(item: nil) }, "the whole card looks or asks")
    }
}

@Test func `a progress push lays the message out again, and its ring follows it`() {
    var row = state("D2")
    let layouts = Layouts()
    let context = MessageContext(group: false, firstOfRun: true, showTime: true, day: nil)
    let s = spec(.bubbles, dark: true)
    let before = layouts.laid(row, width: 640, spec: s, context: context)
    row.v += 1
    row.media?.progress = 0.8
    let after = layouts.laid(row, width: 640, spec: s, context: context)
    #expect(before !== after && rings(after) == [1, 0.8])
}

@Test func `each download state draws as its references, in both looks, dark and light`() throws {
    for look in Look.allCases {
        for dark in [true, false] {
            let name = "download-states-\(look.rawValue)-\(dark ? "dark" : "light")"
            let image = try #require(render(Samples.downloadStates, spec(look, dark: dark), group: false, typing: false))
            try compare(image, name)
        }
    }
}
