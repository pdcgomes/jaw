import AppKit
import CoreGraphics
@testable import JawKit
import JawThemes
@testable import JawUI
import Testing

/// The library's search line, as the terminal's: words and `from:`, `in:`, `since:` and `until:`
/// typed and taken as tokens, what jawd's asked, and the pane with tokens and offers drawn as its
/// references in both looks, dark and light.

let people = [(id: "person:4", name: "Ana Ribeiro"), (id: "person:7", name: "Rui Costa"), (id: "person:9", name: "Inês Gomes")]
let chats = Samples.chats.map { (id: $0.id, name: $0.name) }

func offered(_ text: String, now: Int64 = Samples.now) -> [String] {
    LibrarySearch(text: text).offers(before: text, people: people, chats: chats, now: now, tz: lisbon).map(\.said)
}

@Test func `a filter being typed is found at the end of what's typed, a name of several words too`() {
    #expect(LibrarySearch.typing("from:a").map { [$0.name, $0.rest] } == ["from:", "a"])
    #expect(LibrarySearch.typing("praia from:").map { [String($0.start), $0.name, $0.rest] } == ["6", "from:", ""])
    #expect(LibrarySearch.typing("sunset in:Lisbon Cl").map { [$0.name, $0.rest] } == ["in:", "Lisbon Cl"])
    #expect(LibrarySearch.typing("praia") == nil && LibrarySearch.typing("") == nil)
    #expect(LibrarySearch.typing("xfrom:a") == nil, "a filter starts a word")
    #expect(LibrarySearch.typing("in:a b c d e") == nil, "a name of more than three words is the words")
    #expect(LibrarySearch(text: "praia from:an").words() == "praia", "the words leave out the filter being typed")
}

@Test func `from: offers you and the people, in: the chats, each by the starts of their words`() {
    #expect(offered("from:") == ["you", "Ana Ribeiro", "Rui Costa", "Inês Gomes"])
    #expect(offered("from:a") == ["Ana Ribeiro"])
    #expect(offered("from:ines") == ["Inês Gomes"], "accents either way")
    #expect(offered("from:yo") == ["you"])
    #expect(offered("in:lis") == ["Lisbon Climbing"] && offered("in:") == chats.map(\.name))
    #expect(offered("praia") == [])
}

@Test func `since: and until: offer the terminal's days and months, or a date typed, until: to the day's end`() throws {
    let all = offered("since:")
    #expect(Array(all.prefix(5)) == ["today", "yesterday", "a week ago", "a month ago", "a year ago"])
    #expect(all.count == LibrarySearch.most && offered("since:o") == ["October 2026"] && offered("until:aug") == ["August 2026"])
    let search = LibrarySearch(text: "until:yesterday")
    let until = try #require(search.offers(before: search.text, people: [], chats: [], now: Samples.now, tz: lisbon).first)
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = lisbon
    let today = calendar.startOfDay(for: Date(timeIntervalSince1970: Double(Samples.now) / 1000))
    #expect(until.token == .until(at: Int64(today.timeIntervalSince1970 * 1000) - 1, label: "yesterday"), "the whole of yesterday")
    let since = try #require(LibrarySearch(text: "since:2026-08-01").offers(before: "since:2026-08-01", people: [], chats: [], now: Samples.now,
                                                                           tz: lisbon).first)
    #expect(since.said == "1 Aug 2026")
    if case let .since(at, _) = since.token {
        #expect(Format.formatted(at, "yyyy-MM-dd HH:mm", lisbon) == "2026-08-01 00:00")
    } else {
        Issue.record("not a since: \(since.token)")
    }
}

@Test func `an offer taken is a token in place of what was typed, one of each filter, and ⌫ takes the last back`() {
    var search = LibrarySearch(text: "topo from:an")
    let ana = search.offers(before: search.text, people: people, chats: chats, now: Samples.now, tz: lisbon)[0]
    search.take(ana, before: search.text)
    #expect(search.text == "topo " && search.tokens == [.from(id: "person:4", name: "Ana Ribeiro")])
    #expect(search.filter(kind: .file, chat: nil) == LibraryFilter(kind: .file, person: "person:4", text: "topo"))
    search.text = "from:ru"
    let rui = search.offers(before: search.text, people: people, chats: chats, now: Samples.now, tz: lisbon)[0]
    search.take(rui, before: search.text)
    #expect(search.tokens == [.from(id: "person:7", name: "Rui Costa")], "a second from: takes the first's place")
    search.text = "in:lis"
    search.take(search.offers(before: search.text, people: [], chats: chats, now: Samples.now, tz: lisbon)[0], before: search.text)
    #expect(search.filter(kind: nil, chat: "pane") == LibraryFilter(chat: Samples.group.id, person: "person:7"), "in: takes the pane's chat's place")
    #expect(!search.capped() == false && LibrarySearch(tokens: [.in(id: "c", name: "C")]).capped() == false, "a chat alone counts exactly")
    #expect(search.text.isEmpty && search.tokens.count == 2)
    let first = search.takeBack(), second = search.takeBack(), third = search.takeBack()
    #expect(first && second && !third && search.tokens.isEmpty)
    search.tokens = [.in(id: "c", name: "C")]
    search.text = "x"
    let typed = search.takeBack()
    #expect(!typed && search.tokens.count == 1, "only in an empty field")
}

