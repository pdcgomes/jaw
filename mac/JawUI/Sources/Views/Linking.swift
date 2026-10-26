import AppKit
import CoreImage
import CoreImage.CIFilterBuiltins
import JawKit
import JawThemes
import SwiftUI

/// Linking, as the terminal's linking screen has it: the QR code jawd's `link` push carries beside
/// the Mimic, with the steps on the phone; or the code to type on the phone, for a number; logged
/// out leading back to it.
public struct LinkingView: View {
    public var step: LinkStep?
    /// Why the account was logged out, when it was.
    public var loggedOut: String?
    public var roles: Roles
    public var start: (String?) -> Void
    public var cancel: () -> Void
    @State var byPhone = false
    @State var phone = ""
    @State var asked = false
    @State var now = Date()
    let tick = Timer.publish(every: 1, on: .main, in: .common).autoconnect()

    public init(step: LinkStep?, loggedOut: String?, roles: Roles, start: @escaping (String?) -> Void, cancel: @escaping () -> Void) {
        self.step = step
        self.loggedOut = loggedOut
        self.roles = roles
        self.start = start
        self.cancel = cancel
    }

    public var body: some View {
        VStack(spacing: 22) {
            if let loggedOut, !asked {
                VStack(spacing: 10) {
                    MimicImage(look: .awake, scale: 4)
                    Text("Logged out").font(.system(size: 17, weight: .semibold))
                    Text("\(loggedOut.prefix(1).uppercased() + loggedOut.dropFirst()). jawd has deleted the session and kept your messages. Link again to carry on where you were.")
                        .multilineTextAlignment(.center).frame(maxWidth: 420).foregroundStyle(colour(roles.secondary))
                    Button("Link Again") { begin(nil) }.keyboardShortcut(.defaultAction)
                }
            } else {
                HStack(alignment: .center, spacing: 36) {
                    code
                    VStack(alignment: .leading, spacing: 12) {
                        MimicImage(look: .awake, scale: 3)
                        Text("Link Jaw to WhatsApp").font(.system(size: 17, weight: .semibold))
                        Text(instructions).foregroundStyle(colour(roles.secondary)).fixedSize(horizontal: false, vertical: true)
                        if case let .qr(_, expires) = step {
                            let left = max(0, Int((Double(expires) / 1000 - now.timeIntervalSince1970).rounded()))
                            Text("It changes in \(left) s.").font(.system(size: 11)).foregroundStyle(colour(roles.tertiary))
                        }
                        if case let .failed(reason) = step {
                            Text("Linking failed: \(reason)").foregroundStyle(colour(roles.failed))
                            Button("Try Again") { begin(byPhone ? phone : nil) }
                        }
                        Button(byPhone ? "Use a QR Code Instead" : "Use a Phone Number Instead") {
                            byPhone.toggle()
                            if !byPhone { begin(nil) }
                        }
                        .buttonStyle(.link)
                    }
                    .frame(maxWidth: 340, alignment: .leading)
                }
            }
            Text("Jaw isn't affiliated with WhatsApp or Meta.").font(.system(size: 11)).foregroundStyle(colour(roles.tertiary))
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(colour(roles.ground))
        .foregroundStyle(colour(roles.text))
        .onAppear { if loggedOut == nil { begin(nil) } }
        .onDisappear { cancel() }
        .onReceive(tick) { now = $0 }
        .focusEffectDisabled()
    }

    func begin(_ phone: String?) {
        asked = true
        start(phone)
    }

    var instructions: String {
        if byPhone {
            if case let .code(code) = step {
                return "On the phone: WhatsApp › Settings › Linked devices › Link a device › Link with phone number instead, then type \(code)."
            }
            return "Your number, with its country code, and Jaw shows a code to type on the phone."
        }
        return "On the phone: WhatsApp › Settings › Linked devices › Link a device, then point it at this code."
    }

    @ViewBuilder var code: some View {
        if byPhone {
            VStack(spacing: 12) {
                if case let .code(code) = step {
                    Text(code).font(.system(size: 34, weight: .semibold, design: .monospaced)).textSelection(.enabled)
                } else {
                    TextField("+351 912 345 678", text: $phone)
                        .textFieldStyle(.roundedBorder).frame(width: 240)
                        .onSubmit { if phone.filter(\.isNumber).count >= 7 { begin(phone) } }
                    Button("Get the Code") { begin(phone) }.disabled(phone.filter(\.isNumber).count < 7)
                }
            }
            .frame(width: 260, height: 260)
        } else if case let .qr(text, _) = step, let image = Self.qr(text) {
            Image(decorative: image, scale: 1)
                .interpolation(.none).resizable().frame(width: 260, height: 260)
                .padding(14).background(RoundedRectangle(cornerRadius: 14).fill(.white))
                .accessibilityLabel("The code to scan with your phone")
        } else {
            ProgressView().frame(width: 288, height: 288)
        }
    }

    /// The QR code for jawd's text, a pixel to a module, drawn without smoothing.
    static func qr(_ text: String) -> CGImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        return CIContext().createCGImage(output, from: output.extent)
    }

