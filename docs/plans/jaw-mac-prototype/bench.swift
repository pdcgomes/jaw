// What a native jaw client's hot paths cost, measured without a window: decoding jawd's message
// rows in Swift, measuring a message's height at a width off the main thread, and decoding jawd's
// 256 and 768 px previews. `swiftc -O -parse-as-library bench.swift -o bench && ./bench`

import AppKit
import CoreText
import Foundation
import ImageIO
import UniformTypeIdentifiers

// MARK: - The rows, as protocol/README.md has them

struct Span: Decodable { let t: String; let s: String; let url: String?; let person: String? }
struct From: Decodable { let id: String; let name: String; let colour: Int? }
struct Quote: Decodable { let id: String; let from: String?; let summary: String }
struct Reaction: Decodable { let emoji: String; let n: Int; let mine: Bool }
struct Previews: Decodable {
    let small: String?, large: String?
    enum CodingKeys: String, CodingKey { case small = "256", large = "768" }
}
struct Media: Decodable {
    let w: Int?, h: Int?, seconds: Double?, bytes: Int?, name: String?, mime: String?, hd: Bool?
    let previews: Previews?, thumbnail: String?, path: String?, state: String?, progress: Double?
}
struct AlbumItem: Decodable { let id: String; let kind: String; let media: Media }
struct Album: Decodable { let items: [AlbumItem] }
struct Voice: Decodable { let seconds: Double; let waveform: [Int]; let played: Bool? }
struct Message: Decodable {
    let id: String, v: UInt64, sort: String, chat: String, at: Int64
    let from: From?, mine: Bool, kind: String, spans: [Span], reply: Quote?, forwarded: Int
    let reactions: [Reaction], status: String?, edited: Bool, deleted: Bool, expires_at: Int64?
    let view_once: Bool, starred: Bool, media: Media?, album: Album?, voice: Voice?
}
struct Watched: Decodable { let watch: String; let seq: UInt64; let total: Int; let older: Bool; let newer: Int; let rows: [Message] }
struct Reply: Decodable { let id: Int; let result: Watched }

// MARK: - A window of made-up rows, shaped like jawd demo's

let words = ("did you get the photos from Sunday yes the one at the lake jantar amanhã às nove "
    + "levas tu o vinho ok ligo-te logo à noite praia comboio bolo obrigado até já 😂 ❤️ fotos").split(separator: " ")

func text(_ n: Int, _ seed: Int) -> String {
    (0 ..< n).map { String(words[($0 * 7 + seed * 13) % words.count]) }.joined(separator: " ")
}

func media(_ i: Int, kind: String) -> String {
    #"{"w":4032,"h":3024,"bytes":3480211,"mime":"image/jpeg","hd":\#(i % 2 == 0),"previews":{"256":"media/previews/\#(i).256.jpg","768":"media/previews/\#(i).768.jpg"},"thumbnail":"media/previews/\#(i).instant.jpg","path":"media/files/\#(i).jpg","state":"local"}"#
}

