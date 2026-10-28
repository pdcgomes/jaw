import AppKit
import JawKit
import JawThemes
import Observation
import SwiftUI

/// One of the app's own actions, as the palette lists it with its key, so the palette teaches the
/// keys as the terminal's does.
public struct PaletteAction: Identifiable, Sendable {
    public let id: String
    public let name: String
    public let detail: String
    public let key: String?
    public let run: @MainActor @Sendable () -> Void

    public init(_ name: String, _ detail: String = "", key: String? = nil, run: @escaping @MainActor @Sendable () -> Void) {
        id = name
        self.name = name
        self.detail = detail
        self.key = key
        self.run = run
    }
}

/// The palette's state: what's typed, what it's narrowed to, and jawd's answer to the latest `find`.
@MainActor @Observable
public final class PaletteModel {
    public enum Mode: Equatable {
        /// Finding chats, people, folders and messages, and the app's actions.
        case find
        /// A reaction to a message: the six quick ones, or any emoji by name.
        case react(message: String, summary: String)
        /// Chats to forward messages to, five at most, picked with Tab.
        case forward(messages: [String], from: String, summary: String)
    }

    public var mode: Mode = .find
    public var query = "" {
        didSet { if query != oldValue { selected = nil; ask() } }
    }

    /// The kind the results are narrowed to with Tab, or all of them.
    public var kind: FindKind?
    /// A chat the messages found are in, for searching this chat.
    public var scope: (id: String, name: String)?
    public var picked: [ChatRow] = []
    public private(set) var found = Found()
    /// The counts of the last answer to every kind, kept so Tab can come back.
    public private(set) var counts: [FindKind: Int] = [:]
    public private(set) var finding = false
    /// The selected item, by its id, so it stays on the same row while jawd's answers come in and
    /// the rows above it change; nil for the first.
    public var selected: String?
    public var failed: String?

    /// Where the selected item is in the list now: the first when it's gone or nothing is chosen.
    public var index: Int {
        get {
            let all = items
            return selected.flatMap { id in all.firstIndex { $0.id == id } } ?? 0
        }
        set {
            let all = items
            selected = all.indices.contains(newValue) ? all[newValue].id : nil
        }
    }

    @ObservationIgnored public var find: ((UInt64, String, [FindKind], String?, UInt32) -> Void)?
    @ObservationIgnored public var chose: ((Pick) -> Void)?
    @ObservationIgnored public var actions: () -> [PaletteAction] = { [] }
    @ObservationIgnored var asked: UInt64 = 0

    public static let most = 5

    public enum Pick: Sendable {
        case chat(ChatRow)
        /// A chat in a pane of its own: ⌘↩ on a chat.
        case newPane(ChatRow)
        case person(Person)
        case message(MessageRow)
        case folder(FolderRef)
        case number(String)
        case action(PaletteAction)
        case emoji(message: String, String)
        case forward(messages: [String], from: String, to: [String])
    }

    public init() {}

    /// Opens the palette afresh in a mode.
    public func reset(_ mode: Mode = .find, kind: FindKind? = nil, scope: (String, String)? = nil) {
        self.mode = mode
        self.kind = mode == .find ? kind : (mode.isForward ? .chat : nil)
        self.scope = scope.map { ($0.0, $0.1) }
        picked = []
        found = Found()
        counts = [:]
        failed = nil
        selected = nil
        if query.isEmpty { ask() } else { query = "" }
    }

    public func ask() {
        if case .react = mode { return }
        asked += 1
        finding = true
        failed = nil
        let kinds = kind.map { [$0] } ?? []
        find?(asked, query, kinds, scope?.id, kind == nil ? 10 : 50)
    }

    /// jawd's answer to a find; one to an older one is dropped.
    public func answered(id: UInt64, _ found: Found) {
        guard id == asked else { return }
        self.found = found
        if kind == nil { counts = found.counts }
        finding = false
    }

    public func failed(id: UInt64, why: String) {
        guard id == asked else { return }
        finding = false
        failed = why
    }

    /// What to start a chat with, when a number is typed.
    public var number: String? {
        let digits = query.filter(\.isNumber)
        let allowed = query.allSatisfy { $0.isNumber || " +-()".contains($0) }
        return allowed && digits.count >= 7 ? query.trimmingCharacters(in: .whitespaces) : nil
    }

