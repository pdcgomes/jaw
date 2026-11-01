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

@MainActor @Test func `a cell asks for its pictures once, and lets them go when it's reused`() async {
    let pictures = MessagePictures(files: .memory(["a.768.jpg": jpeg(768, 576), "a.256.jpg": jpeg(256, 192), "a.jpg": jpeg(96, 72)]))
    let cell = MessageView(frame: CGRect(x: 0, y: 0, width: 640, height: 300))
    cell.pictures = pictures
    let l = laid(photo(4032, 3024))
    cell.laid = l
    cell.laid = l
    cell.prepareForReuse()
    _ = await eventually { !pictures.queue.busy }
    let counts = pictures.queue.counts
    #expect(counts.decoded + counts.dropped == 2, "its thumbnail and its preview, asked once each: \(counts)")
}

@MainActor @Test func `a preview decoded before the cell first draws is just there, and one coming after its thumbnail fades in`() async {
    let pictures = MessagePictures(files: .memory(["a.768.jpg": jpeg(768, 576), "a.256.jpg": jpeg(256, 192), "a.jpg": jpeg(96, 72)]), mode: .here)
    let cell = MessageView(frame: CGRect(x: 0, y: 0, width: 640, height: 300))
    cell.wantsLayer = true
    cell.pictures = pictures
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
