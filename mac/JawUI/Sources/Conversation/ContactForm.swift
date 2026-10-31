import AppKit
import Contacts
import JawKit
import JawThemes
import Observation
import SwiftUI

/// The box's Contact…: the cards to send to a chat in one message, each made from one of the Mac's
/// contacts as Contacts exports it, without its photo, and shown as its message will draw it once
/// jawd's row has it; what's in the way of sending them, counted as jawd counts; and, when jawd turns
/// them down, its words, the form opening again as it was. ↑ and ↓ choose a card, ⌫ takes it out,
/// Return sends and Esc cancels.
@MainActor @Observable
public final class ContactForm {
    /// A card on the form: as it's sent, and as its message will draw it, read from its vCard as
    /// JawKit reads a row's.
    public struct Card: Sendable, Hashable {
        public let sent: SendContact.Card
        public let shown: ContactCard

        public nonisolated init(name: String, vcard: String) {
            sent = SendContact.Card(name: name, vcard: vcard)
            shown = ContactCard.read(vcard).first ?? ContactCard(name: name)
        }

        /// The name it's shown by: its card's, or failing that the one it's sent with.
        public var name: String {
            ContactWords.said(shown.name) ?? sent.name
        }
    }

    /// The chat the cards go to, and its name.
    public let chat: String
    public let to: String
    public private(set) var cards: [Card]
    public private(set) var chosen = 0
    /// jawd's words, when it turned the cards down.
    public var said: String?
    /// Why the contact picked last wasn't added.
    public var unadded: String?

    public init(chat: String, to: String, cards: [Card] = [], said: String? = nil) {
        self.chat = chat
        self.to = to
        self.cards = cards
        self.said = said
    }

    /// The cards as they're sent.
    public var sending: [SendContact.Card] {
        cards.map(\.sent)
    }

    /// What's in the way of sending the cards, counted as jawd counts them, in jaw's words; nil
    /// when nothing is, or there's nothing to send yet.
    public var problem: String? {
        switch SendContact.refusal(sending) {
        case nil, .noCards:
            nil
        case let .cards(n):
            "one message shares \(SendContact.cardsMost) cards at most: take \(n - SendContact.cardsMost) out"
        case let .vcards(bytes):
            "these cards take \((bytes + 1023) / 1024) KiB together, and one message takes \(SendContact.vcardsMost / 1024) KiB at most"
        case let .unnamed(i):
            "card \(i + 1) has no name to go by"
        case let .name(i, n):
            "\(cards[i].name.prefix(24).trimmingCharacters(in: .whitespaces))…'s name is \(n) characters, and a card's is \(SendContact.nameMost) at most"
        }
    }

    /// Whether Send can go.
    public var ready: Bool {
        !cards.isEmpty && problem == nil
    }

    /// What the form says under its cards: why a contact wasn't added, jawd's words, or what's in
    /// the way, each a warning; nil for none.
    public var warning: String? {
        unadded ?? said.map { "Not sent: \($0)" } ?? problem
    }

    /// Whether the picker opens with the form, as it does for a form with nothing on it yet.
    public var picksAtOnce: Bool {
        cards.isEmpty && said == nil
    }

    public var title: String {
        cards.count > 1 ? "Send \(cards.count) Contacts" : "Send a Contact"
    }

    /// Adds a card, chosen, unless the form has it already; jawd's words go, as they were about the
    /// cards before it.
    public func add(_ card: Card) {
        if cards.contains(where: { $0.sent == card.sent }) {
            unadded = "\(card.name) is on the form already"
            return
        }
        cards.append(card)
        chosen = cards.count - 1
        unadded = nil
        said = nil
    }

    public func remove(_ i: Int) {
        guard cards.indices.contains(i) else { return }
        cards.remove(at: i)
        chosen = min(chosen, max(cards.count - 1, 0))
        unadded = nil
        said = nil
    }

    public func up() {
        chosen = max(chosen - 1, 0)
    }

    public func down() {
        chosen = min(chosen + 1, max(cards.count - 1, 0))
    }

    /// What a contact picked makes: its card; why it makes none, in jaw's words; or nothing yet, when
    /// it came without what its card is made from, which Contacts is then asked for.
    public enum Made: Sendable, Equatable {
        case card(Card)
        case unadded(String)
        case unfetched
    }

