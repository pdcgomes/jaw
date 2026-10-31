import AppKit
import JawKit
import JawThemes
import SwiftUI

/// The Mimic in SwiftUI: a look, and a count that chomps each time it goes up.
public struct MimicImage: NSViewRepresentable {
    public var look: Mimic.Look
    public var scale: CGFloat
    public var chomps: Int

    public init(look: Mimic.Look = .awake, scale: CGFloat = 4, chomps: Int = 0) {
        self.look = look
        self.scale = scale
        self.chomps = chomps
    }

    public func makeNSView(context _: Context) -> MimicView {
        let v = MimicView(scale: scale)
        v.look = look
        return v
    }

    public func updateNSView(_ v: MimicView, context: Context) {
        v.look = look
        v.scale = scale
        if chomps > context.coordinator.chomps { v.chomp() }
        context.coordinator.chomps = chomps
    }

    public func makeCoordinator() -> Counter {
        Counter(chomps: chomps)
    }

    public final class Counter {
        var chomps: Int
        init(chomps: Int) {
            self.chomps = chomps
        }
    }

    public func sizeThatFits(_: ProposedViewSize, nsView _: MimicView, context _: Context) -> CGSize? {
        CGSize(width: Mimic.size.width * scale, height: Mimic.size.height * scale)
    }
}

/// An empty chat: the Mimic sits in it and chomps when the first message comes.
public struct EmptyChatView: View {
    public var chat: ChatRow
    public var chomped = 0

    public init(chat: ChatRow, chomped: Int = 0) {
        self.chat = chat
        self.chomped = chomped
    }

    public var body: some View {
        VStack(spacing: 16) {
            MimicImage(look: .awake, scale: 4, chomps: chomped)
            Text(Self.words(chat))
                .textStyle(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 320)
        }
        .focusEffectDisabled()
    }

    static func words(_ chat: ChatRow) -> String {
        if chat.kind.isGroup { return "No messages in \(chat.name) yet. What you type goes to everyone in it." }
        let first = chat.name.split(separator: " ").first.map(String.init) ?? chat.name
        return "No messages with \(first) yet. What you type goes to them."
    }
}

/// What fills the conversation's place when no chat is open, or the app can't show chats.
public struct StateView: View {
    public enum Kind: Equatable {
        /// No chat open: ⌘K finds one.
        case noChat
        case noChats
        case starting(failed: String?)
        case asleep(why: String, retryIn: Int)
        case loggedOut(reason: String?)
        case unlinked
        /// Linking in the app, drawn by `LinkingView`.
        case linking(step: LinkStep?, loggedOut: String?)
    }

    public var kind: Kind
    public var roles: Roles

    public init(kind: Kind, roles: Roles) {
        self.kind = kind
        self.roles = roles
    }

    public var body: some View {
        VStack(spacing: 18) {
            MimicImage(look: look, scale: 4)
            VStack(spacing: 6) {
                Text(title).textStyle(.heading)
                    .foregroundStyle(roles.text.color)
                if let detail {
                    Text(detail).textStyle(.body)
                        .foregroundStyle(roles.secondary.color)
                        .multilineTextAlignment(.center)
                        .textSelection(.enabled)
                }
            }
            .frame(maxWidth: 420)
            if case .loggedOut = kind { notAffiliated } else if kind == .unlinked { notAffiliated }
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(roles.ground.color)
        .focusEffectDisabled()
        .accessibilityElement(children: .combine)
    }

    var notAffiliated: some View {
        Text("Jaw isn't affiliated with WhatsApp or Meta.")
            .textStyle(.caption).foregroundStyle(roles.tertiary.color)
    }

    var look: Mimic.Look {
        if case .asleep = kind { return .asleep }
        return .awake
    }

    var title: String {
        switch kind {
        case .noChat: "No chat open"
        case .noChats: "No chats yet"
        case .starting(nil): "Starting jawd"
        case .starting: "jawd didn't start"
        case .asleep: "jawd isn't answering"
        case .loggedOut: "Logged out"
        case .unlinked, .linking: "Not linked yet"
        }
    }

    var detail: String? {
        switch kind {
        case .noChat: "⌘K finds one, and ⌘N starts one."
        case .noChats: "They come as your phone sends them."
        case .starting(nil): "jawd keeps your messages and talks to WhatsApp. If it can't start, this says why."
        case let .starting(failed?): "It didn't start: \(failed)"
        case let .asleep(why, retryIn): "\(why). Jaw tries again by itself\(retryIn > 0 ? " in \(retryIn) s" : ""); jaw status says more."
        case let .loggedOut(reason):
            "\(reason.map { $0.prefix(1).uppercased() + $0.dropFirst() + "." } ?? "Your phone logged jaw out.") jawd has deleted the session and kept your messages. Link again with jaw in the terminal: run jaw and scan the code with your phone. Linking comes to this app later."
        case .linking: nil
        case .unlinked: "Link this Mac with jaw in the terminal: run jaw and scan the code with your phone (WhatsApp › Settings › Linked devices › Link a device). Linking comes to this app later."
        }
    }
}

/// The status line along the bottom of the window, in SF Mono: on the left, jawd's state when it
/// isn't online and how the focused chat is, without its name, which its box and header already
/// say; on the right, what needs you elsewhere, as the spike's has it: mentions and people by name,
/// groups counted, the count shortening before a name goes, then the glance's key.
public struct StatusLine: View {
    public var state: String?
    public var warn: Bool
    /// Who's typing in the focused chat, drawn in the online colour before the rest of `chat`.
    public var typing: String?
    public var chat: String?
    public var needs: [ChatRow]
    /// The chats someone's typing in, for the dot before a person named on the right.
    public var typingIn: Set<String>
    public var roles: Roles
    public var open: (ChatRow) -> Void
    public var glance: () -> Void

