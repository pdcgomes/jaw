import AppKit
import JawKit
import JawThemes

@MainActor
protocol ComposerDelegate: AnyObject {
    func composer(_ c: ComposerController, send text: String, key: String, reply: String?, mentions: [String])
    /// The strip's files, what was typed being the first one's caption.
    func composer(_ c: ComposerController, send files: [AttachedFile], caption: String, hd: Bool, key: String, reply: String?)
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
/// Files attached, picked, pasted or dropped, wait in the strip above the field and go with the next
/// message, what's typed being the first one's caption: Tab switches them between SD and HD, and Esc
/// takes them out.
@MainActor
public final class ComposerController: NSViewController, NSTextViewDelegate {
    weak var delegate: ComposerDelegate?
    var chat: ChatRow
    let textView = BoxTextView(usingTextLayoutManager: true)
    let scroll = NSScrollView()
    /// The field's ground and border, behind the text.
    let field = Surface(.field, radius: Radius.field)
    let bar = Labels.make("", .caption)
    lazy var barClose = IconButton(symbol: "xmark.circle.fill", label: "Drop it", shortcut: Shortcut("⎋", []), size: .small) { [weak self] in
        self?.dropMode()
    }

    let placeholder = Labels.make("Message", .body)
    /// The chat this box writes to, always shown at the start of the field.
    let chip = ChatChip()
    var avatar: NSImage? {
        didSet { showChip() }
    }
    let completions = CompletionView()
    lazy var plus = IconButton(symbol: "plus", label: "Attach, or make a poll or an event", tone: .secondary) {}
    lazy var emojiButton = IconButton(symbol: "face.smiling", label: "Emoji", shortcut: Shortcut("e", [.control, .command]), size: .small,
                                      tone: .secondary) { [weak self] in self?.showEmoji() }
    lazy var gifButton = IconButton(symbol: "", label: "GIFs", shortcut: Shortcut("g", [.control, .command]), size: .small, tone: .secondary,
                                    title: "GIF") { [weak self] in self?.gif?() }
    lazy var sendButton = IconButton(symbol: "arrow.up", label: "Send", shortcut: Shortcut("↩", [])) { [weak self] in
        self?.send(hd: NSEvent.modifierFlags.contains(.option))
    }

    /// The files waiting to go with the next message, in the strip above the field.
    public internal(set) var attachments = Attachments() {
        didSet { showStrip() }
    }

    /// Whether files attached to an empty strip go in HD, as Settings has it.
    public var hdByDefault = false
    /// What jawd says sending videos, GIFs and audio files needs, when its `status` says it has no
    /// ffmpeg; nil when it has it.
    public var ffmpegMissing: String? {
        didSet { showStrip() }
    }

    /// Where an image pasted or dropped that isn't a file, or a file another app promises, is put
    /// before it's attached: the run's own folder, emptied as the app quits.
    public var attachedFolder = Opened.folder.appending(path: "attached", directoryHint: .isDirectory)
    let strip = AttachmentStrip()
    /// What the box says in its bar until it changes, as why files were left out.
    var said: String?
    var stripTop: NSLayoutConstraint?
    /// The + button's menu: attachments, a poll, an event, a place.
    public var plusMenu: (() -> NSMenu?)? {
        didSet { plus.menuToOpen = plusMenu }
    }

    /// The GIF button.
    public var gif: (() -> Void)?

    public func showGIFs() {
        gif?()
    }
    /// The field's trailing edge: before Send while it shows, at the box's edge when it doesn't.
    var fieldToSend: NSLayoutConstraint!
    var fieldToEdge: NSLayoutConstraint!
    var heightConstraint: NSLayoutConstraint!
    /// The field's top: just under the panes, or under the strip while there is one, so a box with
    /// nothing to say above it keeps no room for it.
    var scrollTop: NSLayoutConstraint!

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
        let root = BoxView()
        root.box = self
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
        Scrolling.style(scroll)
        scroll.autohidesScrollers = true
        scroll.drawsBackground = false
        completions.isHidden = true
        completions.picked = { [weak self] in self?.take($0) }