    func colour(_ c: RGBA) -> Color {
        Color(nsColor: c.nsColor)
    }
}

/// The keys, every command with its own, as the terminal's `?` shows them.
public struct KeysView: View {
    public var close: () -> Void

    public init(close: @escaping () -> Void) {
        self.close = close
    }

    static let sections: [(String, [(String, String)])] = [
        ("Everywhere", [
            ("⌘K or ⌃K", "the palette: chats, people, messages and every action"), ("⌘N", "start a chat"),
            ("⌘F", "search this chat"), ("⌘⇧F", "search every chat"), ("⌥↑ ⌥↓", "the chat before or after"),
            ("⌘B", "the sidebar"), ("⌘⇧L", "the chat list"), ("⌘⇧↩", "focus on this chat"), ("⌘J", "the glance"),
            ("⌘+ ⌘− ⌘0", "message text size"), ("⌘,", "settings"), ("⌘/", "these keys"),
        ]),
        ("Panes", [
            ("⌘D", "split across"), ("⌘⇧D", "split down"), ("⌘⌥ arrows", "the pane that way"),
            ("⌥⌘1 to ⌥⌘9", "a pane by its place"), ("⌃⌘ arrows", "swap with the pane that way"), ("⌘W", "close the pane"),
        ]),
        ("The box", [
            ("↩", "send"), ("⇧↩ or ⌥↩", "a new line"), ("↑ from an empty box", "select the newest message"),
            ("@", "a group's members"), (":", "emoji by name"), ("Esc", "drop the reply or the edit"),
        ]),
        ("A selected message", [
            ("↑ ↓", "the one before or after"), ("r", "reply"), ("e", "edit"), ("d", "delete"), ("+", "react"),
            ("1 to 6", "a quick reaction"), ("f", "forward"), ("y", "copy"), ("i", "its info"),
            ("↩", "send again, unfold, or open its link"), ("Esc", "back to typing"),
        ]),
        ("The palette", [
            ("↑ ↓", "move"), ("↩", "do it"), ("Tab", "narrow to a kind, or pick a chat to forward to"),
            ("⌘G", "go to the message"), ("⌫ on nothing", "widen again"), ("Esc", "close"),
        ]),
    ]

    public var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("The keys").font(.headline)
            ScrollView {
                Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 5) {
                    ForEach(Self.sections, id: \.0) { title, keys in
                        GridRow { Text(title).font(.system(size: 12, weight: .semibold)).padding(.top, 8).gridCellColumns(2) }
                        ForEach(keys, id: \.0) { key, what in
                            GridRow {
                                Text(key).font(.system(size: 12, design: .monospaced))
                                Text(what).font(.system(size: 12)).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
            HStack {
                Spacer()
                Button("Done", action: close).keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 520, height: 560)
        .focusEffectDisabled()
    }
}
