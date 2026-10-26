@testable import JawKit
import Testing

/// jaw-client's window tests, ported: a reference model under shuffled and repeated updates, the
/// merge, the version rule, the hold, and what arrived since a move.

struct Line: WindowRow, Hashable {
    var id: String
    var v: UInt64
    var sort: String
    var body: UInt64 = 0
}

func line(_ id: String, _ v: UInt64, _ sort: String) -> Line {
    Line(id: id, v: v, sort: sort)
}

func push(_ remove: [String], _ upsert: [Line]) -> Update<Line> {
    Update(remove: remove, upsert: upsert)
}

/// A xorshift generator, so a failing seed can be run again.
struct Rng {
    var state: UInt64

    init(_ seed: UInt64) {
        state = seed &* 0x9E37_79B9_7F4A_7C15 | 1
    }

    mutating func next() -> UInt64 {
        state ^= state << 13
        state ^= state >> 7
        state ^= state << 17
        return state
    }

    mutating func below(_ n: Int) -> Int {
        Int(next() % UInt64(max(n, 1)))
    }

    mutating func oneIn(_ n: Int) -> Bool {
        below(n) == 0
    }

    mutating func shuffle<T>(_ items: inout [T]) {
        guard items.count > 1 else { return }
        for i in stride(from: items.count - 1, to: 0, by: -1) {
            items.swapAt(i, below(i + 1))
        }
    }
}

/// A window as jawd holds it at one seq: what a fresh watch would return.
struct WState {
    var seq: UInt64
    var rows: [String: Line]
    var total: UInt64
    var older: Bool
    var newer: UInt64
}

/// Every row's newest version, as jaw.db has it, and the window's state over time.
struct World {
    var rng: Rng
    var rows: [String: Line] = [:]
    var versions: [String: [Line]] = [:]
    var state = WState(seq: 1, rows: [:], total: 0, older: false, newer: 0)
    static let ids = 80

    init(_ seed: UInt64) {
        rng = Rng(seed)
        for n in 0 ..< Self.ids {
            let id = String(format: "m%02d", n)
            let row = Line(id: id, v: 1, sort: String(format: "%06d~", rng.below(1_000_000)) + id)
            rows[id] = row
            versions[id] = [row]
        }
    }

    mutating func someID() -> String {
        String(format: "m%02d", rng.below(Self.ids))
    }

    mutating func touch(_ id: String, _ change: (inout Line) -> Void) {
        change(&rows[id]!)
        versions[id]!.append(rows[id]!)
    }

    /// Moves the world on a step: rows change, some keep their `v` and take another `sort` as a
    /// held chat list's do when it reorders, some swap places, and the window's rows come and go.
    mutating func step() -> WState {
        let seq = state.seq + 1 + UInt64(rng.below(3))
        for _ in 0 ..< rng.below(6) {
            let id = someID()
            let resort = rng.oneIn(2)
            let sort = String(format: "%06d~", rng.below(1_000_000)) + id
            touch(id) { row in
                row.v = seq
                row.body += 1
                if resort { row.sort = sort }
            }
        }
        for _ in 0 ..< rng.below(3) {
            let id = someID()
            let sort = String(format: "%06d~", rng.below(1_000_000)) + id
            touch(id) { $0.sort = sort }
        }
        var members = Set(state.rows.keys)
        if rng.oneIn(3), members.count >= 2 {
            let list = members.sorted()
            let a = list[rng.below(list.count)], b = list[rng.below(list.count)]
            let (sa, sb) = (rows[a]!.sort, rows[b]!.sort)
            touch(a) { $0.sort = sb; $0.v = seq }
            touch(b) { $0.sort = sa; $0.v = seq }
        }
        for _ in 0 ..< rng.below(5) {
            let list = members.sorted()
            if !list.isEmpty { members.remove(list[rng.below(list.count)]) }
        }
        for _ in 0 ..< rng.below(6) {
            members.insert(someID())
        }
        var next = state
        next.seq = seq
        next.rows = Dictionary(uniqueKeysWithValues: members.map { ($0, rows[$0]!) })
        if rng.oneIn(3) { next.total = UInt64(next.rows.count + rng.below(50)) }
        if rng.oneIn(4) { next.older.toggle() }
        if rng.oneIn(3) { next.newer = UInt64(rng.below(1001)) }
        state = next
        return next
    }
}

