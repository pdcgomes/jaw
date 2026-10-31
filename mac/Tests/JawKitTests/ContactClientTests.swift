import Foundation
@testable import JawKit
import Testing

/// Contact cards through the client, against a fake jawd on a real socket: sent with their key and
/// back as jawd's row, asked again with that key after a lost answer, handed back whole when jawd
/// turns them down; and a row whose card is as long as a line jawd sends carries, read on the
/// client's thread as the row arrives, so the main thread never reads a vCard.

let marta = SendContact.Card(name: "Marta Lopes", vcard: "BEGIN:VCARD\r\nVERSION:3.0\r\nN:Lopes;Marta;;;\r\nFN:Marta Lopes\r\n"
    + "TEL;TYPE=CELL;waid=351213456789:+351 213 456 789\r\nEND:VCARD\r\n")

/// A contact row as jawd sends one, in Ana's chat, named `name`, of `vcard`.
func contactRow(_ id: String, name: String, vcard: String, mine: Bool = true) -> String {
    let row: JSONValue = .object([
        "id": .string(id), "v": .number(7), "sort": .string("9~1"), "chat": .string("ana"), "at": .number(1), "mine": .bool(mine),
        "kind": .string("contact"), "spans": .array([.object(["t": .string("text"), "s": .string(name)])]),
        "status": mine ? .string("pending") : .null, "contact": .object(["name": .string(name), "vcard": .string(vcard)]),
    ])
    return (try? JSONEncoder().encode(row)).map { String(decoding: $0, as: UTF8.self) } ?? "null"
}

/// A script that answers `send.contact` as jawd does: the message it made, and its row, its cards
/// one after another, named for one card or for how many there are.
func carding(_ method: String, _ params: JSONValue, _ id: UInt64) -> [String] {
    guard method == "send.contact" else { return ordinary(method, params, id) }
    let key = params["key"]?.string ?? ""
    let cards = params["cards"]?.array ?? []
    let name = cards.count == 1 ? cards[0]["name"]?.string ?? "" : "\(cards.count) contacts"
    let message = "3EB0\(key.suffix(4))"
    let row = contactRow(message, name: name, vcard: cards.compactMap { $0["vcard"]?.string }.joined())
    return ["{\"id\":\(id),\"result\":{\"message\":\"\(message)\",\"row\":\(row)}}"]
}

/// A card named `name` whose vCard is `bytes` bytes long in UTF-8, filled with `filler`, a letter of
/// one or two bytes, and as many `x` as make up the rest.
func sized(_ bytes: Int, name: String = "Ana", filler: String = "x") -> SendContact.Card {
    let begin = "BEGIN:VCARD\r\nFN:", end = "\r\nEND:VCARD\r\n"
    let room = bytes - begin.utf8.count - end.utf8.count
    let wide = filler.utf8.count
    let fill = String(repeating: filler, count: room / wide) + String(repeating: "x", count: room % wide)
    return SendContact.Card(name: name, vcard: begin + fill + end)
}

@Test func `jawd's limits are counted as jawd counts them, in its order: 64 cards, 256 KiB of vCards in UTF-8, a name of 256 characters trimmed`() {
    #expect(SendContact.refusal([]) == .noCards)
    #expect(SendContact.refusal(Array(repeating: marta, count: 64)) == nil, "64 cards go")
    #expect(SendContact.refusal(Array(repeating: SendContact.Card(name: " ", vcard: marta.vcard), count: 65)) == .cards(65),
            "65 don't, before their names are looked at")

    let most = SendContact.vcardsMost
    #expect(sized(most, filler: "ã").vcard.utf8.count == most && sized(most, filler: "ã").vcard.count < most / 2 + 30)
    #expect(SendContact.refusal([sized(most, filler: "ã")]) == nil, "256 KiB in UTF-8, though half as many characters")
    #expect(SendContact.refusal([sized(most + 1, filler: "ã")]) == .vcards(most + 1))
    #expect(SendContact.refusal([sized(most / 2), sized(most / 2 + 1, name: " ")]) == .vcards(most + 1),
            "together, before a card's blank name")

    let a256 = String(repeating: "a", count: 256)
    #expect(SendContact.refusal([SendContact.Card(name: "\u{3000} \(a256)\u{A0}\n", vcard: marta.vcard)]) == nil,
            "256 once trimmed of spaces as Rust trims them, the ideographic and the no-break among them")
    #expect(SendContact.refusal([SendContact.Card(name: a256 + "a", vcard: marta.vcard)]) == .name(0, 257))
    let thumbs = String(repeating: "👍🏽", count: 128)
    #expect(thumbs.count == 128 && thumbs.unicodeScalars.count == 256)
    #expect(SendContact.refusal([SendContact.Card(name: thumbs, vcard: marta.vcard)]) == nil)
    #expect(SendContact.refusal([SendContact.Card(name: thumbs + "👍🏽", vcard: marta.vcard)]) == .name(0, 258),
            "Unicode scalars, as Rust's chars count, not the 129 characters Swift sees")
    #expect(SendContact.refusal([marta, SendContact.Card(name: "\u{A0}\u{2003}\t", vcard: marta.vcard)]) == .unnamed(1))
    #expect(SendContact.refusal([SendContact.Card(name: a256 + "a", vcard: marta.vcard), SendContact.Card(name: " ", vcard: marta.vcard)])
        == .name(0, 257), "card by card, the first first")
}