func row(_ i: Int) -> String {
    let at = 1_792_145_060_000 + i * 60_000
    let kind: String
    var extra = ""
    var spans = #"[{"t":"text","s":"\#(text(4 + (i * 37) % 60, i))"},{"t":"bold","s":"fotos"}]"#
    switch i % 20 {
    case 0, 1, 2: kind = "image"; extra = #","media":\#(media(i, kind: "image"))"#
    case 3: kind = "album"
        let items = (0 ..< 4).map { #"{"id":"3EB0\#(i)\#($0)","kind":"image","media":\#(media(i * 10 + $0, kind: "image"))}"# }
        extra = #","album":{"items":[\#(items.joined(separator: ","))]}"#
    case 4, 5: kind = "voice"; spans = "[]"
        extra = #","voice":{"seconds":14,"waveform":[\#((0 ..< 64).map { String(($0 * 37) % 101) }.joined(separator: ","))],"played":false}"#
    case 6: kind = "sticker"; spans = "[]"; extra = #","media":\#(media(i, kind: "sticker"))"#
    default: kind = "text"
    }
    let from = i % 3 == 0 ? "" : #""from":{"id":"person:\#(i % 7)","name":"Ana Ribeiro","colour":\#(i % 8)},"#
    let reply = i % 9 == 0 ? #"{"id":"3EB0A1B2C3D4E5F6","from":"Ana","summary":"did you get the photos from Sunday?"}"# : "null"
    let reactions = i % 4 == 0 ? #"[{"emoji":"❤","n":2,"mine":true},{"emoji":"😂","n":1,"mine":false}]"# : "[]"
    return #"{"id":"3EB0\#(String(format: "%018X", i))","v":\#(81_000 + i),"sort":"\#(at)~\#(String(format: "%012d", i))","chat":"120363012345678901@g.us","at":\#(at),\#(from)"mine":\#(i % 3 == 0),"kind":"\#(kind)","spans":\#(spans),"reply":\#(reply),"forwarded":0,"reactions":\#(reactions),"status":\#(i % 3 == 0 ? "\"read\"" : "null"),"edited":false,"deleted":false,"expires_at":null,"view_once":false,"starred":false\#(extra)}"#
}

func window(_ n: Int) -> Data {
    let rows = (0 ..< n).map(row).joined(separator: ",")
    return Data(#"{"id":3,"result":{"watch":"w2","seq":81233,"total":5120,"older":true,"newer":0,"rows":[\#(rows)]}}"#.utf8)
}

// MARK: - Timing

func clock(_ body: () -> Void) -> Double {
    let start = DispatchTime.now().uptimeNanoseconds
    body()
    return Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6
}

func spread(_ name: String, _ times: [Double], per: String = "") {
    let s = times.sorted()
    let at = { (q: Double) in s[Int((Double(s.count - 1) * q).rounded())] }
    print(String(format: "  %-44@ p50 %7.3f ms   p99 %7.3f ms   max %7.3f ms%@", name as NSString, at(0.5), at(0.99), s.last!, per as NSString))
}

// MARK: - Main

@main
struct Bench {
    static func main() throws {
        print("load averages: \(String(format: "%.1f %.1f %.1f", loads().0, loads().1, loads().2))")
        let decoder = JSONDecoder()

        print("\ndecoding jawd's rows with Foundation's JSONDecoder, off the main thread in the app:")
        for n in [100, 20, 1] {
            let line = window(n)
            var times: [Double] = []
            for _ in 0 ..< (n == 100 ? 1_000 : 4_000) {
                times.append(clock { _ = try! decoder.decode(Reply.self, from: line) })
            }
            spread("a \(n)-row line (\(line.count / 1024) KB)", times)
        }
        let flood = window(100)
        let rate = 100.0 / (clock { for _ in 0 ..< 200 { _ = try! decoder.decode(Reply.self, from: flood) } } / 200) * 1000
        print(String(format: "  rows a second on one core: %.0f, so 2,000 a second costs %.1f%% of a core", rate, 2000 / rate * 100))

        print("\nmeasuring a message's height at 480 pt, 13 pt system font, off the main thread:")
        let rows = try decoder.decode(Reply.self, from: window(100)).result.rows.filter { !$0.spans.isEmpty }
        let font = NSFont.systemFont(ofSize: 13), bold = NSFont.boldSystemFont(ofSize: 13)
        let strings: [NSAttributedString] = rows.map { m in
            let s = NSMutableAttributedString()
            for span in m.spans {
                s.append(NSAttributedString(string: span.s, attributes: [.font: span.t == "bold" ? bold : font]))
            }
            return s
        }
        var ct: [Double] = [], tk1: [Double] = [], tk2: [Double] = []
        for _ in 0 ..< 300 {
            ct.append(clock {
                for s in strings {
                    let setter = CTFramesetterCreateWithAttributedString(s)
                    _ = CTFramesetterSuggestFrameSizeWithConstraints(setter, CFRange(), nil, CGSize(width: 480, height: CGFloat.greatestFiniteMagnitude), nil)
                }
            })
            tk1.append(clock {
                for s in strings {
                    _ = s.boundingRect(with: NSSize(width: 480, height: CGFloat.greatestFiniteMagnitude), options: [.usesLineFragmentOrigin, .usesFontLeading])
                }
            })
            tk2.append(clock {
                for s in strings {
                    let storage = NSTextContentStorage()
                    let layout = NSTextLayoutManager()
                    storage.addTextLayoutManager(layout)
                    layout.textContainer = NSTextContainer(size: NSSize(width: 480, height: 0))
                    storage.attributedString = s
                    layout.ensureLayout(for: layout.documentRange)
                    _ = layout.usageBoundsForTextContainer
                }
            })
        }
        let k = strings.count
        spread("\(k) messages, Core Text's framesetter", ct, per: String(format: "   (%.1f µs each)", ct.sorted()[ct.count / 2] / Double(k) * 1000))
        spread("\(k) messages, NSAttributedString.boundingRect", tk1, per: String(format: "   (%.1f µs each)", tk1.sorted()[tk1.count / 2] / Double(k) * 1000))
        spread("\(k) messages, TextKit 2, a layout manager each", tk2, per: String(format: "   (%.1f µs each)", tk2.sorted()[tk2.count / 2] / Double(k) * 1000))

        print("\ndecoding jawd's previews with ImageIO into a ready bitmap, off the main thread:")
        for side in [256, 768] {
            let jpeg = photo(side: side)
            var times: [Double] = []
            for _ in 0 ..< 300 {
                times.append(clock {
                    let source = CGImageSourceCreateWithData(jpeg as CFData, nil)!
                    let image = CGImageSourceCreateImageAtIndex(source, 0, [kCGImageSourceShouldCacheImmediately: true] as CFDictionary)!
                    precondition(image.width == side)
                })
            }
            spread("a \(side) px preview (\(jpeg.count / 1024) KB JPEG)", times)
        }
        print("\nload averages: \(String(format: "%.1f %.1f %.1f", loads().0, loads().1, loads().2))")
    }

    static func loads() -> (Double, Double, Double) {
        var l = [Double](repeating: 0, count: 3)
        getloadavg(&l, 3)
        return (l[0], l[1], l[2])
    }

    /// A photo-like JPEG at `side` px on its long side: soft gradients with noise, as a beach would be.
    static func photo(side: Int) -> Data {
        let h = side * 3 / 4
        let ctx = CGContext(data: nil, width: side, height: h, bitsPerComponent: 8, bytesPerRow: 0,
                            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        let px = ctx.data!.assumingMemoryBound(to: UInt8.self)
        var seed: UInt32 = 7
        for y in 0 ..< h {
            for x in 0 ..< side {
                seed = seed &* 1_664_525 &+ 1_013_904_223
                let n = Int(seed >> 28)
                let o = y * ctx.bytesPerRow + x * 4
                px[o] = UInt8(min(255, 60 + y * 180 / h + n))
                px[o + 1] = UInt8(min(255, 120 + x * 80 / side + n))
                px[o + 2] = UInt8(min(255, 200 - y * 120 / h + n))
                px[o + 3] = 255
            }
        }
        let out = NSMutableData()
        let dest = CGImageDestinationCreateWithData(out, UTType.jpeg.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(dest, ctx.makeImage()!, [kCGImageDestinationLossyCompressionQuality: 0.8] as CFDictionary)
        CGImageDestinationFinalize(dest)
        return out as Data
    }
}