    // MARK: - What's listed

    public enum Group: Hashable {
        case number, recent, actions, kind(FindKind), reactions, emoji, picked

        public var title: String {
            switch self {
            case .number: "a number"
            case .recent: "recent"
            case .actions: "actions"
            case let .kind(k): PaletteView.plural(k)
            case .reactions: "quick reactions, 1 to 6"
            case .emoji: "emoji"
            case .picked: "forwarding to"
            }
        }
    }

    public struct Item: Identifiable {
        public let id: String
        public let group: Group
        public let title: [Span]
        public let detail: String
        public let key: String?
        public let pick: Pick
    }

    public var items: [Item] {
        switch mode {
        case .react(let message, _): return reactions(message)
        case .forward: return forwardItems
        case .find: return findItems
        }
    }

    var findItems: [Item] {
        var out: [Item] = []
        if let number, kind == nil || kind == .person {
            out.append(Item(id: "number", group: .number, title: [Span(.text, "Start a chat with \(number)")], detail: "⏎ starts it",
                            key: nil, pick: .number(number)))
        }
        let actions = kind == nil && scope == nil ? rankedActions : []
        let verb = query.count >= 3 && (actions.first?.1 ?? 0) >= 80
        if verb { out.append(contentsOf: actions.map(\.0)) }
        for k in FindKind.allCases where kind == nil || kind == k {
            let hits = found.results.filter { $0.kind == k }
            for (i, hit) in hits.prefix(kind == nil ? (query.isEmpty ? 8 : 4) : 50).enumerated() {
                guard let pick = Self.pick(hit) else { continue }
                if k == .message, !verb, kind == nil, !actions.isEmpty, !out.contains(where: { $0.group == .actions }) {
                    out.append(contentsOf: actions.map(\.0))
                }
                let group: Group = query.isEmpty && k == .chat ? .recent : .kind(k)
                out.append(Item(id: "\(k)\(i)\(Self.id(hit))", group: group, title: hit.text.isEmpty ? [Span(.text, Self.name(hit))] : hit.text,
                                detail: Self.detail(hit), key: nil, pick: pick))
            }
        }
        if !verb, !out.contains(where: { $0.group == .actions }) { out.append(contentsOf: actions.map(\.0)) }
        return out
    }

    /// The app's actions matched as jawd matches names, best first; all of them when nothing's typed.
    var rankedActions: [(Item, Int)] {
        let all = actions()
        let scored: [(PaletteAction, Int)] = query.isEmpty
            ? all.prefix(8).map { ($0, 0) }
            : all.compactMap { a in Self.score(query, a.name).map { (a, $0) } }.sorted { $0.1 > $1.1 }
        return scored.map { a, s in
            (Item(id: "action:\(a.id)", group: .actions, title: Self.marked(query, a.name), detail: a.detail, key: a.key, pick: .action(a)), s)
        }
    }

    func reactions(_ message: String) -> [Item] {
        var out: [Item] = []
        if query.isEmpty {
            for (i, e) in ["👍", "❤️", "😂", "😮", "😢", "🙏"].enumerated() {
                out.append(Item(id: "q\(i)", group: .reactions, title: [Span(.text, e)], detail: "", key: "\(i + 1)", pick: .emoji(message: message, e)))
            }
            out.append(Item(id: "none", group: .reactions, title: [Span(.text, "Take mine back")], detail: "", key: nil,
                            pick: .emoji(message: message, "")))
        } else {
            for (i, e) in Emoji.matching(query, most: 40).enumerated() {
                out.append(Item(id: "e\(i)", group: .emoji, title: [Span(.text, "\(e.emoji)  "), Span(.text, e.name)], detail: "", key: nil,
                                pick: .emoji(message: message, e.emoji)))
            }
        }
        return out
    }

