import AppKit
@testable import JawKit
@testable import JawThemes
@testable import JawUI
import Testing

/// The chat list's rows: a badge's count or @ sits in its middle, at 1x as at 2x; and who's typing
/// says so on a row's second line, redrawing only the rows it changes.

@MainActor @Test func `a row says who's typing in the online colour in place of its draft or newest message, with no ticks`() {
    let roles = Catalog.jaw.dark
    let cell = ChatCell()
    func line(_ chat: ChatRow, _ typists: [Typist]) -> NSAttributedString {
        cell.show(chat, typists: typists, roles: roles, open: false, inCommunity: false, avatar: nil)
        return cell.line(chat, roles)
    }
    let said = line(Samples.group, [Typist(id: "person:7", name: "Rui", state: .recording)])
    #expect(said.string == "Rui recording…")
    #expect(said.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor == roles.online.nsColor, "in the online colour")
    #expect(cell.accessibilityLabel() == "Lisbon Climbing, 3 unread, mentions you, Rui recording…", "VoiceOver hears who")
    #expect(line(Samples.group, []).string == "Rui: Saturday at 9?", "the newest message back once nobody's typing")

    let ana = [Typist(id: "person:12", name: "Ana")]
    var direct = Samples.direct
    #expect(line(direct, ana).string == "typing…", "in the draft's place, naming nobody in a one-to-one chat")
    direct.draft = nil
    _ = line(direct, ana)
    #expect(cell.ticks(direct, roles) == nil, "no ticks while someone's typing")
    #expect(line(direct, []).string.hasSuffix("yes, the one at the lake") && cell.ticks(direct, roles)?.0 == 2,
            "your read message and its ticks back once nobody is")
}

@MainActor @Test func `a change of who's typing redraws only the rows whose typing it changes`() {
    let list = ChatListController(folder: "all", roles: Catalog.jaw.dark)
    list.view.frame = NSRect(x: 0, y: 0, width: 300, height: 260)
    list.apply(Update(seq: 1, upsert: Samples.chats, total: UInt64(Samples.chats.count)))
    let rows = list.all.map(\.id)
    let rui = [Typist(id: "person:7", name: "Rui")], ana = [Typist(id: "person:12", name: "Ana")]
    list.typing = [Samples.group.id: rui]
    let was = list.typing
    list.typing = [Samples.group.id: rui, Samples.direct.id: ana]
    #expect(list.rows(changedFrom: was) == IndexSet(integer: rows.firstIndex(of: Samples.direct.id)!), "Ana's row alone")
    #expect(list.rows(changedFrom: list.typing).isEmpty, "none when nothing changed")
    #expect(list.rows(changedFrom: [:]) == IndexSet([Samples.direct.id, Samples.group.id].map { rows.firstIndex(of: $0)! }))
    let cell = list.table.view(atColumn: 0, row: rows.firstIndex(of: Samples.direct.id)!, makeIfNecessary: true) as? ChatCell
    #expect(cell?.typists == ana, "and that row shows it")
}

/// Where a row's badge and the ink of its text fall, in points from the row's top-left, the badge
/// drawn by itself at `scale` in black, so its text, white in the light look, is told from it.
@MainActor
func badgeInk(_ text: String, scale: Int) throws -> (badge: CGRect, ink: CGRect) {
    let size = CGSize(width: 60, height: 56), s = CGFloat(scale)
    let ctx = try #require(CGContext(data: nil, width: Int(size.width * s), height: Int(size.height * s), bitsPerComponent: 8,
                                     bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                     bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
    ctx.scaleBy(x: s, y: s)
    ctx.translateBy(x: 0, y: size.height)
    ctx.scaleBy(x: 1, y: -1)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: true)
    ChatCell().badge(text, right: 48, fill: RGBA(0x000000), Catalog.jaw.light)
    NSGraphicsContext.restoreGraphicsState()
    let image = try #require(ctx.makeImage())
    let p = pixels(image)
    /// The bounds of what `channel` covers, its edges placed within their pixels by how much they cover.
    func bounds(_ channel: Int) throws -> CGRect {
        var rows = [Int: Double](), cols = [Int: Double]()
        for y in 0 ..< image.height {
            for x in 0 ..< image.width {
                let c = Double(p[(y * image.width + x) * 4 + channel]) / 255
                if c > 0.02 { rows[y] = max(rows[y] ?? 0, c); cols[x] = max(cols[x] ?? 0, c) }
            }
        }
        let top = try #require(rows.keys.min()), bottom = rows.keys.max()!, left = cols.keys.min()!, right = cols.keys.max()!
        let minY = Double(top) + 1 - rows[top]!, maxY = Double(bottom) + rows[bottom]!
        let minX = Double(left) + 1 - cols[left]!, maxX = Double(right) + cols[right]!
        return CGRect(x: minX / s, y: minY / s, width: (maxX - minX) / s, height: (maxY - minY) / s)
    }
    return (try bounds(3), try bounds(0))
}

@MainActor
@Test(arguments: [1, 2])
func `an unread count sits in the middle of its badge`(scale: Int) throws {
    for count in ["1", "4", "7", "12", "99"] {
        let (badge, ink) = try badgeInk(count, scale: scale)
        #expect(abs(ink.midY - badge.midY) <= 0.25, "\(count) at \(scale)x: \(ink.midY - badge.midY) pt below the middle")
    }
    let (one, _) = try badgeInk("1", scale: scale)
    #expect(one.width.rounded() == 18 && one.height.rounded() == 18, "one digit's badge is round, not \(one.size)")
    let (_, digits) = try badgeInk("12", scale: scale), (_, thousands) = try badgeInk("1,204", scale: scale)
    #expect(abs(thousands.minY - digits.minY) < 0.1, "a comma, hanging below the digits, doesn't lift them")
    let (mention, at) = try badgeInk("@", scale: scale)
    #expect(abs(at.midY - mention.midY) < 0.5, "@ at \(scale)x: \(at.midY - mention.midY) pt below the middle")
}