        sendButton.filled = true
        strip.isHidden = true
        strip.removed = { [weak self] i in self?.takeOut(at: i) }
        strip.switched = { [weak self] in self?.attachments.hd.toggle() }
        for v in [bar, barClose, strip, field, plus, chip, scroll, emojiButton, gifButton, sendButton, placeholder, completions] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            root.addSubview(v)
        }
        heightConstraint = scroll.heightAnchor.constraint(equalToConstant: 30)
        fieldToSend = field.trailingAnchor.constraint(equalTo: sendButton.leadingAnchor, constant: -Space.s)
        fieldToEdge = field.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -Space.l)
        scrollTop = scroll.topAnchor.constraint(equalTo: root.topAnchor, constant: Space.s)
        NSLayoutConstraint.activate([
            bar.topAnchor.constraint(equalTo: root.topAnchor, constant: 6),
            bar.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 20),
            bar.trailingAnchor.constraint(equalTo: barClose.leadingAnchor, constant: -6),
            barClose.centerYAnchor.constraint(equalTo: bar.centerYAnchor),
            barClose.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -18),
            strip.leadingAnchor.constraint(equalTo: field.leadingAnchor),
            strip.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -Space.m - 2),
            scrollTop,
            plus.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: Space.m),
            plus.centerYAnchor.constraint(equalTo: scroll.topAnchor, constant: 15),
            field.leadingAnchor.constraint(equalTo: plus.trailingAnchor, constant: Space.xs),
            chip.leadingAnchor.constraint(equalTo: field.leadingAnchor, constant: Space.xs),
            chip.centerYAnchor.constraint(equalTo: scroll.topAnchor, constant: 15),
            scroll.leadingAnchor.constraint(equalTo: chip.trailingAnchor, constant: Space.xxs),
            scroll.trailingAnchor.constraint(equalTo: emojiButton.leadingAnchor, constant: -Space.xxs),
            emojiButton.centerYAnchor.constraint(equalTo: scroll.topAnchor, constant: 15),
            gifButton.leadingAnchor.constraint(equalTo: emojiButton.trailingAnchor, constant: Space.xxs),
            gifButton.centerYAnchor.constraint(equalTo: scroll.topAnchor, constant: 15),
            gifButton.trailingAnchor.constraint(equalTo: field.trailingAnchor, constant: -Space.xs),
            sendButton.centerYAnchor.constraint(equalTo: scroll.topAnchor, constant: 15),
            sendButton.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -Space.m - 2),
            scroll.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -Space.m),
            heightConstraint,
            field.topAnchor.constraint(equalTo: scroll.topAnchor),
            field.bottomAnchor.constraint(equalTo: scroll.bottomAnchor),

            placeholder.leadingAnchor.constraint(equalTo: scroll.leadingAnchor, constant: 9),
            placeholder.centerYAnchor.constraint(equalTo: scroll.topAnchor, constant: 15),
            completions.leadingAnchor.constraint(equalTo: field.leadingAnchor),
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
        field.roles = roles
        textView.textColor = roles.text.nsColor
        textView.insertionPointColor = roles.accent.nsColor
        placeholder.textColor = roles.tertiary.nsColor
        showChip()
        bar.textColor = unsentWhy == nil ? roles.secondary.nsColor : roles.failed.nsColor
        barClose.roles = roles
        for b in [plus, emojiButton, gifButton, sendButton] {
            b.roles = roles
        }
        completions.roles = roles
        strip.roles = roles
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
        if let said { text = said }
        if let why = unsentWhy { text = "Not sent: \(why)" }
        bar.stringValue = text ?? ""
        bar.isHidden = text == nil
        placeField()
        barClose.isHidden = mode == .writing && unsentWhy == nil && said == nil
        showChip()
        colour()
    }

    /// The field's top: under the strip while it shows, which goes under the bar while there's
    /// something to say, or at the box's top.
    func placeField() {
        let top = bar.isHidden ? view.topAnchor : bar.bottomAnchor
        let gap = bar.isHidden ? Space.s : Space.xs
        stripTop?.isActive = false
        scrollTop.isActive = false
        if strip.isHidden {
            scrollTop = scroll.topAnchor.constraint(equalTo: top, constant: gap)
        } else {
            let placed = strip.topAnchor.constraint(equalTo: top, constant: gap)
            placed.isActive = true
            stripTop = placed
            scrollTop = scroll.topAnchor.constraint(equalTo: strip.bottomAnchor, constant: Space.s)
        }
        scrollTop.isActive = true
    }

    /// The strip as the files are: hidden with none, and saying when jawd can't send one of them.
    func showStrip() {
        guard isViewLoaded else { return }
        let missing = attachments.needsFFmpeg ? ffmpegMissing : nil
        strip.show(attachments, note: missing.map { "jawd has no ffmpeg, which \($0) needs (brew install ffmpeg)" })
        if strip.isHidden != attachments.isEmpty {
            strip.isHidden = attachments.isEmpty
            placeField()
        }
        showSend()
    }

    func showChip() {
        guard isViewLoaded, let roles else { return }
        chip.show(chat, image: avatar, roles: roles)
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
        if unsentWhy != nil || said != nil {
            unsentWhy = nil
            said = nil
            showBar()
        }
        scheduleDraft()
        sayTyping()
        completeIfAsked()
    }

    func changed() {
        placeholder.isHidden = !textView.string.isEmpty
        showSend()
        resize()
    }

    /// Send shows while there's something to send, words or files; a microphone will take its place
    /// when the box is empty, once jawd records voice (M5).
    func showSend() {
        guard isViewLoaded, fieldToSend != nil else { return }
        let something = !textView.string.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !attachments.isEmpty
        guard sendButton.isHidden == something || !(fieldToSend.isActive || fieldToEdge.isActive) else { return }
        sendButton.isHidden = !something
        fieldToEdge.isActive = !something
        fieldToSend.isActive = something
    }

    public func showEmoji() {
        guard let roles else { return }
        EmojiPicker.shared.show(above: emojiButton, roles: roles) { [weak self] emoji in
            guard let self else { return }
            view.window?.makeFirstResponder(textView)
            textView.insertText(emoji, replacementRange: textView.selectedRange())
        }
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
        showChip()
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

    /// Sends what's in the box: an edit saved, the strip's files waiting meanwhile; the strip's files,
    /// what's typed the first one's caption, in HD when the strip says so or ⌥ is `held`; or the words.
    func send(hd held: Bool = false) {
        let text = textView.string
        let editing = if case .editing = mode { true } else { false }
        if !editing, !attachments.isEmpty {
            let reply: String? = if case let .replying(m) = mode { m.id } else { nil }
            delegate?.composer(self, send: attachments.files, caption: text, hd: attachments.hd || held, key: Jawd.key(), reply: reply)
            attachments = Attachments(hd: hdByDefault)
            textView.string = ""
            said = nil
            mode = .writing
        } else {
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

    // MARK: - Attaching

    /// Puts files in the strip after those there, to go with the next message, as many as fit; while
    /// a message is being edited they're turned down, since files go with a new one.
    public func attach(_ files: [AttachedFile]) {
        guard !files.isEmpty else { return }
        if case .editing = mode {
            AttachedFile.discard(files)
            return say("files go with a new message: Esc stops editing first")
        }
        var more = attachments
        if more.isEmpty { more.hd = hdByDefault }
        let left = more.add(files)
        attachments = more
        if left > 0 {
            AttachedFile.discard(files.filter { !attachments.files.contains($0) })
            say("\(SendMedia.filesMost) files at most at once: \(left) left out")
        }
        view.window?.makeFirstResponder(textView)
    }

    /// Files jawd turned down come back to the strip, before any attached since, with what was typed
    /// and why.
    public func unsent(files: [AttachedFile], caption: String, why: String) {
        var back = Attachments(hd: attachments.isEmpty ? hdByDefault : attachments.hd)
        back.add(files)
        back.add(attachments.files)
        attachments = back
        if caption.isEmpty {
            unsentWhy = why
            showBar()
        } else {
            unsent(text: caption, why: why)
        }
    }

    /// The strip's × on the file at `i`.
    func takeOut(at i: Int) {
        guard attachments.files.indices.contains(i) else { return }
        AttachedFile.discard([attachments.files[i]])
        attachments.remove(at: i)
    }

    /// Says `words` in the bar until the box changes.
    func say(_ words: String) {
        said = words
        showBar()
    }

    /// What ⌘V brings, attached when it's files or an image; false for words, which the field pastes.
    func attachPasted(from board: NSPasteboard) -> Bool {
        guard let files = Pasted.attachments(from: board, into: attachedFolder) else { return false }
        attach(files)
        return true
    }

    /// What's dropped on the box, attached when it's files, files another app promises, or an image;
    /// false for words, which the field takes.
    func attachDropped(from board: NSPasteboard) -> Bool {
        if let files = Pasted.files(board) {
            attach(files)
            return true
        }
        let promises = board.readObjects(forClasses: [NSFilePromiseReceiver.self]) as? [NSFilePromiseReceiver] ?? []
        if !promises.isEmpty {
            receive(promises)
            return true
        }
        guard !Pasted.hasText(board), let image = Pasted.image(board, into: attachedFolder) else { return false }
        attach([image])
        return true
    }

    /// Files another app promises, as Photos and browsers drag theirs out: each received into a
    /// folder of its own and attached as it comes.
    func receive(_ promises: [NSFilePromiseReceiver]) {
        let take: @MainActor (AttachedFile) -> Void = { [weak self] in self?.attach([$0]) }
        let refused: @MainActor (String) -> Void = { [weak self] in self?.say("a file dropped didn't come: \($0)") }
        for promise in promises {
            let folder = attachedFolder.appending(path: UUID().uuidString, directoryHint: .isDirectory)
            guard (try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)) != nil else { continue }
            promise.receivePromisedFiles(atDestination: folder, options: [:], operationQueue: Self.receiving) { url, error in
                let why = error?.localizedDescription
                DispatchQueue.main.async {
                    MainActor.assumeIsolated {
                        if let why { refused(why) } else { take(AttachedFile(url: url, written: true)) }
                    }
                }
            }
        }
    }

    static let receiving: OperationQueue = {
        let q = OperationQueue()
        q.qualityOfService = .userInitiated
        return q
    }()

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
        said = nil
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
        case 48 where flags.isEmpty && attachments.showsQuality:
            attachments.hd.toggle()
            return true
        case 53:
            if mode != .writing || unsentWhy != nil || said != nil {
                dropMode()
                return true
            }
            if !attachments.isEmpty {
                AttachedFile.discard(attachments.files)
                attachments.removeAll()
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

/// The box's text view: its keys go to the box first, and what's pasted or dropped on it that's files
/// or an image goes to the strip.
final class BoxTextView: NSTextView {
    weak var box: ComposerController?

    override func keyDown(with event: NSEvent) {
        if box?.key(event) == true { return }
        super.keyDown(with: event)
    }

    override func paste(_ sender: Any?) {
        if box?.attachPasted(from: .general) == true { return }
        super.paste(sender)
    }

    override var acceptableDragTypes: [NSPasteboard.PasteboardType] {
        let words = super.acceptableDragTypes
        return words + BoxView.dropped.filter { !words.contains($0) }
    }

    override func dragOperation(for info: any NSDraggingInfo, type: NSPasteboard.PasteboardType) -> NSDragOperation {
        BoxView.attaches(info.draggingPasteboard) ? .copy : super.dragOperation(for: info, type: type)
    }

    override func performDragOperation(_ sender: any NSDraggingInfo) -> Bool {
        if BoxView.attaches(sender.draggingPasteboard), box?.attachDropped(from: sender.draggingPasteboard) == true { return true }
        return super.performDragOperation(sender)
    }

    var focused: (() -> Void)?

    override func becomeFirstResponder() -> Bool {
        let took = super.becomeFirstResponder()
        if took { focused?() }
        return took
    }
}

/// The box's own view, which takes files, files promised and images dropped on it around the field,
/// as the field does.
final class BoxView: NSView {
    weak var box: ComposerController?

    static let dropped: [NSPasteboard.PasteboardType] = [.fileURL, .png, .tiff]
        + NSFilePromiseReceiver.readableDraggedTypes.map { NSPasteboard.PasteboardType($0) }

    /// Whether a drag brings what the strip takes, rather than words.
    static func attaches(_ board: NSPasteboard) -> Bool {
        Pasted.files(board) != nil || board.canReadObject(forClasses: [NSFilePromiseReceiver.self], options: nil)
            || !Pasted.hasText(board) && board.availableType(from: [.png, .tiff]) != nil
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        registerForDraggedTypes(Self.dropped)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func draggingEntered(_ sender: any NSDraggingInfo) -> NSDragOperation {
        Self.attaches(sender.draggingPasteboard) ? .copy : []
    }

    override func draggingUpdated(_ sender: any NSDraggingInfo) -> NSDragOperation {
        Self.attaches(sender.draggingPasteboard) ? .copy : []
    }

    override func performDragOperation(_ sender: any NSDraggingInfo) -> Bool {
        box?.attachDropped(from: sender.draggingPasteboard) ?? false
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