    var forwardItems: [Item] {
        guard case let .forward(messages, from, _) = mode else { return [] }
        var out = picked.map { c in
            Item(id: "picked\(c.id)", group: .picked, title: [Span(.text, c.name)], detail: "Tab takes it out", key: nil,
                 pick: .forward(messages: messages, from: from, to: picked.map(\.id)))
        }
        for (i, hit) in found.results.filter({ $0.kind == .chat }).enumerated() {
            guard case let .chat(row) = hit.row, !picked.contains(where: { $0.id == row.id }) else { continue }
            out.append(Item(id: "c\(i)\(row.id)", group: .kind(.chat), title: hit.text.isEmpty ? [Span(.text, row.name)] : hit.text,
                            detail: Self.detail(hit), key: nil, pick: .chat(row)))
        }
        return out
    }

    // MARK: - Keys

    /// ↑ and ↓: the row before or after the selected one, wrapping round at the ends.
    public func move(_ by: Int) {
        let all = items
        guard !all.isEmpty else { return }
        let at = selected.flatMap { id in all.firstIndex { $0.id == id } } ?? 0
        selected = all[(at + by + all.count) % all.count].id
    }

    /// Tab: in forwarding, picks or unpicks the chat; otherwise narrows to the next kind found.
    public func tab(back: Bool = false) {
        if case .forward = mode {
            guard index < items.count else { return }
            let item = items[index]
            if item.group == .picked, let c = picked.first(where: { "picked\($0.id)" == item.id }) {
                picked.removeAll { $0.id == c.id }
            } else if case let .chat(row) = item.pick, picked.count < Self.most {
                picked.append(row)
            }
            return
        }
        guard mode == .find else { return }
        let kinds = FindKind.allCases.filter { (counts[$0] ?? 0) > 0 }
        let strip: [FindKind?] = [nil] + kinds
        let at = strip.firstIndex(of: kind) ?? 0
        kind = strip[(at + (back ? -1 : 1) + strip.count) % strip.count]
        selected = nil
        ask()
    }

    /// Backspace on an empty query: the scope goes, then the narrowing.
    public func backspace() -> Bool {
        guard query.isEmpty else { return false }
        if scope != nil {
            scope = nil
            ask()
            return true
        }
        if kind != nil, mode == .find {
            kind = nil
            ask()
            return true
        }
        if case .forward = mode, !picked.isEmpty {
            picked.removeLast()
            return true
        }
        return false
    }

    /// ↩, or ⌘↩ with `newPane`, which opens a chat in a pane of its own and does what ↩ does with
    /// anything else.
    public func pick(newPane: Bool = false) {
        let all = items
        if case let .forward(messages, from, _) = mode {
            let to = picked.isEmpty ? (index < all.count ? [all[index]].compactMap { if case let .chat(c) = $0.pick { c.id } else { nil } } : []) : picked.map(\.id)
            guard !to.isEmpty else { return }
            chose?(.forward(messages: messages, from: from, to: to))
            return
        }
        guard index < all.count else { return }
        if newPane, case let .chat(row) = all[index].pick {
            chose?(.newPane(row))
        } else {
            chose?(all[index].pick)
        }
    }

    public func pick(_ item: Item) {
        selected = item.id
        pick()
    }

    /// 1 to 6 for the quick reactions while nothing is typed.
    public func digit(_ n: Int) -> Bool {
        guard case let .react(message, _) = mode, query.isEmpty, (1 ... 6).contains(n) else { return false }
        chose?(.emoji(message: message, ["👍", "❤️", "😂", "😮", "😢", "🙏"][n - 1]))
        return true
    }

    var selectedMessage: MessageRow? {
        guard index < items.count, case let .message(m) = items[index].pick else { return nil }
        return m
    }

    // MARK: - Helpers

    static func pick(_ hit: Hit) -> Pick? {
        switch hit.row {
        case let .chat(c): .chat(c)
        case let .person(p): .person(p)
        case let .message(m): .message(m)
        case let .folder(f): .folder(f)
        case .other: nil
        }
    }

    static func id(_ hit: Hit) -> String {
        switch hit.row {
        case let .chat(c): c.id
        case let .person(p): p.id
        case let .message(m): m.id
        case let .folder(f): f.id
        case .other: ""
        }
    }

    static func name(_ hit: Hit) -> String {
        switch hit.row {
        case let .chat(c): c.name
        case let .person(p): p.name
        case let .message(m): m.plain
        case let .folder(f): f.name
        case .other: ""
        }
    }