@MainActor
@Suite(.serialized) struct ContactClientTests {
    @Test func `cards go with their key, and come back as jawd's row with each card read`() async throws {
        let fake = try FakeJawd(script: carding)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        let ines = SendContact.Card(name: "Inês Gomes", vcard: "BEGIN:VCARD\r\nFN:Inês Gomes\r\nTEL;waid=351913002210:+351 913 002 210\r\nEND:VCARD\r\n")
        jawd.sendContact(chat: "ana", cards: [marta, ines], key: "jaw-c0de")
        let all = await drain(jawd) { all in all.contains { if case .sent = $0 { true } else { false } } }
        let sent = try #require(all.compactMap { if case let .sent(key, message, row) = $0 { (key, message, row) } else { nil } }.first)
        #expect(sent.0 == "jaw-c0de" && sent.1 == "3EB0c0de")
        let contact = try #require(sent.2?.contact)
        #expect(contact.name == "2 contacts" && contact.cards.map(\.name) == ["Marta Lopes", "Inês Gomes"])
        #expect(contact.cards.map { $0.phones.map(\.dialled) } == [["+351213456789"], ["+351913002210"]])
        #expect(requests(fake, "send.contact") == [.object([
            "chat": .string("ana"), "key": .string("jaw-c0de"), "cards": .array([
                .object(["name": .string("Marta Lopes"), "vcard": .string(marta.vcard)]),
                .object(["name": .string("Inês Gomes"), "vcard": .string(ines.vcard)]),
            ]),
        ])])
    }

    @Test func `cards whose answer was lost are asked again with their key, and past a day come back whole`() async throws {
        let fake = try FakeJawd(script: carding)
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        fake.stopAnswering()
        jawd.sendContact(chat: "ana", cards: [marta], key: "jaw-lost")
        #expect(await eventually { requests(fake, "send.contact").count == 1 })
        fake.state.withLock { $0.stopped = false }
        fake.drop()
        let after = await drain(jawd) { all in all.contains { if case .sent = $0 { true } else { false } } }
        #expect(after.contains { if case let .unsure(chat, key) = $0 { chat == "ana" && key == "jaw-lost" } else { false } },
                "said to be unsure while jawd was away")
        #expect(after.contains { if case let .sent(key, _, row) = $0 { key == "jaw-lost" && row?.contact?.cards.first?.name == "Marta Lopes" } else { false } },
                "then sent, under the same key")
        #expect(requests(fake, "send.contact").map { $0["key"]?.string } == ["jaw-lost", "jaw-lost"])

        fake.stopAnswering()
        jawd.sendContact(chat: "ana", cards: [marta], key: "jaw-old")
        #expect(await eventually { requests(fake, "send.contact").count == 3 })
        jawd.now = { Date().addingTimeInterval(21 * 60 * 60) }
        fake.state.withLock { $0.stopped = false }
        fake.drop()
        let late = await drain(jawd) { all in all.contains { if case .contactUnsent = $0 { true } else { false } } }
        let back = late.compactMap { if case let .contactUnsent(chat, key, cards, why) = $0 { (chat, key, cards, why) } else { nil } }.first
        #expect(back?.0 == "ana" && back?.1 == "jaw-old" && back?.2 == [marta])
        #expect(back?.3 == "jawd was away too long to tell whether it went, so look in the chat before sending it again")
        #expect(requests(fake, "send.contact").count == 3, "and isn't asked again")
    }

    @Test func `cards jawd turns down come back whole in jawd's words, and a jawd without send.contact says it comes next`() async throws {
        let fake = try FakeJawd { method, params, id in
            guard method == "send.contact" else { return ordinary(method, params, id) }
            return switch params["key"]?.string {
            case "jaw-many":
                [#"{"id":\#(id),"error":{"code":5,"message":"one message shares 64 cards at most","data":{"limit":"cards","most":64}}}"#]
            case "jaw-bad":
                [#"{"id":\#(id),"error":{"code":-32602,"message":"each card's vcard is one vCard, from BEGIN:VCARD to END:VCARD","data":{"param":"cards"}}}"#]
            default:
                [#"{"id":\#(id),"error":{"code":-32601,"message":"unknown method: send.contact"}}"#]
            }
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        for key in ["jaw-many", "jaw-bad", "jaw-before"] {
            jawd.sendContact(chat: "ana", cards: [marta], key: key)
        }
        let all = await drain(jawd) { $0.count { if case .contactUnsent = $0 { true } else { false } } == 3 }
        let said = all.compactMap { if case let .contactUnsent(_, key, cards, why) = $0, cards == [marta] { "\(key): \(why)" } else { nil } }
        #expect(said == [
            "jaw-many: one message shares 64 cards at most",
            "jaw-bad: each card's vcard is one vCard, from BEGIN:VCARD to END:VCARD",
            "jaw-before: sending contacts comes with the next step",
        ])
    }

    @Test func `jawd's other refusals bring the cards back whole in its words, and a jawd away says so at once`() async throws {
        let fake = try FakeJawd { method, params, id in
            guard method == "send.contact" else { return ordinary(method, params, id) }
            return switch params["key"]?.string {
            case "jaw-vcards":
                [#"{"id":\#(id),"error":{"code":5,"message":"a message's vCards take 256 KiB at most together","data":{"limit":"vcards","most":262144}}}"#]
            case "jaw-name":
                [#"{"id":\#(id),"error":{"code":5,"message":"a card's name is 256 characters at most","data":{"limit":"name","most":256}}}"#]
            case "jaw-chat":
                [#"{"id":\#(id),"error":{"code":3,"message":"jawd has no chat ana; chat.start finds or makes one"}}"#]
            default:
                [#"{"id":\#(id),"error":{"code":5,"message":"40 messages of yours are waiting to send, 2 of them failed; jawd takes more once some have gone or been retried","data":{"limit":"outbox","waiting":40,"failed":2}}}"#]
            }
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        for key in ["jaw-vcards", "jaw-name", "jaw-chat", "jaw-full"] {
            jawd.sendContact(chat: "ana", cards: [marta], key: key)
        }
        let all = await drain(jawd) { $0.count { if case .contactUnsent = $0 { true } else { false } } == 4 }
        let said = all.compactMap { if case let .contactUnsent(chat, key, cards, why) = $0, chat == "ana", cards == [marta] { "\(key): \(why)" } else { nil } }
        #expect(said == [
            "jaw-vcards: a message's vCards take 256 KiB at most together",
            "jaw-name: a card's name is 256 characters at most",
            "jaw-chat: jawd has no chat ana; chat.start finds or makes one",
            "jaw-full: 40 wait to send, 2 failed; it can go once some have",
        ])

        let away = Jawd(socket: fake.socket + ".nobody", client: "jaw-mac-test", version: "0", focus: true) { _ in }
        away.start()
        defer { away.close() }
        away.sendContact(chat: "ana", cards: [marta], key: "jaw-away")
        let back = await drain(away) { all in all.contains { if case .contactUnsent = $0 { true } else { false } } }
        #expect(back.contains { if case let .contactUnsent(_, key, cards, why) = $0 {
            key == "jaw-away" && cards == [marta] && why == "jawd isn't connected; jaw is trying to reach it, so try again in a moment"
        } else { false } })
    }

    @Test func `a card of 10 MiB is read on the client's thread as its row arrives, and the main thread's drain reads none of it`() async throws {
        // Escaped as JSON, a card of 10 MiB broken softly after each character is nearly the 16 MiB a
        // line jawd sends carries.
        let card = softBroken(10 * 1024 * 1024).replacingOccurrences(of: "VERSION:2.1\r\n", with: "VERSION:2.1\r\nFN:Ana Ribeiro\r\n")
        let row = contactRow("3EB0BIG", name: "Ana Ribeiro", vcard: card, mine: false)
        let fake = try FakeJawd { method, params, id in
            guard method == "messages.watch" else { return ordinary(method, params, id) }
            return ["{\"id\":\(id),\"result\":{\"watch\":\"w2\",\"seq\":10,\"total\":1,\"older\":false,\"newer\":0,\"rows\":[\(row)]}}"]
        }
        defer { fake.stop() }
        let jawd = connected(fake)
        defer { jawd.close() }
        _ = await drain(jawd) { helloed($0) == 1 }
        let window = jawd.watchMessages("ana", .newest(80))
        let clock = ContinuousClock()
        var drains: [Duration] = []
        var arrived: MessageRow?
        let end = Date().addingTimeInterval(30)
        while arrived == nil, Date() < end {
            var updates: [JawUpdate] = []
            drains.append(clock.measure { updates = jawd.drain() })
            for case let .messages(w, update) in updates where w == window {
                arrived = update.upsert.first
            }
            try await Task.sleep(for: .milliseconds(2))
        }
        let read = try #require(arrived, "the row came")
        #expect(read.contact?.name == "Ana Ribeiro" && read.contact?.cards.map(\.name) == ["Ana Ribeiro"], "its card read as it arrived")
        let longest = drains.max() ?? .zero
        let reading = fastest(card)
        // A drain that read the card would take as long as reading it, on any Mac and in any build;
        // a quarter of that leaves the drain room for its own work on a loaded one.
        #expect(longest < reading / 4, "the main thread's longest drain took \(longest); reading the card takes \(reading)")
    }
}
