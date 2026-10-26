import AppKit
import JawKit
import JawThemes
import Observation
import SwiftUI

/// The palette's state: what's typed, and jawd's answer to the latest `find`.
@MainActor @Observable
public final class PaletteModel {
    public var query = "" {
        didSet { if query != oldValue { ask() } }
    }

    public private(set) var found = Found()
    public private(set) var finding = false
    public var index = 0
    public var failed: String?
    /// What to start a chat with, when a number is typed.
    public var number: String? {
        let digits = query.filter(\.isNumber)
        let allowed = query.allSatisfy { $0.isNumber || " +-()".contains($0) }
        return allowed && digits.count >= 7 ? query.trimmingCharacters(in: .whitespaces) : nil
    }

    @ObservationIgnored public var find: ((UInt64, String) -> Void)?
    @ObservationIgnored public var picked: ((Pick) -> Void)?
    @ObservationIgnored var asked: UInt64 = 0

    public enum Pick: Sendable {
        case chat(ChatRow)
        case person(Person)
        case message(MessageRow)
        case folder(FolderRef)
        case number(String)
    }

    public init() {}

    public func ask() {
        asked += 1
        finding = true
        failed = nil
        find?(asked, query)
    }

    /// jawd's answer to a find; one to an older one is dropped.
    public func answered(id: UInt64, _ found: Found) {
        guard id == asked else { return }
        self.found = found
        finding = false
        index = min(index, max(items.count - 1, 0))
    }

    public func failed(id: UInt64, why: String) {
        guard id == asked else { return }
        finding = false
        failed = why
    }

    /// The results as listed: a number to start a chat with, then what jawd found, by kind, best
    /// first, four of each when everything shows.
    public var items: [Item] {
        var out: [Item] = []
        if let number { out.append(Item(kind: nil, title: [Span(.text, "Start a chat with \(number)")], detail: "a number", pick: .number(number))) }
        for kind in FindKind.allCases {
            for hit in found.results.filter({ $0.kind == kind }).prefix(query.isEmpty ? 8 : 4) {
                guard let pick = Self.pick(hit) else { continue }
                out.append(Item(kind: kind, title: hit.text, detail: Self.detail(hit), pick: pick))
            }
        }
        return out
    }

    public struct Item: Identifiable {
        public let id = UUID()
        public let kind: FindKind?
        public let title: [Span]
        public let detail: String
        public let pick: Pick
    }

    static func pick(_ hit: Hit) -> Pick? {
        switch hit.row {
        case let .chat(c): .chat(c)
        case let .person(p): .person(p)
        case let .message(m): .message(m)
        case let .folder(f): .folder(f)
        case .other: nil
        }
    }

    static func detail(_ hit: Hit) -> String {
        switch hit.row {
        case let .chat(c):
            var parts = [c.kind.isGroup ? "group" : "chat"]
            if c.unread > 0 { parts.append("\(c.unread) unread") }
            if c.archived { parts.append("archived") }
            parts.append(contentsOf: hit.folders)
            return parts.joined(separator: " · ")
        case .person: return "no chat yet · ⏎ starts one"
        case let .message(m): return [m.from?.name ?? (m.mine ? "You" : nil), hit.chatName, Format.when(m.at, now: Format.now())]
            .compactMap { $0 }.joined(separator: " · ")
        case .folder: return "folder"
        case .other: return ""
        }
    }

    public func move(_ by: Int) {
        let n = items.count
        guard n > 0 else { return }
        index = (index + by + n) % n
    }

    public func pick() {
        let all = items
        guard index < all.count else { return }
        picked?(all[index].pick)
    }

    public func pick(_ item: Item) {
        picked?(item.pick)
    }
}

/// The palette: jawd's `find` behind one line, results grouped by kind with the counts above them.
public struct PaletteView: View {
    @Bindable var model: PaletteModel
    var roles: Roles
    var close: () -> Void
    @FocusState var focused: Bool

    public init(model: PaletteModel, roles: Roles, close: @escaping () -> Void) {
        self.model = model
        self.roles = roles
        self.close = close
    }