    /// What a contact's card is made from, as Contacts is asked for it: what its vCard needs, and
    /// what its name is made from.
    public nonisolated static var keys: [any CNKeyDescriptor] {
        [CNContactVCardSerialization.descriptorForRequiredKeys(), CNContactFormatter.descriptorForRequiredKeys(for: .fullName)]
    }

    /// A contact's card as Contacts exports it, a vCard 3.0, without its photo, which keeps it small,
    /// as WhatsApp's phones send theirs; named as Contacts names it, or failing that by its
    /// organisation, its first number or its first email.
    public nonisolated static func made(_ contact: CNContact) -> Made {
        guard contact.areKeysAvailable(keys), let copy = contact.mutableCopy() as? CNMutableContact else { return .unfetched }
        let names = [CNContactFormatter.string(from: contact, style: .fullName), contact.organizationName,
                     contact.phoneNumbers.first?.value.stringValue, contact.emailAddresses.first.map { $0.value as String }]
        guard let name = names.compactMap({ $0?.trimmingCharacters(in: .whitespacesAndNewlines) }).first(where: { !$0.isEmpty }) else {
            return .unadded("that contact has no name, number or email to send")
        }
        copy.imageData = nil
        do {
            let data = try CNContactVCardSerialization.data(with: [copy])
            guard let vcard = String(data: data, encoding: .utf8) else { return .unadded(unwritten) }
            return .card(Card(name: name, vcard: vcard))
        } catch {
            return .unadded(unwritten + ": " + error.localizedDescription)
        }
    }

    /// What's said when Contacts can't write a contact as a card.
    nonisolated static let unwritten = "Contacts couldn't write that contact as a card"

    /// A contact's card, as `made` makes it, or nil when it makes none.
    public nonisolated static func card(_ contact: CNContact) -> Card? {
        if case let .card(card) = made(contact) { card } else { nil }
    }

    /// Puts on the form what was made of a contact picked for it: its card, or why there's none.
    public func land(_ made: Made) {
        switch made {
        case let .card(card): add(card)
        case let .unadded(why): unadded = why
        case .unfetched: unadded = "Contacts gave Jaw less of that contact than its card needs"
        }
    }
}

/// The contact forms whose cards have gone to jawd, by the key each went with, until jawd answers:
/// one it turns down opens again as it was, with its words, and none of its vCards is read again.
@MainActor
public final class ContactFormsSent {
    var forms: [String: ContactForm] = [:]

    public init() {}

    public func going(_ form: ContactForm, key: String) {
        forms[key] = form
    }

    /// jawd took the cards sent with `key`, or they're past opening again.
    public func taken(_ key: String) {
        forms[key] = nil
    }

    /// jawd turned down the cards sent with `key`: their form as it was, with jawd's words, or nil
    /// for a key it doesn't hold.
    public func refused(_ key: String, why: String) -> ContactForm? {
        guard let form = forms.removeValue(forKey: key) else { return nil }
        form.said = why
        form.unadded = nil
        return form
    }
}

/// The contact form's sheet: what it's to, its cards each with an ×, Add a Contact…, which shows the
/// system's picker from under itself, what's said or in the way, and Cancel and Send.
public struct ContactFormView: View {
    let form: ContactForm
    let roles: Roles
    let pick: (NSView) -> Void
    let send: () -> Void
    let cancel: () -> Void
    @State var anchor = Anchor()
    @FocusState var focused: Bool

    /// `pick` is given the view the picker is shown from: Add a Contact…, at once when the form opens
    /// with nothing on it, once the sheet is down.
    public init(form: ContactForm, roles: Roles, pick: @escaping (NSView) -> Void, send: @escaping () -> Void,
                cancel: @escaping () -> Void)
    {
        self.form = form
        self.roles = roles
        self.pick = pick
        self.send = send
        self.cancel = cancel
    }

    /// The most cards shown without scrolling.
    static let unscrolled = 6
    /// How long a sheet takes to come down, after which the picker can be shown from it.
    static let sheetDown: TimeInterval = 0.3