    static func detail(_ hit: Hit) -> String {
        switch hit.row {
        case let .chat(c):
            var parts = [c.kind.isGroup ? "group" : "chat"]
            if c.unread > 0 { parts.append("\(c.unread) unread") }
            if c.archived { parts.append("archived") }
            if let last = c.last { parts.append(Format.when(last.at, now: Format.now())) }
            parts.append(contentsOf: hit.folders)
            return parts.joined(separator: " · ")
        case .person: return "no chat yet · ⏎ starts one"
        case let .message(m): return [m.from?.name ?? (m.mine ? "You" : nil), hit.chatName, Format.when(m.at, now: Format.now())]
            .compactMap { $0 }.joined(separator: " · ")
        case .folder: return "folder"
        case .other: return ""
        }
    }

    nonisolated static func fold(_ s: String) -> String {
        s.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
    }

    /// How well a query matches a name, as jawd matches names: the whole start, initials, word
    /// starts, then a part of a word; nil when it doesn't.
    nonisolated static func score(_ query: String, _ name: String) -> Int? {
        let q = fold(query).trimmingCharacters(in: .whitespaces), n = fold(name)
        guard !q.isEmpty else { return 0 }
        if n.hasPrefix(q) { return 100 }
        let words = n.split(whereSeparator: { !$0.isLetter && !$0.isNumber })
        let initials = String(words.compactMap(\.first))
        if initials.hasPrefix(q) { return 90 }
        let parts = q.split(separator: " ")
        if parts.allSatisfy({ p in words.contains { $0.hasPrefix(p) } }) { return 80 }
        if words.contains(where: { $0.contains(q) }) { return 40 }
        return nil
    }

    /// The name with the query's letters marked where they matched a word's start.
    nonisolated static func marked(_ query: String, _ name: String) -> [Span] {
        let q = fold(query)
        guard !q.isEmpty, let r = fold(name).range(of: q) else { return [Span(.text, name)] }
        let start = name.index(name.startIndex, offsetBy: fold(name).distance(from: fold(name).startIndex, to: r.lowerBound))
        let end = name.index(start, offsetBy: min(q.count, name.distance(from: start, to: name.endIndex)))
        return [Span(.text, String(name[..<start])), Span(.mark, String(name[start ..< end])), Span(.text, String(name[end...]))]
    }
}

extension PaletteModel.Mode {
    var isForward: Bool {
        if case .forward = self { true } else { false }
    }
}

/// The palette: jawd's `find` behind one line, results grouped by kind with the counts above them,
/// and the app's actions among them with their keys.
public struct PaletteView: View {
    @Bindable var model: PaletteModel
    var roles: Roles
    var close: () -> Void
    var goTo: (MessageRow) -> Void
    @FocusState var focused: Bool

    public init(model: PaletteModel, roles: Roles, close: @escaping () -> Void, goTo: @escaping (MessageRow) -> Void = { _ in }) {
        self.model = model
        self.roles = roles
        self.close = close
        self.goTo = goTo
    }

    var placeholder: String {
        switch model.mode {
        case .find:
            if let scope = model.scope { return "Messages in \(scope.name)" }
            if model.kind == .message { return "Messages in every chat" }
            return "Chats, people, messages and actions, or a number to start a chat"
        case let .react(_, summary): return "React to “\(summary)”: 1 to 6, or an emoji by name"
        case let .forward(_, _, summary): return "Forward “\(summary)” to: Tab picks up to five chats"
        }
    }