    public var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").foregroundStyle(colour(roles.secondary))
                TextField("Chats, people and messages, or a number to start a chat", text: $model.query)
                    .textFieldStyle(.plain)
                    .font(.system(size: 16))
                    .focused($focused)
                    .onKeyPress(.downArrow) { model.move(1); return .handled }
                    .onKeyPress(.upArrow) { model.move(-1); return .handled }
                    .onKeyPress(.escape) { close(); return .handled }
                    .onSubmit { model.pick() }
                if model.finding { ProgressView().controlSize(.small) }
            }
            .padding(.horizontal, 14)
            .frame(height: 46)
            counts
            Divider().overlay(colour(roles.separator))
            results
        }
        .frame(width: 620)
        .background(colour(roles.surface))
        .clipShape(RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14).stroke(colour(roles.separator)))
        .onAppear { focused = true }
        .focusEffectDisabled()
    }

    @ViewBuilder var counts: some View {
        let shown = FindKind.allCases.compactMap { k in model.found.counts[k].map { (k, $0) } }.filter { $0.1 > 0 }
        if !shown.isEmpty {
            HStack(spacing: 12) {
                ForEach(shown, id: \.0) { kind, n in
                    Text("\(Self.plural(kind)) \(n >= 1000 ? "1,000+" : Format.num(n))")
                }
                Spacer()
            }
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(colour(roles.secondary))
            .padding(.horizontal, 14)
            .padding(.bottom, 6)
        }
    }

    var results: some View {
        let items = model.items
        return ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(items.enumerated()), id: \.element.id) { i, item in
                        if i == 0 || items[i - 1].kind != item.kind {
                            Text(item.kind.map(Self.plural) ?? "a number")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(colour(roles.tertiary))
                                .padding(.horizontal, 14).padding(.top, 8).padding(.bottom, 2)
                        }
                        row(item, selected: i == model.index).id(i)
                            .onTapGesture { model.pick(item) }
                    }
                    if items.isEmpty, !model.finding {
                        Text(model.failed ?? (model.query.isEmpty ? "Type to find chats, people and messages." : "Nothing found."))
                            .foregroundStyle(colour(roles.secondary)).padding(14)
                    }
                }
                .padding(.vertical, 4)
            }
            .frame(maxHeight: 372)
            .onChange(of: model.index) { proxy.scrollTo(model.index) }
        }
    }

    func row(_ item: PaletteModel.Item, selected: Bool) -> some View {
        HStack(spacing: 8) {
            Self.marked(item.title, roles).lineLimit(1)
            Spacer(minLength: 8)
            Text(item.detail).font(.system(size: 11)).foregroundStyle(colour(roles.secondary)).lineLimit(1)
        }
        .padding(.horizontal, 14)
        .frame(height: 32)
        .background(selected ? colour(roles.selection) : .clear)
        .contentShape(Rectangle())
    }

    static func marked(_ spans: [Span], _ roles: Roles) -> Text {
        var out = AttributedString()
        for span in spans {
            var part = AttributedString(span.s)
            part.foregroundColor = Color(nsColor: (span.t == .mark ? roles.accent : roles.text).nsColor)
            if span.t == .mark { part.font = .system(size: 13, weight: .bold) }
            out += part
        }
        return Text(out).font(.system(size: 13))
    }

    static func plural(_ kind: FindKind) -> String {
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

    func colour(_ c: RGBA) -> Color {
        Color(nsColor: c.nsColor)
    }
}

/// The palette's panel, at the top of the window, its field taking keys in the frame after ⌘K.
@MainActor
public final class PalettePanel: NSPanel {
    public let model = PaletteModel()

    public init() {
        super.init(contentRect: NSRect(x: 0, y: 0, width: 620, height: 460), styleMask: [.borderless, .nonactivatingPanel],
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

    /// Shows the palette over `window`, with jawd's answers to what's typed.
    public func show(over window: NSWindow, roles: Roles) {
        let hosting = NSHostingView(rootView: PaletteView(model: model, roles: roles) { [weak self] in self?.close() })
        hosting.frame = NSRect(x: 0, y: 0, width: 620, height: 460)
        contentView = hosting
        let frame = window.frame
        setFrame(NSRect(x: frame.midX - 310, y: frame.maxY - 60 - 460, width: 620, height: 460), display: true)
        window.addChildWindow(self, ordered: .above)
        makeKeyAndOrderFront(nil)
        if model.query.isEmpty { model.ask() }
    }

    override public func resignKey() {
        super.resignKey()
        close()
    }

    override public func close() {
        parent?.removeChildWindow(self)
        super.close()
        model.query = ""
        model.index = 0
    }

    override public func cancelOperation(_: Any?) {
        close()
    }
}
