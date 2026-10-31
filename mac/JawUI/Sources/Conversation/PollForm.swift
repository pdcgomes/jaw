import JawKit
import JawThemes
import Observation
import SwiftUI

/// The poll form, as the box design has it: a question, options added with Return, twelve at most as
/// WhatsApp allows, and "allow several answers". It's ready once it's a poll jawd takes, trimmed and
/// counted as jawd trims and counts them, and says the first thing in the way until then.
@MainActor @Observable
public final class PollForm {
    /// A field of the form: the question, or an option's by its number in `ids`.
    public enum Field: Hashable, Sendable {
        case question, option(Int)
    }

    /// What Return did in a field.
    public enum Returned: Equatable, Sendable {
        case moved, added, send, notReady
    }

    public static let most = 12
    public static let questionMost = 255
    public static let optionMost = 100

    /// The chat it's to, by name.
    public let chat: String
    public var question: String { didSet { said = nil } }
    public var options: [String] {
        didSet {
            said = nil
            while ids.count < options.count {
                ids.append(next)
                next += 1
            }
            if ids.count > options.count { ids.removeLast(ids.count - options.count) }
        }
    }

    public var several: Bool { didSet { said = nil } }
    /// What jawd said when it turned this poll down, until it's changed.
    public var said: String?
    /// The field that has the keys.
    public var focus: Field?
    /// Each option's own number, which stays with it as others are added and taken out, so each field
    /// keeps its option.
    public private(set) var ids: [Int]
    @ObservationIgnored var next: Int
    /// The key the poll goes with, made with the form, so jawd takes a second Send of it for the same
    /// poll rather than another.
    public let key = Jawd.key()
    /// Whether Send has taken the poll, which it does once.
    public private(set) var sent = false

    public init(chat: String, question: String = "", options: [String] = ["", ""], several: Bool = false, said: String? = nil) {
        let options = options.count < 2 ? options + Array(repeating: "", count: 2 - options.count) : options
        self.chat = chat
        self.question = question
        self.options = options
        self.several = several
        self.said = said
        ids = Array(options.indices)
        next = options.count
    }

    /// A poll jawd turned down, as it was, with jawd's words.
    public convenience init(chat: String, returned poll: SendPoll, said: String) {
        self.init(chat: chat, question: poll.question, options: poll.options, several: poll.max > 1, said: said)
    }

    static func trimmed(_ s: String) -> String {
        s.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// The options with words, trimmed, in their order.
    var named: [String] {
        options.map(Self.trimmed).filter { !$0.isEmpty }
    }

    /// The first thing in the way of sending it, in jaw's words; nil once it's ready.
    public var problem: String? {
        let q = Self.trimmed(question)
        if q.isEmpty { return "Ask a question" }
        if q.unicodeScalars.count > Self.questionMost { return "A question has \(Self.questionMost) characters at most" }
        let named = named
        if named.count < 2 { return "Give it two options at least" }
        if named.count > Self.most { return "A poll has \(Self.most) options at most" }
        if named.contains(where: { $0.unicodeScalars.count > Self.optionMost }) { return "An option has \(Self.optionMost) characters at most" }
        if let twice = named.enumerated().first(where: { named[..<$0.offset].contains($0.element) })?.element {
            return "\(twice) is there twice, and a vote names an option by its name"
        }
        return nil
    }

    /// The poll as it goes, once it's ready: its question and options trimmed, the empty options left
    /// out, and every option a vote's most when it allows several answers.
    public var poll: (question: String, options: [String], max: Int)? {
        guard problem == nil else { return nil }
        let named = named
        return (Self.trimmed(question), named, several ? named.count : 1)
    }

    /// The poll for Send to send with `key`, the first time it asks once the form is ready, and nil
    /// after: a second click, Return or ⌘↩ that reaches the sheet as it closes sends nothing more.
    public func send() -> (question: String, options: [String], max: Int)? {
        guard !sent, let poll else { return nil }
        sent = true
        return poll
    }

    /// Return in a field: from the question to the first option; from an option with words to the
    /// next, adding one after the last while there are fewer than twelve; and from an empty option, or
    /// the twelfth, the poll goes once it's ready.
    public func returned(in field: Field) -> Returned {
        switch field {
        case .question:
            if let first = ids.first { focus = .option(first) }
            return .moved
        case let .option(id):
            let i = ids.firstIndex(of: id)
            let words = i.map { !Self.trimmed(options[$0]).isEmpty } ?? false
            if words, let i, i + 1 < options.count {
                focus = .option(ids[i + 1])
                return .moved
            }
            if words, options.count < Self.most {
                add()
                return .added
            }
            return problem == nil ? .send : .notReady
        }
    }

    /// Another option, after the last, which takes the keys.
    public func add() {
        guard options.count < Self.most else { return }
        let id = next
        ids.append(id)
        next += 1
        options.append("")
        focus = .option(id)
    }

    /// Takes an option out, two staying at least; when its field had the keys, they go to the option
    /// in its place, or to the last when it was the last.
    public func remove(at i: Int) {
        guard options.count > 2, options.indices.contains(i) else { return }
        let gone = ids.remove(at: i)
        options.remove(at: i)
        if focus == .option(gone) { focus = .option(ids[min(i, ids.count - 1)]) }
    }

    /// The words of option `id`, for its field; none once it's been taken out.
    public func words(of id: Int) -> String {
        ids.firstIndex(of: id).map { options[$0] } ?? ""
    }

    /// The words typed in option `id`'s field, written unless the option has been taken out: AppKit
    /// ends the editing of a field whose option went while it had the keys, and SwiftUI then writes
    /// what the field held.
    public func write(_ words: String, to id: Int) {
        guard let i = ids.firstIndex(of: id), options[i] != words else { return }
        options[i] = words
    }
}

/// The poll form's sheet: the chat it's to, the question, the options with an × past the second and
/// Add Option, "Allow several answers", what's in the way or jawd's words, and Cancel and Send. Tab
/// moves, Return does what `PollForm.returned(in:)` says, ⌘↩ sends from anywhere, and Esc cancels.
public struct PollFormView: View {
    @Bindable var model: PollForm
    let roles: Roles
    let send: () -> Void
    let cancel: () -> Void
    @FocusState var focused: PollForm.Field?