    public var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: icon).foregroundStyle(roles.secondary.color)
                if let scope = model.scope { chip("in \(scope.name)") }
                if let kind = model.kind, model.mode == .find { chip(Self.plural(kind)) }
                TextField(placeholder, text: $model.query)
                    .textFieldStyle(.plain)
                    .textStyle(.heading)
                    .focused($focused)
                    .onKeyPress(.downArrow) { model.move(1); return .handled }
                    .onKeyPress(.upArrow) { model.move(-1); return .handled }
                    .onKeyPress(.escape) { close(); return .handled }
                    .onKeyPress(.tab, phases: .down) { press in model.tab(back: press.modifiers.contains(.shift)); return .handled }
                    .onKeyPress(.delete) { model.backspace() ? .handled : .ignored }
                    .onKeyPress(characters: .decimalDigits) { press in
                        model.digit(Int(press.characters) ?? 0) ? .handled : .ignored
                    }
                    .onKeyPress(characters: CharacterSet(charactersIn: "gG")) { press in
                        guard press.modifiers.contains(.command) || press.modifiers.contains(.control),
                              let m = model.selectedMessage else { return .ignored }
                        goTo(m)
                        return .handled
                    }
                    .onSubmit { model.pick() }
                if model.finding { ProgressView().controlSize(.small) }
            }
            .padding(.horizontal, 14)
            .frame(height: 46)
            strip
            Divider().overlay(roles.separator.color)
            results
            hints
        }
        .frame(width: 620)
        .background(roles.surface.color)
        .clipShape(RoundedRectangle(cornerRadius: Radius.panel))
        .overlay(RoundedRectangle(cornerRadius: Radius.panel).stroke(roles.separator.color))
        .onAppear { focused = true }
        .focusEffectDisabled()
    }

    var icon: String {
        switch model.mode {
        case .find: "magnifyingglass"
        case .react: "face.smiling"
        case .forward: "arrowshape.turn.up.right"
        }
    }

    func chip(_ text: String) -> some View {
        Text(text).textStyle(.caption).fontWeight(.medium)
            .padding(.horizontal, 7).padding(.vertical, 2)
            .background(Capsule().fill(roles.selection.color))
            .foregroundStyle(roles.text.color)
    }

    /// The counts of each kind, the one narrowed to marked, for Tab.
    @ViewBuilder var strip: some View {
        let shown = FindKind.allCases.compactMap { k in model.counts[k].map { (k, $0) } }.filter { $0.1 > 0 }
        if model.mode == .find, !shown.isEmpty {
            HStack(spacing: 12) {
                Text("all").foregroundStyle((model.kind == nil ? roles.accent : roles.secondary).color)
                ForEach(shown, id: \.0) { kind, n in
                    Text("\(Self.plural(kind)) \(n >= 1000 ? "1,000+" : Format.num(n))")
                        .foregroundStyle((model.kind == kind ? roles.accent : roles.secondary).color)
                }
                Spacer()
                Text("Tab narrows").foregroundStyle(roles.tertiary.color)
            }
            .textStyle(.mono)
            .padding(.horizontal, 14)
            .padding(.bottom, 6)
        }
    }

    var results: some View {
        let items = model.items
        let at = model.selected.flatMap { id in items.firstIndex { $0.id == id } } ?? 0
        return ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(items.enumerated()), id: \.element.id) { i, item in
                        if i == 0 || items[i - 1].group != item.group {
                            Text(item.group.title)
                                .textStyle(.caption).fontWeight(.semibold)
                                .foregroundStyle(roles.tertiary.color)
                                .padding(.horizontal, 14).padding(.top, 8).padding(.bottom, 2)
                        }
                        row(item, selected: i == at).id(item.id)
                            .onTapGesture { model.pick(item) }
                    }
                    if items.isEmpty, !model.finding {
                        Text(model.failed ?? (model.query.isEmpty ? "Type to find chats, people and messages." : "Nothing found."))
                            .foregroundStyle(roles.secondary.color).padding(14)
                    }
                }
                .padding(.vertical, 4)
            }
            .frame(maxHeight: 372)
            .onChange(of: model.selected) { proxy.scrollTo(model.selected ?? items.first?.id) }
        }
    }

    var hints: some View {
        let text = switch model.mode {
        case .find: "⏎ opens · ⌘⏎ in a new pane · ⌘G goes to a message · Tab narrows · Esc closes"
        case .react: "1 to 6 · ⏎ reacts · Esc closes"
        case .forward: "Tab picks · ⏎ forwards to \(model.picked.isEmpty ? "the chat" : "\(model.picked.count) chat\(model.picked.count == 1 ? "" : "s")") · Esc closes"
        }
        return HStack {
            Text(text).textStyle(.caption).foregroundStyle(roles.tertiary.color)
            Spacer()
        }
        .padding(.horizontal, 14).frame(height: 26)
        .background(roles.ground.opacity(0.4).color)
    }

    func row(_ item: PaletteModel.Item, selected: Bool) -> some View {
        HStack(spacing: 8) {
            if case .action = item.pick { Image(systemName: "chevron.right").textStyle(.micro).foregroundStyle(roles.tertiary.color) }
            if model.picked.contains(where: { "picked\($0.id)" == item.id }) {
                Image(systemName: "checkmark.circle.fill").foregroundStyle(roles.accent.color)
            }
            Self.marked(item.title, roles).lineLimit(1)
            Spacer(minLength: 8)
            Text(item.detail).textStyle(.caption).foregroundStyle(roles.secondary.color).lineLimit(1)
            if let key = item.key {
                Text(key).textStyle(.key).foregroundStyle(roles.accent.color)
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 32)
        .background(selected ? roles.selection.color : .clear)
        .contentShape(Rectangle())
    }

    static func marked(_ spans: [Span], _ roles: Roles) -> Text {
        var out = AttributedString()
        for span in spans {
            var part = AttributedString(span.s)
            part.foregroundColor = (span.t == .mark ? roles.accent : roles.text).color
            if span.t == .mark { part.font = .system(size: 13, weight: .bold) }
            out += part
        }
        return Text(out).textStyle(.body)
    }

    nonisolated static func plural(_ kind: FindKind) -> String {
        switch kind {
        case .chat: "chats"
        case .person: "people"
        case .folder: "folders"
        case .message: "messages"
        case .link: "links"
        case .media: "media"
        case .file: "files"
        }
    }

}