    public var body: some View {
        VStack(alignment: .leading, spacing: Space.l) {
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(form.title).textStyle(.heading).tone(.primary, roles)
                Text("To \(form.to)").textStyle(.callout).tone(.secondary, roles)
            }
            if form.cards.isEmpty {
                Text("No contact on it yet: Add a Contact… picks one from Contacts, and again for each one more.")
                    .textStyle(.callout).tone(.tertiary, roles)
                    .frame(maxWidth: .infinity, minHeight: 56, alignment: .leading)
            } else if form.cards.count > Self.unscrolled {
                ScrollViewReader { proxy in
                    ScrollView { rows }
                        .onChange(of: form.chosen) { proxy.scrollTo(form.chosen) }
                }
                .frame(height: 360)
            } else {
                rows
            }
            Text(form.warning ?? (form.cards.isEmpty ? " " : "↑↓ choose · ⌫ takes out · ↩ sends · esc cancels"))
                .textStyle(.caption).tone(form.warning == nil ? .tertiary : .warn, roles)
                .lineLimit(3)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: Space.m) {
                Button("Add a Contact…") { if let view = anchor.view { pick(view) } }
                    .background(PopoverAnchor(anchor: anchor))
                Spacer()
                Button("Cancel", action: cancel).keyboardShortcut(.cancelAction)
                Button("Send", action: send).keyboardShortcut(.defaultAction).disabled(!form.ready)
            }
        }
        .padding(Space.xl)
        .frame(width: 440)
        .background(roles.ground.color)
        .focusable()
        .focused($focused)
        .focusEffectDisabled()
        .onAppear {
            focused = true
            guard form.picksAtOnce else { return }
            let anchor = anchor, pick = pick
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(Self.sheetDown))
                if let view = anchor.view, view.window != nil { pick(view) }
            }
        }
        .onKeyPress(.upArrow) {
            form.up()
            return .handled
        }
        .onKeyPress(.downArrow) {
            form.down()
            return .handled
        }
        .onKeyPress(.delete) {
            form.remove(form.chosen)
            return .handled
        }
    }

    var rows: some View {
        VStack(spacing: Space.xs) {
            ForEach(Array(form.cards.enumerated()), id: \.offset) { i, card in
                FormCardRow(card: card, roles: roles, chosen: i == form.chosen) { form.remove(i) }
                    .id(i)
            }
        }
    }

    /// Where the view the picker is shown from is kept: a holder rather than state, so finding it
    /// changes nothing SwiftUI draws.
    @MainActor
    final class Anchor {
        weak var view: NSView?
    }
}

/// A card on the form: its picture, its name and its numbers as its message will show them, and ×.
struct FormCardRow: View {
    let card: ContactForm.Card
    let roles: Roles
    let chosen: Bool
    let remove: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: Space.m) {
            CardPictureView(card: card.shown, name: card.name, roles: roles, side: 36)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(card.name).textStyle(.body).fontWeight(.semibold).tone(.primary, roles).lineLimit(1)
                let numbers = CardAction.numbers(card.shown).map(\.number)
                Text(numbers.isEmpty ? "No number" : numbers.joined(separator: " · "))
                    .textStyle(.callout).tone(numbers.isEmpty ? .tertiary : .secondary, roles).lineLimit(1)
            }
            Spacer(minLength: Space.m)
            Button(action: remove) { Image(systemName: "xmark") }
                .buttonStyle(.borderless)
                .help("Take \(card.name) out (⌫)")
                .accessibilityLabel("Take \(card.name) out")
        }
        .padding(Space.s)
        .card(roles, selected: chosen)
    }
}

/// An empty view behind a control, which AppKit can show a popover from, as the system's contact
/// picker is shown: it keeps itself in `anchor` while it's in a window.
struct PopoverAnchor: NSViewRepresentable {
    let anchor: ContactFormView.Anchor

    func makeNSView(context _: Context) -> AnchorView {
        let view = AnchorView()
        view.anchor = anchor
        return view
    }

    func updateNSView(_ view: AnchorView, context _: Context) {
        view.anchor = anchor
    }

    final class AnchorView: NSView {
        var anchor: ContactFormView.Anchor?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            anchor?.view = window == nil ? nil : self
        }
    }
}