    public init(model: PollForm, roles: Roles, send: @escaping () -> Void, cancel: @escaping () -> Void) {
        self.model = model
        self.roles = roles
        self.send = send
        self.cancel = cancel
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: Space.l) {
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text("New Poll").textStyle(.heading).tone(.primary, roles)
                Text("to \(model.chat)").textStyle(.callout).tone(.secondary, roles)
            }
            TextField("Ask a question", text: $model.question, axis: .vertical)
                .lineLimit(1 ... 3)
                .textFieldStyle(.roundedBorder)
                .focused($focused, equals: .question)
                .onSubmit { returned(.question) }
            VStack(alignment: .leading, spacing: Space.s) {
                Text("Options").textStyle(.caption).tone(.secondary, roles)
                ForEach(Array(model.ids.enumerated()), id: \.element) { i, id in
                    HStack(spacing: Space.s) {
                        TextField("Option \(i + 1)", text: Binding(get: { model.words(of: id) }, set: { model.write($0, to: id) }))
                            .textFieldStyle(.roundedBorder)
                            .focused($focused, equals: .option(id))
                            .onSubmit { returned(.option(id)) }
                        if model.options.count > 2 {
                            Button {
                                if let at = model.ids.firstIndex(of: id) { model.remove(at: at) }
                            } label: { Image(systemName: "xmark.circle.fill").tone(.tertiary, roles) }
                                .buttonStyle(.borderless)
                                .help("Take this option out")
                        }
                    }
                }
                if model.options.count < PollForm.most {
                    Button { model.add() } label: { Label("Add Option", systemImage: "plus") }
                        .buttonStyle(.borderless)
                        .help("Another option, \(PollForm.most) at most")
                }
            }
            Toggle("Allow several answers", isOn: $model.several)
            HStack(spacing: Space.m) {
                Text(model.said.map { "Not sent: \($0)" } ?? model.problem ?? "Return adds an option, and sends from an empty one")
                    .textStyle(.caption).tone(model.said != nil ? .warn : .tertiary, roles)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer()
                Button("Cancel", action: cancel).keyboardShortcut(.cancelAction)
                Button("Send", action: send)
                    .keyboardShortcut(.return, modifiers: .command)
                    .buttonStyle(.borderedProminent)
                    .disabled(model.poll == nil || model.sent)
                    .help("Send (⌘↩)")
            }
        }
        .padding(Space.xxl)
        .frame(width: 460)
        .background(roles.ground.color)
        .onAppear { focused = model.focus ?? .question }
        .onChange(of: model.focus) { focused = model.focus }
        .onChange(of: focused) { model.focus = focused }
    }

    func returned(_ field: PollForm.Field) {
        if model.returned(in: field) == .send { send() }
    }
}