/// The update from one state to the next, as jawd sends it.
func between(_ was: WState, _ now: WState, moved: Bool) -> Update<Line> {
    let upsert = now.rows.values.filter { was.rows[$0.id] != $0 }.sorted { $0.id < $1.id }
    let remove = was.rows.keys.filter { now.rows[$0] == nil }.sorted()
    return Update(
        seq: now.seq,
        remove: remove,
        upsert: upsert,
        total: moved || was.total != now.total ? now.total : nil,
        older: moved || was.older != now.older ? now.older : nil,
        newer: moved || was.newer != now.newer ? now.newer : nil,
        moved: moved ? Moved(place: .around("p\(now.seq)", before: 5, after: 5), newer: now.newer) : nil,
    )
}

/// What the client receives for one update: shuffled, sometimes split, sometimes sent twice, and
/// sometimes followed by stale copies of rows the window holds newer.
func deliver(_ rng: inout Rng, _ world: World, _ update: Update<Line>) -> [Update<Line>] {
    var update = update
    rng.shuffle(&update.upsert)
    rng.shuffle(&update.remove)
    var parts: [Update<Line>]
    if rng.oneIn(3), update.upsert.count + update.remove.count >= 2 {
        var first = update
        first.remove = Array(update.remove[(update.remove.count / 2)...])
        first.upsert = Array(update.upsert[(update.upsert.count / 2)...])
        update.remove = Array(update.remove[..<(update.remove.count / 2)])
        update.upsert = Array(update.upsert[..<(update.upsert.count / 2)])
        parts = [first, update]
    } else {
        parts = [update]
    }
    if rng.oneIn(4) {
        parts.append(parts[rng.below(parts.count)])
    }
    if rng.oneIn(3) {
        let stale = world.state.rows.values.sorted { $0.id < $1.id }
            .compactMap { row in world.versions[row.id]!.first { $0.v < row.v } }
            .prefix(3)
        if !stale.isEmpty { parts.append(Update(seq: 1, upsert: Array(stale))) }
    }
    return parts
}

func expected(_ state: WState, descending: Bool) -> [Line] {
    let rows = state.rows.values.sorted { Key($0.sort, $0.id) < Key($1.sort, $1.id) }
    return descending ? rows.reversed() : rows
}

@Test func `window models match a reference under shuffled and repeated updates`() {
    for seed in UInt64(1) ... 300 {
        var world = World(seed)
        var rng = Rng(seed ^ 0xDEAD_BEEF)
        var chats = Chats<Line>(folder: "all", place: .top(30))
        var messages = Messages<Line>(chat: "chat", place: .newest(30))
        var direct = Messages<Line>(chat: "chat", place: .newest(30))
        var mergedChats = Merged<Line>(), mergedMessages = Merged<Line>()
        var chatIDs = Set<String>(), messageIDs = Set<String>()
        var newerThen: UInt64 = 0
        var was = world.state
        for step in 0 ..< 80 {
            let now = world.step()
            let moved = step == 0 || rng.oneIn(6)
            if moved { newerThen = now.newer }
            for part in deliver(&rng, world, between(was, now, moved: moved)) {
                direct.apply(part)
                mergedChats.add(part, ids: &chatIDs)
                mergedMessages.add(part, ids: &messageIDs)
            }
            was = now
            if rng.oneIn(3), step != 79 { continue }
            let what = "seed \(seed), step \(step)"
            #expect(mergedMessages.counts.remove <= messages.count && mergedMessages.counts.upsert <= was.rows.count,
                    "\(what): a merge keeps no more than the window does")
            chats.apply(mergedChats.update())
            messages.apply(mergedMessages.update())
            mergedChats = Merged()
            mergedMessages = Merged()
            let ascending = expected(was, descending: false)
            #expect(messages.list == ascending, "\(what): messages")
            #expect(direct.list == ascending, "\(what): messages applied one by one")
            #expect(chats.list == expected(was, descending: true), "\(what): chats")
            #expect(messages.total == was.total && messages.older == was.older && messages.newer == was.newer,
                    "\(what): total, older and newer")
            #expect(chats.total == was.total, "\(what): the chat list's total")
            #expect(messages.arrived == (was.newer > newerThen ? was.newer - newerThen : 0),
                    "\(what): arrived since the move")
            #expect(messages.seq == direct.seq, "\(what): seq")
            #expect(messageIDs.sorted() == was.rows.keys.sorted(), "\(what): the ids the client keeps")
        }
    }
}

