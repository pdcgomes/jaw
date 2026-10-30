import AppKit
@testable import JawKit
@testable import JawThemes
@testable import JawUI
import Testing

/// The chat list's rows: a badge's count or @ sits in its middle, at 1x as at 2x; and who's typing
/// says so on a row's second line, redrawing only the rows it changes.

@MainActor @Test func `a row says who's typing in the online colour in place of its draft or newest message, with no ticks`() {
    let roles = Catalog.jaw.dark
    let cell = ChatCell(frame: NSRect(x: 0, y: 0, width: 300, height: 56))
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

@MainActor @Test func `a selected row says who's typing in the selection's green, which reads on it`() {
    let light = Catalog.jaw.light
    let cell = ChatCell(frame: NSRect(x: 0, y: 0, width: 300, height: 56))
    cell.show(Samples.group, typists: [Typist(id: "person:7", name: "Rui")], roles: light, open: false, inCommunity: false, avatar: nil)
    func colour() -> NSColor? {
        cell.line(Samples.group, light).attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor
    }
    #expect(colour() == light.online.nsColor)
    cell.isSelected = true
    #expect(colour() == light.selectedOnline.nsColor && light.selectedOnline != light.online, "a darker green on the selection")
}

@MainActor @Test func `a busy group's row cuts its names short at 300 points, and always says typing`() throws {
    let roles = Catalog.jaw.dark
    let list = ChatListController(folder: "all", roles: roles)
    let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 260), styleMask: [.borderless], backing: .buffered, defer: true)
    w.contentView = list.view
    var busy = Samples.busyGroup
    busy.unread = 12
    busy.mentions = 1
    list.apply(Update(seq: 1, upsert: [busy, Samples.direct], total: 2))
    list.typing = [busy.id: Samples.busy]
    list.view.layoutSubtreeIfNeeded()
    let row = try #require(list.all.firstIndex { $0.id == busy.id })
    func drawn() throws -> (ChatCell, NSAttributedString, NSRect) {
        let cell = try #require(list.table.view(atColumn: 0, row: row, makeIfNecessary: true) as? ChatCell)
        return (cell, cell.line(busy, roles), cell.lineRect(busy, roles))
    }
    let (cell, said, room) = try drawn()
    #expect(cell.bounds.width >= 280 && room.width < 200, "a 300-point row, its count and its @ beside the line")
    #expect(said.string.hasPrefix("Ana Margarida") && said.string.hasSuffix("… typing…"), "\(said.string)")
    #expect(said.size().width <= room.width, "\(said.size().width) in \(room.width) points, so nothing cuts its end")
    #expect(said.attribute(.foregroundColor, at: said.length - 1, effectiveRange: nil) as? NSColor == roles.online.nsColor)
    #expect(cell.accessibilityLabel() == "Família Gomes, 12 unread, mentions you, \(Samples.busy.map(\.name).joined(separator: ", ")) typing…",
            "VoiceOver hears every name")
    list.typing = [busy.id: Samples.busy.map { Typist(id: $0.id, name: $0.name, state: .recording) }]
    let (_, recording, _) = try drawn()
    #expect(recording.string.hasSuffix("… recording…") && recording.size().width <= room.width, "\(recording.string)")
}

@MainActor @Test func `a change of who's typing redraws only the rows whose typing it changes`() throws {
    let list = ChatListController(folder: "all", roles: Catalog.jaw.dark)
    let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 260), styleMask: [.borderless], backing: .buffered, defer: true)
    w.contentView = list.view
    list.apply(Update(seq: 1, upsert: Samples.chats, total: UInt64(Samples.chats.count)))
    let rui = [Typist(id: "person:7", name: "Rui")], ana = [Typist(id: "person:12", name: "Ana")]
    list.typing = [Samples.group.id: rui]
    list.view.layoutSubtreeIfNeeded()
    let rows = list.all.map(\.id)
    let group = try #require(rows.firstIndex(of: Samples.group.id)), direct = try #require(rows.firstIndex(of: Samples.direct.id))
    /// Each row's cell as the table holds it once laid out, none made for the asking, and how many
    /// times each was shown a chat.
    func drawn() throws -> [(cell: ChatCell, shows: Int)] {
        list.view.layoutSubtreeIfNeeded()
        return try rows.indices.map { i in
            let cell = try #require(list.table.view(atColumn: 0, row: i, makeIfNecessary: false) as? ChatCell, "row \(i) has its cell")
            return (cell, cell.shows)
        }
    }
    let before = try drawn()
    #expect(before[group].cell.typists == rui && before[direct].cell.typists.isEmpty)

    list.typing = [Samples.group.id: rui, Samples.direct.id: ana]
    let after = try drawn()
    #expect(after[direct].cell.typists == ana, "Ana's row shows her typing")
    for i in rows.indices where i != direct {
        #expect(after[i].cell === before[i].cell && after[i].shows == before[i].shows, "\(rows[i])'s row isn't shown again")
    }

    list.typing = [Samples.group.id: rui, Samples.direct.id: ana]
    let same = try drawn()
    #expect(zip(same, after).allSatisfy { $0.cell === $1.cell && $0.shows == $1.shows }, "no row is shown again when nothing changed")

    list.typing = [:]
    let none = try drawn()
    #expect(none[group].cell.typists.isEmpty && none[direct].cell.typists.isEmpty, "both rows say nobody's typing")
    for i in rows.indices where i != direct && i != group {
        #expect(none[i].cell === same[i].cell && none[i].shows == same[i].shows, "\(rows[i])'s row isn't shown again")
    }
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

@MainActor
@Test func `an empty folder's list says so once its window has come, and not while it comes or once it has chats`() {
    let list = ChatListController(folder: "folder:1", roles: Catalog.jaw.roles(dark: true))
    _ = list.view
    list.emptyLine = Samples.tree.emptyLine("folder:1")
    #expect(list.emptyLabel.isHidden, "nothing said before the window comes")
    list.apply(Update(seq: 1, upsert: [], total: 0))
    #expect(!list.emptyLabel.isHidden && list.emptyLabel.stringValue == "No chats in this folder.")
    list.apply(Update(seq: 2, upsert: [Samples.direct], total: 1))
    #expect(list.emptyLabel.isHidden)
}
