import AppKit
import JawKit
import JawThemes

@MainActor
protocol ComposerDelegate: AnyObject {
    func composer(_ c: ComposerController, send text: String, key: String, reply: String?, mentions: [String])
    func composer(_ c: ComposerController, edit message: String, text: String)
    func composer(_ c: ComposerController, draft text: String)
    func composer(_ c: ComposerController, typing state: TypingState)
    func composerStepIntoMessages(_ c: ComposerController)
    /// A key while a message is selected; true when the conversation took it.
    func composer(_ c: ComposerController, key event: NSEvent) -> Bool
    func composerScroll(_ c: ComposerController, pages: Int)
}

/// The box, one per conversation, naming its chat. Enter sends, ⇧↩ and ⌥↩ add a line, ↑ from an
/// empty box steps into the messages, `@` completes the group's members and `:` emoji. Drafts go to
/// jawd once typing has paused for 1.5 s and on leaving; typing is said as the terminal says it.
@MainActor
public final class ComposerController: NSViewController, NSTextViewDelegate {
    weak var delegate: ComposerDelegate?
    var chat: ChatRow
    let textView = BoxTextView(usingTextLayoutManager: true)
    let scroll = NSScrollView()
    let bar = Labels.make("", .caption)
    lazy var barClose = IconButton(symbol: "xmark.circle.fill", label: "Drop it", shortcut: Shortcut("⎋", []), size: .small) { [weak self] in
        self?.dropMode()
    }

    let placeholder = Labels.make("", .body)
    let completions = CompletionView()
    var heightConstraint: NSLayoutConstraint!

    /// What the box is doing besides writing a new message.
    enum Mode: Equatable {
        case writing
        case replying(MessageRow)
        case editing(MessageRow, draft: String)
    }

    var mode: Mode = .writing {
        didSet { showBar() }
    }

    var members: [ChatInfo.Member] = []
    /// `@` names taken from the completion, by what was written, for the mentions sent.
    var taken: [String: String] = [:]
    /// Why the last message didn't go, until the box changes.
    var unsentWhy: String?
    var selecting = false
    var roles: Roles? {
        didSet { colour() }
    }

    /// The text last sent to jawd as the draft, so a draft from elsewhere is taken only when the box
    /// hasn't changed since.
    var draftSent = ""
    var draftTimer: Timer?
    var lastTyping: Date?

    static let draftQuiet: TimeInterval = 1.5
    static let typingEvery: TimeInterval = 5
    static let mostLines = 8

