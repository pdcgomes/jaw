import AppKit
import JawThemes
import Observation
import SwiftUI

/// The emoji picker's state: what's typed, the emoji it finds (recents first when nothing is),
/// the one the arrows have reached, and the skin tone chosen once and kept.
@MainActor
@Observable
public final class EmojiPickerModel {
    public var query = "" {
        didSet { if query != oldValue { index = 0 } }
    }

    public var index = 0
    /// 0 for none, 1 to 5 for the five Fitzpatrick tones.
    public var tone: Int {
        didSet { defaults.set(tone, forKey: "emoji.tone") }
    }

    public private(set) var recents: [String]
    @ObservationIgnored let defaults: UserDefaults
    public static let columns = 9
    static let mostRecents = 27

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        tone = min(max(defaults.integer(forKey: "emoji.tone"), 0), 5)
        recents = defaults.stringArray(forKey: "emoji.recent") ?? []
    }

    /// What's shown: recents then every emoji when nothing's typed, otherwise the names matched.
    public var shown: [String] {
        guard query.trimmingCharacters(in: .whitespaces).isEmpty else {
            return Emoji.matching(query, most: 270).map(\.emoji)
        }
        var seen = Set(recents)
        return recents + Self.ordered.filter { seen.insert($0).inserted }
    }

    /// Every emoji with faces and hands first, as people look for them, then the rest in Unicode's
    /// order.
    static let ordered: [String] = {
        let first: [ClosedRange<UInt32>] = [0x1F600 ... 0x1F64F, 0x1F910 ... 0x1F92F, 0x1F970 ... 0x1F97F, 0x1F9D0 ... 0x1F9DF,
                                            0x1F440 ... 0x1F450, 0x1F466 ... 0x1F487, 0x1F90C ... 0x1F90F, 0x1F918 ... 0x1F91F]
        func rank(_ e: String) -> Int {
            guard let v = e.unicodeScalars.first?.value else { return first.count }
            return first.firstIndex { $0.contains(v) } ?? first.count
        }
        return Emoji.all.map(\.emoji).enumerated().sorted { (rank($0.element), $0.offset) < (rank($1.element), $1.offset) }.map(\.element)
    }()

    /// An emoji in the chosen tone, when it takes one.
    public func toned(_ emoji: String) -> String {
        guard tone > 0, let first = emoji.unicodeScalars.first, first.properties.isEmojiModifierBase,
              let modifier = Unicode.Scalar(0x1F3FB + UInt32(tone - 1))
        else { return emoji }
        return String(first) + String(modifier) + String(emoji.unicodeScalars.dropFirst().map(Character.init))
    }

    /// The arrows: across by one, up and down by a row, staying on the grid.
    public func move(_ dx: Int, _ dy: Int) {
        let n = shown.count
        guard n > 0 else { return }
        index = min(max(index + dx + dy * Self.columns, 0), n - 1)
    }

    /// The emoji the arrows have reached, in its tone, kept first among the recents.
    public func pick(_ at: Int? = nil) -> String? {
        let all = shown
        let i = at ?? index
        guard all.indices.contains(i) else { return nil }
        let base = all[i]
        recents.removeAll { $0 == base }
        recents.insert(base, at: 0)
        recents = Array(recents.prefix(Self.mostRecents))
        defaults.set(recents, forKey: "emoji.recent")
        return toned(base)
    }
}

/// The emoji picker: a field, the five tones, and a grid of emoji the arrows move through; Return
/// adds the emoji and keeps the picker open, Esc closes it.
struct EmojiPickerView: View {
    @Bindable var model: EmojiPickerModel
    let roles: Roles
    let picked: (String) -> Void
    @FocusState var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: Space.m) {
            HStack(spacing: Space.s) {
                Image(systemName: "magnifyingglass").foregroundStyle(roles.secondary.color)
                TextField("Search emoji", text: $model.query)
                    .textFieldStyle(.plain)
                    .textStyle(.body)
                    .focused($focused)
                    .onSubmit { if let e = model.pick() { picked(e) } }
                Spacer()
                ForEach(0 ..< 6, id: \.self) { t in
                    Text(t == 0 ? "✋" : "✋" + String(Unicode.Scalar(0x1F3FB + UInt32(t - 1))!))
                        .textStyle(.heading)
                        .padding(2)
                        .background(Circle().fill(t == model.tone ? roles.selection.color : .clear))
                        .onTapGesture { model.tone = t }
                        .help(["No skin tone", "Light", "Medium light", "Medium", "Medium dark", "Dark"][t])
                }
            }
            let all = model.shown
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVGrid(columns: Array(repeating: GridItem(.fixed(32), spacing: 2), count: EmojiPickerModel.columns), spacing: 2) {
                        ForEach(Array(all.enumerated()), id: \.offset) { i, e in
                            Text(model.toned(e))
                                .textStyle(.emoji)
                                .frame(width: 32, height: 32)
                                .background(RoundedRectangle(cornerRadius: Radius.small).fill(i == model.index ? roles.selection.color : .clear))
                                .onTapGesture { if let e = model.pick(i) { picked(e) } }
                                .id(i)
                        }
                    }
                }
                .onChange(of: model.index) { proxy.scrollTo(model.index) }
            }
            Text(model.query.isEmpty && !model.recents.isEmpty ? "recents first · ↩ adds it · Esc closes" : "↩ adds it · Esc closes")
                .textStyle(.micro).tone(.tertiary, roles)
        }
        .padding(Space.l)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .panel(roles)
        .onAppear { focused = true }
    }
}

/// The emoji picker's panel, over the box above its button.
@MainActor
public final class EmojiPicker {
    public static let shared = EmojiPicker()
    let model = EmojiPickerModel()
    public let panel = FloatingPanel()

    /// Shows the picker above `button`, each emoji chosen given to `picked`.
    public func show(above button: NSView, roles: Roles, picked: @escaping (String) -> Void) {
        guard let window = button.window else { return }
        if panel.isVisible { return panel.close() }
        model.query = ""
        let host = NSHostingView(rootView: EmojiPickerView(model: model, roles: roles, picked: picked))
        host.sizingOptions = []
        panel.keys = { [weak self] event in
            guard let self else { return false }
            switch event.keyCode {
            case 123: model.move(-1, 0)
            case 124: model.move(1, 0)
            case 125: model.move(0, 1)
            case 126: model.move(0, -1)
            default: return false
            }
            return true
        }
        let anchor = window.convertToScreen(button.convert(button.bounds, to: nil))
        panel.show(host, size: NSSize(width: 360, height: 330), over: window, at: .above(anchor))
    }
}