/// The palette's panel, at the top of the window, its field taking keys in the frame after ⌘K.
@MainActor
public final class PalettePanel: NSPanel {
    public let model = PaletteModel()
    public var goTo: ((MessageRow) -> Void)?

    public init() {
        super.init(contentRect: NSRect(x: 0, y: 0, width: 620, height: 490), styleMask: [.borderless, .nonactivatingPanel],
                   backing: .buffered, defer: true)
        isFloatingPanel = true
        backgroundColor = .clear
        isOpaque = false
        hasShadow = true
        hidesOnDeactivate = true
    }

    override public var canBecomeKey: Bool {
        true
    }

    /// ↑ and ↓ move through the results before the field sees them: its text editor takes them to
    /// move the caret, so SwiftUI's key handlers only got them some of the time.
    override public func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown, event.modifierFlags.intersection([.command, .option, .control]).isEmpty {
            switch event.keyCode {
            case 125: return model.move(1)
            case 126: return model.move(-1)
            default: break
            }
        }
        super.sendEvent(event)
    }

    /// ⌘↩ puts the chat in a pane of its own. A key with ⌘ goes to the key window and then to the
    /// menu bar before the window's `sendEvent`, so it's taken here.
    override public func performKeyEquivalent(with event: NSEvent) -> Bool {
        if event.type == .keyDown, [36, 76].contains(event.keyCode), event.modifierFlags.intersection(MenuBar.modifiers) == .command {
            model.pick(newPane: true)
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    /// Shows the palette over `window`, in a mode, with jawd's answers to what's typed.
    public func show(over window: NSWindow, roles: Roles, mode: PaletteModel.Mode = .find, kind: FindKind? = nil,
                     scope: (String, String)? = nil)
    {
        let view = PaletteView(model: model, roles: roles, close: { [weak self] in self?.close() },
                               goTo: { [weak self] m in self?.goTo?(m) })
        let hosting = NSHostingView(rootView: view)
        hosting.frame = NSRect(x: 0, y: 0, width: 620, height: 490)
        contentView = hosting
        let frame = window.frame
        setFrame(NSRect(x: frame.midX - 310, y: frame.maxY - 60 - 490, width: 620, height: 490), display: true)
        window.addChildWindow(self, ordered: .above)
        makeKeyAndOrderFront(nil)
        model.reset(mode, kind: kind, scope: scope)
    }

    override public func resignKey() {
        super.resignKey()
        close()
    }

    override public func close() {
        parent?.removeChildWindow(self)
        super.close()
    }

    override public func cancelOperation(_: Any?) {
        close()
    }
}