    init(chat: ChatRow) {
        self.chat = chat
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func loadView() {
        let root = NSView()
        root.wantsLayer = true
        view = root
        textView.delegate = self
        textView.box = self
        textView.isRichText = false
        textView.allowsUndo = true
        textView.drawsBackground = false
        textView.textContainerInset = NSSize(width: 4, height: 6)
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isContinuousSpellCheckingEnabled = true
        textView.font = TextStyle.body.nsFont
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true
        textView.setAccessibilityLabel("Message to \(chat.name)")
        scroll.documentView = textView
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.drawsBackground = false
        completions.isHidden = true
        completions.picked = { [weak self] in self?.take($0) }

        for v in [bar, barClose, scroll, placeholder, completions] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            root.addSubview(v)
        }
        heightConstraint = scroll.heightAnchor.constraint(equalToConstant: 30)
        NSLayoutConstraint.activate([
            bar.topAnchor.constraint(equalTo: root.topAnchor, constant: 6),
            bar.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 20),
            bar.trailingAnchor.constraint(equalTo: barClose.leadingAnchor, constant: -6),
            barClose.centerYAnchor.constraint(equalTo: bar.centerYAnchor),
            barClose.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -18),
            scroll.topAnchor.constraint(equalTo: bar.bottomAnchor, constant: 4),
            scroll.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 12),
            scroll.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -12),
            scroll.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -10),
            heightConstraint,
            placeholder.leadingAnchor.constraint(equalTo: scroll.leadingAnchor, constant: 9),
            placeholder.centerYAnchor.constraint(equalTo: scroll.topAnchor, constant: 15),
            completions.leadingAnchor.constraint(equalTo: scroll.leadingAnchor),
            completions.bottomAnchor.constraint(equalTo: scroll.topAnchor, constant: -4),
            completions.widthAnchor.constraint(equalToConstant: 280),
        ])
        if let draft = chat.draft, !draft.isEmpty {
            textView.string = draft
            draftSent = draft
        }
        showBar()
        changed()
    }

    func colour() {
        guard let roles else { return }
        view.layer?.backgroundColor = roles.ground.cgColor
        Surface.dress(scroll, .field, roles: roles)
        textView.textColor = roles.text.nsColor
        textView.insertionPointColor = roles.accent.nsColor
        placeholder.textColor = roles.tertiary.nsColor
        bar.textColor = unsentWhy == nil ? roles.secondary.nsColor : roles.failed.nsColor
        barClose.roles = roles
        completions.roles = roles
    }

    // MARK: - What the box shows

    func showBar() {
        guard isViewLoaded else { return }
        var text: String?
        switch mode {
        case .writing: break
        case let .replying(m): text = "↩ Replying to \(m.mine ? "yourself" : m.from?.name ?? "them"): \(Self.summary(m))"
        case .editing: text = "✎ Editing · Enter saves, Esc stops"
        }
        if let why = unsentWhy { text = "Not sent: \(why)" }
        bar.stringValue = text ?? ""
        bar.isHidden = text == nil
        barClose.isHidden = mode == .writing && unsentWhy == nil
        placeholder.stringValue = "to \(chat.name)"
        colour()
    }

    public static func summary(_ m: MessageRow) -> String {
        let s = m.plain.isEmpty ? (MessageLayout.Builder.kindLine(m) ?? "") : m.plain
        return s.split(whereSeparator: \.isNewline).joined(separator: " ")
    }

    func resize() {
        guard let lm = textView.textLayoutManager else { return }
        lm.ensureLayout(for: lm.documentRange)
        let used = lm.usageBoundsForTextContainer.height + textView.textContainerInset.height * 2
        let line = (textView.font?.boundingRectForFont.height ?? 16)
        let most = line * CGFloat(Self.mostLines) + textView.textContainerInset.height * 2
        heightConstraint.constant = min(max(ceil(used), 30), most)
    }

    public func textDidChange(_: Notification) {
        changed()
        if unsentWhy != nil {
            unsentWhy = nil
            showBar()
        }
        scheduleDraft()
        sayTyping()
        completeIfAsked()
    }

    func changed() {
        placeholder.isHidden = !textView.string.isEmpty
        resize()
    }

    // MARK: - Drafts and typing

    func scheduleDraft() {
        if case .editing = mode { return }
        scheduleDraftNow()
    }

    func scheduleDraftNow() {
        draftTimer?.invalidate()
        draftTimer = Timer.scheduledTimer(withTimeInterval: Self.draftQuiet, repeats: false) { [weak self] _ in
            MainActor.assumeIsolated { self?.flushDraft() }
        }
    }

    func flushDraft() {
        draftTimer?.invalidate()
        draftTimer = nil
        if case .editing = mode { return }
        let text = textView.string
        guard text != draftSent else { return }
        draftSent = text
        delegate?.composer(self, draft: text)
    }

    func sayTyping() {
        let text = textView.string
        if text.isEmpty {
            if lastTyping != nil {
                lastTyping = nil
                delegate?.composer(self, typing: .paused)
            }
            return
        }
        if lastTyping.map({ Date().timeIntervalSince($0) >= Self.typingEvery }) ?? true {
            lastTyping = Date()
            delegate?.composer(self, typing: .composing)
        }
    }

    /// The chat's row changed: a draft typed in another interface comes into the box, unless the box
    /// has changed since its own last draft.
    func chatChanged(_ row: ChatRow) {
        chat = row
        placeholder.stringValue = "to \(row.name)"
        let theirs = row.draft ?? ""
        if case .writing = mode, textView.string == draftSent, theirs != draftSent, textView.window?.firstResponder !== textView {
            textView.string = theirs
            draftSent = theirs
            changed()
        }
    }

    func leave() {
        flushDraft()
        if lastTyping != nil {
            lastTyping = nil
            delegate?.composer(self, typing: .paused)
        }
    }

    // MARK: - Sending

    func send() {
        let text = textView.string
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        switch mode {
        case let .editing(m, draft):
            delegate?.composer(self, edit: m.id, text: text)
            textView.string = draft
            mode = .writing
        case let .replying(m):
            delegate?.composer(self, send: text, key: Jawd.key(), reply: m.id, mentions: mentions(in: text))
            textView.string = ""
            mode = .writing
        case .writing:
            delegate?.composer(self, send: text, key: Jawd.key(), reply: nil, mentions: mentions(in: text))
            textView.string = ""
        }
        draftSent = textView.string
        taken = [:]
        lastTyping = nil
        changed()
    }

    /// The people whose `@Name`, taken from the completion, is still in the text.
    func mentions(in text: String) -> [String] {
        taken.filter { text.contains("@" + $0.key) }.map(\.value).sorted()
    }

    func unsent(text: String, why: String) {
        if textView.string.isEmpty { textView.string = text } else { textView.string = text + "\n" + textView.string }
        unsentWhy = why
        showBar()
        changed()
    }

    func reply(to m: MessageRow) {
        mode = .replying(m)
        view.window?.makeFirstResponder(textView)
    }

    func edit(_ m: MessageRow) {
        let draft = textView.string
        mode = .editing(m, draft: draft)
        textView.string = m.plain
        changed()
        view.window?.makeFirstResponder(textView)
    }

    @objc func dropMode() {
        if unsentWhy != nil {
            unsentWhy = nil
        }
        if case let .editing(_, draft) = mode {
            textView.string = draft
            changed()
        }
        mode = .writing
    }

    // MARK: - Keys

    /// A key the box's text view gets first; true when it's been dealt with.
    func key(_ event: NSEvent) -> Bool {
        if selecting, delegate?.composer(self, key: event) == true { return true }
        if !completions.isHidden {
            switch event.keyCode {
            case 125: completions.move(1); return true
            case 126: completions.move(-1); return true
            case 36, 76, 48: completions.pick(); return true
            case 53: completions.isHidden = true; dismissedAt = completionStart; return true
            default: break
            }
        }
        let flags = event.modifierFlags.intersection([.shift, .option, .command, .control])
        switch event.keyCode {
        case 36, 76:
            if flags.contains(.shift) || flags.contains(.option) {
                textView.insertNewlineIgnoringFieldEditor(nil)
            } else if flags.isEmpty {
                send()
            } else {
                return false
            }
            return true
        case 126 where textView.string.isEmpty && flags.isEmpty:
            delegate?.composerStepIntoMessages(self)
            return true
        case 53:
            if mode != .writing || unsentWhy != nil {
                dropMode()
                return true
            }
            return false
        case 116:
            delegate?.composerScroll(self, pages: -1)
            return true
        case 121:
            delegate?.composerScroll(self, pages: 1)
            return true
        default:
            return false
        }
    }

    // MARK: - Completing @ names and : emoji

    var completionStart: Int?
    var dismissedAt: Int?

    enum Completing { case name, emoji }
    var completing: Completing?

    func completeIfAsked() {
        let text = textView.string as NSString
        let cursor = textView.selectedRange().location
        guard cursor <= text.length else { return }
        let before = text.substring(to: cursor)
        guard let (trigger, start, query) = Self.token(before) else {
            completions.isHidden = true
            return
        }
        guard start != dismissedAt else { return }
        completionStart = start
        switch trigger {
        case "@" where chat.kind.isGroup:
            completing = .name
            let found = Self.names(members, matching: query)
            completions.show(found.map { .init(title: "@" + $0.name, detail: $0.phone.map { "··· " + String($0.suffix(4)) }, value: $0.person) })
        case ":" where query.count >= 2:
            completing = .emoji
            completions.show(Emoji.matching(query, most: 6).map { .init(title: "\($0.emoji)  \($0.name)", detail: nil, value: $0.emoji) })
        default:
            completions.isHidden = true
        }
    }

    /// The `@` or `:` being typed before the cursor: after a space or at the start, with no space in
    /// what follows, at most 24 characters.
    static func token(_ before: String) -> (Character, Int, String)? {
        let utf16 = Array(before.utf16)
        var i = utf16.count - 1
        while i >= 0, utf16.count - i <= 25 {
            let c = utf16[i]
            if c == 0x20 || c == 0x0A { return nil }
            if c == 0x40 || c == 0x3A {
                if i > 0, utf16[i - 1] != 0x20, utf16[i - 1] != 0x0A { return nil }
                let query = String(utf16CodeUnits: Array(utf16[(i + 1)...]), count: utf16.count - i - 1)
                return (c == 0x40 ? "@" : ":", i, query)
            }
            i -= 1
        }
        return nil
    }

    static func names(_ members: [ChatInfo.Member], matching query: String) -> [ChatInfo.Member] {
        let q = query.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
        return members.filter { m in
            q.isEmpty || m.name.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil)
                .split(separator: " ").contains { $0.hasPrefix(q) }
        }.prefix(5).map(\.self)
    }

    func take(_ item: CompletionView.Item) {
        guard let start = completionStart else { return }
        let cursor = textView.selectedRange().location
        let range = NSRange(location: start, length: cursor - start)
        switch completing {
        case .name:
            let name = String(item.title.dropFirst())
            taken[name] = item.value
            textView.insertText("@\(name) ", replacementRange: range)
        case .emoji:
            textView.insertText(item.value, replacementRange: range)
        case nil: break
        }
        completions.isHidden = true
    }
}