@Test func `merging updates ends where applying them in turn does`() {
    let cases: [([Update<Line>], [String])] = [
        ([push([], [line("a", 5, "1")]), push(["a"], [])], ["b@7"]),
        ([push(["b"], []), push([], [line("b", 3, "2")])], ["b@3"]),
        ([push([], [line("b", 9, "2")]), push(["b"], []), push([], [line("b", 4, "2")])], ["b@4"]),
        ([push([], [line("b", 2, "2")])], ["b@7"]),
        ([push([], [line("b", 7, "5")])], ["b@7"]),
    ]
    for (n, (updates, want)) in cases.enumerated() {
        func start() -> Messages<Line> {
            var m = Messages<Line>(chat: "c", place: .newest(5))
            m.apply(push([], [line("b", 7, "2")]))
            return m
        }
        var oneByOne = start(), atOnce = start()
        var ids: Set<String> = ["b"]
        var merged = Merged<Line>()
        for update in updates {
            oneByOne.apply(update)
            merged.add(update, ids: &ids)
        }
        atOnce.apply(merged.update())
        let held = { (m: Messages<Line>) in m.list.map { "\($0.id)@\($0.v)" } }
        #expect(held(oneByOne) == want, "case \(n), one by one")
        #expect(held(atOnce) == want, "case \(n), merged")
        #expect(oneByOne.list.first?.sort == atOnce.list.first?.sort, "case \(n)")
    }
}

@Test func `an older version never undoes a newer one and the same version resorts`() {
    var list = Chats<Line>(folder: "all", place: .top(10))
    list.apply(push([], [line("ana", 1, "0~2"), line("rui", 1, "0~1")]))
    #expect(list.list.map(\.id) == ["ana", "rui"])
    list.apply(push([], [line("rui", 2, "0~3"), line("rui", 1, "0~1")]))
    #expect(list.list.map(\.id) == ["rui", "ana"])
    list.apply(push([], [line("rui", 2, "0~0")]))
    #expect(list.list.map(\.id) == ["ana", "rui"], "the same v with a new sort, as a catch-up's reorder sends")
}

@Test func `a held chat list keeps its order with new chats first until released`() {
    var list = Chats<Line>(folder: "all", place: .top(10))
    list.apply(push([], [line("ana", 1, "0~2"), line("rui", 1, "0~1"), line("mum", 1, "0~0")]))
    list.hold()
    list.apply(push(["mum"], [line("rui", 2, "0~4"), line("sofia", 1, "0~5")]))
    #expect(list.list.map(\.id) == ["sofia", "ana", "rui"], "held, with what came since first")
    #expect(list.position("rui") == 2)
    let released = list.release(), again = list.release()
    #expect(released && !again)
    #expect(list.list.map(\.id) == ["sofia", "rui", "ana"])
    #expect(list.position("rui") == 1)
}

@Test func `messages read oldest first and say what left`() {
    var w = Messages<Line>(chat: "ana", place: .newest(5))
    #expect(!w.loaded)
    w.apply(push([], [line("b", 1, "2"), line("a", 1, "1"), line("c", 1, "3")]))
    #expect(w.loaded)
    #expect(w.oldest?.id == "a" && w.newest?.id == "c")
    #expect(w.after("1")?.id == "b")
    #expect(w.before("2")?.id == "a")
    #expect(w.before("1") == nil)
    #expect(w.countAfter("1") == 2 && w.countBefore("3", most: 1) == 1 && w.countBefore("3", most: 9) == 2)
    let gone = w.apply(push(["a", "x"], [line("d", 1, "4")]))
    #expect(gone == ["a"])
    #expect(w.nearest("1")?.id == "b", "an anchor that went falls to the next")
    #expect(w.nearest("9")?.id == "d", "or to the newest")
    #expect(w.list.reversed().map(\.id) == ["d", "c", "b"])
}

@Test func `what arrived counts from the last move and the newest is followed from the bottom`() {
    var w = Messages<Line>(chat: "ana", place: .newest(5))
    let moved = { (newer: UInt64) in Moved(place: .around("b", before: 1, after: 1), newer: newer) }
    var first = push([], [line("b", 1, "2")])
    first.newer = 40
    first.moved = moved(40)
    w.apply(first)
    #expect(w.arrived == 0 && !w.following && !w.holdsNewest)
    w.apply(Update(newer: 42))
    #expect(w.arrived == 2, "two came since the window moved")
    w.apply(Update(newer: 3, moved: moved(3)))
    #expect(w.arrived == 0)
    w.apply(Update(newer: 0, moved: Moved(place: .newest(5), newer: 0)))
    #expect(w.following && w.holdsNewest)
}

@Test func `sorts compare bytewise, as jawd's do`() {
    #expect(Key("1~10", "a") < Key("1~9", "a"))
    #expect(Key("Z", "a") < Key("a", "a"), "upper case before lower, by byte")
    #expect(Key("1", "a") < Key("1", "b"), "then by id")
}