@Test func `Esc puts the offers away until the words change`() {
    var search = LibrarySearch(text: "from:a")
    search.dismissed = search.text
    #expect(search.offers(before: search.text, people: people, chats: [], now: Samples.now).isEmpty)
    search.text = "from:an"
    #expect(!search.offers(before: search.text, people: people, chats: [], now: Samples.now).isEmpty)
}

/// Types `text` into a pane's search line, as the field does.
@MainActor
func type(_ text: String, into c: LibraryController) {
    c.bar.field.stringValue = text
    c.controlTextDidChange(Notification(name: NSControl.textDidChangeNotification, object: c.bar.field))
}

@MainActor @Test func `what's typed asks jawd again only as the words change, and the grid keeps what it shows until the answer`() throws {
    let (c, asked, w) = pane(.file)
    defer { w.close() }
    c.people = { people }
    c.chats = { chats }
    let before = asked.watched.count
    type("topo", into: c)
    #expect(asked.watched.count == before + 1 && asked.watched.last?.0 == LibraryFilter(kind: .file, text: "topo"))
    #expect(c.items.count == 4 && !c.window.loaded, "what was shown stays until jawd answers")
    type("topo from:", into: c)
    #expect(asked.watched.count == before + 1, "a filter being typed asks nothing")
    #expect(!c.offersView.isHidden && c.offers.map(\.said) == ["you", "Ana Ribeiro", "Rui Costa", "Inês Gomes"],
            "the senders held are offered once, by the name the app knows them by")
    type("topo from:a", into: c)
    #expect(c.control(c.bar.field, textView: NSTextView(), doCommandBy: #selector(NSResponder.insertTab(_:))))
    #expect(c.search.tokens == [.from(id: "person:4", name: "Ana Ribeiro")] && c.bar.field.stringValue == "topo " && c.offersView.isHidden)
    #expect(asked.watched.last?.0 == LibraryFilter(kind: .file, person: "person:4", text: "topo"))
    c.apply(Samples.libraryUpdate(of: [], kind: .file))
    #expect(c.items.isEmpty && c.laid.elements.map(\.what) == [.said("Nothing matches.")])
    type("", into: c)
    #expect(c.control(c.bar.field, textView: NSTextView(), doCommandBy: #selector(NSResponder.deleteBackward(_:))))
    #expect(c.search.tokens.isEmpty && asked.watched.last?.0 == LibraryFilter(kind: .file))
    type("in:lis", into: c)
    #expect(c.control(c.bar.field, textView: NSTextView(), doCommandBy: #selector(NSResponder.cancelOperation(_:))))
    #expect(c.offersView.isHidden && asked.closed == 0, "Esc puts the offers away first")
    #expect(c.control(c.bar.field, textView: NSTextView(), doCommandBy: #selector(NSResponder.cancelOperation(_:))))
    #expect(asked.closed == 1, "then closes the pane")
}

@MainActor @Test func `in the search line the arrows move the selection, ← and → while it's empty, and Tab changes the kind`() {
    let (c, _, w) = pane(.photo)
    defer { w.close() }
    let tv = NSTextView()
    #expect(c.control(c.bar.field, textView: tv, doCommandBy: #selector(NSResponder.moveDown(_:))) && c.selected == "LP0")
    #expect(c.control(c.bar.field, textView: tv, doCommandBy: #selector(NSResponder.moveRight(_:))) && c.selected == "LP1")
    #expect(c.control(c.bar.field, textView: tv, doCommandBy: #selector(NSResponder.moveDown(_:))) && c.selected == "LP6", "a line down")
    type("x", into: c)
    #expect(!c.control(c.bar.field, textView: tv, doCommandBy: #selector(NSResponder.moveLeft(_:))), "with words, ← moves the caret")
    #expect(c.control(c.bar.field, textView: tv, doCommandBy: #selector(NSResponder.insertTab(_:))) && c.kind == .video)
}

@MainActor @Test(.timeLimit(.minutes(1))) func `the pane with tokens and offers draws as its references, in both looks, dark and light`() async throws {
    try await asOfSamples {
        for look in Look.allCases {
            for dark in [true, false] {
                let style = libraryStyle(look, dark: dark)
                let (c, _, w) = pane(.file, style: style, height: 460)
                defer { w.close() }
                c.chats = { chats }
                c.typed("in:lis", tokens: [.from(id: "person:4", name: "Ana Ribeiro"), .since(at: 0, label: "September 2026")])
                c.ask()
                let ana = Samples.library.filter { $0.from?.id == "person:4" }
                c.apply(Samples.libraryUpdate(of: ana, kind: .file, counts: Samples.counts(of: ana)))
                c.select("LF0")
                w.layoutIfNeeded()
                c.scroll.verticalScroller?.isHidden = true
                try compare(try picture(c.view, on: style.roles.ground), "library-search-\(look.rawValue)-\(dark ? "dark" : "light")")
            }
        }
    }
}