/// The box's text view: its keys go to the box first.
final class BoxTextView: NSTextView {
    weak var box: ComposerController?

    override func keyDown(with event: NSEvent) {
        if box?.key(event) == true { return }
        super.keyDown(with: event)
    }

    var focused: (() -> Void)?

    override func becomeFirstResponder() -> Bool {
        let took = super.becomeFirstResponder()
        if took { focused?() }
        return took
    }
}

/// The `@` and `:` completions over the box.
final class CompletionView: NSView {
    struct Item {
        let title: String
        let detail: String?
        let value: String
    }

    var items: [Item] = []
    var index = 0
    var picked: ((Item) -> Void)?
    let stack = NSStackView()
    var roles: Roles? {
        didSet { redraw() }
    }

    init() {
        super.init(frame: .zero)
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 0
        stack.edgeInsets = NSEdgeInsets(top: Space.xs, left: 0, bottom: Space.xs, right: 0)
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func show(_ found: [Item]) {
        items = found
        index = 0
        isHidden = found.isEmpty
        redraw()
    }

    func move(_ by: Int) {
        guard !items.isEmpty else { return }
        index = (index + by + items.count) % items.count
        redraw()
    }

    func pick() {
        guard index < items.count else { return }
        picked?(items[index])
    }

    func redraw() {
        for v in stack.arrangedSubviews {
            v.removeFromSuperview()
        }
        guard let roles else { return }
        Surface.dress(self, .popover, radius: Radius.control, roles: roles)
        for (i, item) in items.enumerated() {
            let title = Labels.make(item.title, .body, .primary, roles: roles)
            let detail = Labels.make(item.detail ?? "", .caption, .secondary, roles: roles)
            let row = NSStackView(views: [title, detail])
            row.edgeInsets = NSEdgeInsets(top: Space.xxs + 1, left: Space.l - 2, bottom: Space.xxs + 1, right: Space.l - 2)
            row.wantsLayer = true
            row.layer?.backgroundColor = i == index ? roles.selection.cgColor : nil
            row.widthAnchor.constraint(equalToConstant: 280).isActive = true
            stack.addArrangedSubview(row)
        }
    }
}

/// Emoji by name, from Unicode's own names, for `:` in the box and the palette's reactions.
public enum Emoji {
    public struct Named: Sendable {
        public let emoji: String
        public let name: String
    }

    static let all: [Named] = {
        var out: [Named] = []
        for range in [0x1F300 ... 0x1FAFF, 0x2600 ... 0x27BF] {
            for v in range {
                guard let s = Unicode.Scalar(v), s.properties.isEmojiPresentation, let name = s.properties.name else { continue }
                out.append(Named(emoji: String(s), name: name.lowercased()))
            }
        }
        return out
    }()

    /// Emoji whose name has a word starting with each word typed.
    public static func matching(_ query: String, most: Int) -> [Named] {
        let words = query.lowercased().split(whereSeparator: { $0 == "_" || $0 == " " || $0 == "-" })
        guard !words.isEmpty else { return [] }
        return Array(all.lazy.filter { e in
            let parts = e.name.split(separator: " ")
            return words.allSatisfy { w in parts.contains { $0.hasPrefix(w) } }
        }.prefix(most))
    }
}