    public init(state: String?, warn: Bool, typing: String? = nil, chat: String?, needs: [ChatRow] = [], typingIn: Set<String> = [],
                roles: Roles, open: @escaping (ChatRow) -> Void = { _ in }, glance: @escaping () -> Void = {})
    {
        self.state = state
        self.warn = warn
        self.typing = typing
        self.chat = chat
        self.needs = needs
        self.typingIn = typingIn
        self.roles = roles
        self.open = open
        self.glance = glance
    }

    /// How the focused chat is, as it's drawn: who's typing in the online colour, then the rest,
    /// quieter.
    static func chatParts(typing: String?, chat: String?, roles: Roles) -> [(text: String, colour: RGBA)] {
        var parts: [(text: String, colour: RGBA)] = []
        if let typing { parts.append((typing, roles.online)) }
        if let chat { parts.append(((typing == nil ? "" : " · ") + chat, roles.secondary)) }
        return parts
    }

    public var body: some View {
        HStack(spacing: 10) {
            if let state {
                Text(state).foregroundStyle((warn ? roles.mention : roles.accent).color)
            }
            let parts = Self.chatParts(typing: typing, chat: chat, roles: roles)
            if !parts.isEmpty {
                Text(parts.reduce(into: AttributedString()) { line, part in
                    var said = AttributedString(part.text)
                    said.foregroundColor = part.colour.color
                    line.append(said)
                })
            }
            Spacer(minLength: 12)
            ViewThatFits(in: .horizontal) {
                right(names: 3, count: .long)
                right(names: 2, count: .long)
                right(names: 2, count: .short)
                right(names: 1, count: .short)
                right(names: 0, count: .tiny)
                EmptyView()
            }
        }
        .textStyle(.caption)
        .monospacedDigit()
        .lineLimit(1)
        .padding(.horizontal, Space.l)
        .frame(maxHeight: .infinity)
        .background(roles.surface.color)
        .focusEffectDisabled()
    }

    enum Count { case long, short, tiny }

    /// Mentions and people named, up to `names`, and the rest counted.
    @ViewBuilder func right(names: Int, count: Count) -> some View {
        let named = Array(needs.filter { $0.mentions > 0 || !$0.kind.isGroup }.prefix(names))
        let rest = needs.filter { c in !named.contains { $0.id == c.id } }
        HStack(spacing: 10) {
            ForEach(named, id: \.id) { c in
                Button { open(c) } label: {
                    if c.mentions > 0 {
                        Text("@ \(c.name)").foregroundStyle(roles.mention.color).bold()
                    } else {
                        HStack(spacing: 4) {
                            if typingIn.contains(c.id) { Circle().fill(roles.online.color).frame(width: 6, height: 6) }
                            Text("\(c.name) · \(c.unread)").foregroundStyle(roles.text.color)
                        }
                    }
                }
                .buttonStyle(.plain)
            }
            if !rest.isEmpty {
                let unread = rest.reduce(0) { $0 + $1.unread }
                let s = rest.count == 1 ? "" : "s"
                let text = switch count {
                case .long: "\(rest.count) more chat\(s) · \(Format.num(unread)) unread"
                case .short: "\(rest.count) more chat\(s)"
                case .tiny: "+\(rest.count)"
                }
                Button(action: glance) { Text(text).foregroundStyle(roles.secondary.color) }.buttonStyle(.plain)
            }
            if !needs.isEmpty {
                Button(action: glance) { Text("⌘J glance").foregroundStyle(roles.tertiary.color) }.buttonStyle(.plain)
            }
        }
        .fixedSize()
    }

}

/// A message's receipts: when it was sent, and each recipient's delivered, read and played times.
public struct InfoView: View {
    public var info: MessageInfo?
    public var why: String?
    public var mine: Bool
    public var voice: Bool
    public var close: () -> Void

    public init(info: MessageInfo?, why: String?, mine: Bool, voice: Bool, close: @escaping () -> Void) {
        self.info = info
        self.why = why
        self.mine = mine
        self.voice = voice
        self.close = close
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Message info").font(.headline)
            if let why {
                Text(why).foregroundStyle(.secondary)
            } else if let info {
                Text(info.sent.map { "Sent \(Format.full($0))" } ?? "On its way").foregroundStyle(.secondary)
                if !mine {
                    Text("Receipts are only for your own messages.").foregroundStyle(.secondary)
                } else {
                    Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 6) {
                        GridRow {
                            Text("").bold()
                            Text("Delivered").bold()
                            Text("Read").bold()
                            if voice { Text("Played").bold() }
                        }
                        ForEach(info.recipients, id: \.person) { r in
                            GridRow {
                                Text(r.name)
                                Text(time(r.delivered))
                                Text(time(r.read))
                                if voice { Text(time(r.played)) }
                            }
                        }
                    }
                    .textStyle(.callout)
                }
            } else {
                ProgressView().controlSize(.small)
            }
            HStack {
                Spacer()
                Button("Done", action: close).keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(minWidth: 380)
        .focusEffectDisabled()
    }

    func time(_ at: Int64?) -> String {
        guard let at else { return "–" }
        return Format.sameDay(at, Format.now()) ? Format.clock(at) : Format.when(at, now: Format.now()) + " " + Format.clock(at)
    }
}
