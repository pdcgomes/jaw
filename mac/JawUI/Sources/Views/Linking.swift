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
                    Text("Logged out").textStyle(.title)
                    Text("\(loggedOut.prefix(1).uppercased() + loggedOut.dropFirst()). jawd has deleted the session and kept your messages. Link again to carry on where you were.")
                        .multilineTextAlignment(.center).frame(maxWidth: 420).foregroundStyle(roles.secondary.color)
                    Button("Link Again") { begin(nil) }.keyboardShortcut(.defaultAction)
                }
            } else {
                HStack(alignment: .center, spacing: 36) {
                    code
                    VStack(alignment: .leading, spacing: 12) {
                        MimicImage(look: .awake, scale: 3)
                        Text("Link Jaw to WhatsApp").textStyle(.title)
                        Text(instructions).foregroundStyle(roles.secondary.color).fixedSize(horizontal: false, vertical: true)
                        if case let .qr(_, expires) = step {
                            let left = max(0, Int((Double(expires) / 1000 - now.timeIntervalSince1970).rounded()))
                            Text("It changes in \(left) s.").textStyle(.caption).foregroundStyle(roles.tertiary.color)
                        }
                        if case let .failed(reason) = step {
                            Text("Linking failed: \(reason)").foregroundStyle(roles.failed.color)
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
            Text("Jaw isn't affiliated with WhatsApp or Meta.").textStyle(.caption).foregroundStyle(roles.tertiary.color)
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(roles.ground.color)
        .foregroundStyle(roles.text.color)
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
                    Text(code).textStyle(.display).textSelection(.enabled)
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
                .padding(14).background(RoundedRectangle(cornerRadius: Radius.panel).fill(.white))
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

}

/// The keys, every command with its own, as the terminal's `?` shows them.
public struct KeysView: View {
    public var close: () -> Void
    /// The commands' keys, by menu, as `Menus.keys` gives them.
    let commands: [(String, [(String, String)])]

    public init(commands: [(String, [(String, String)])], close: @escaping () -> Void) {
        self.commands = commands
        self.close = close
    }

    var sections: [(String, [(String, String)])] {
        commands + Self.views
    }

    /// Keys the box and the palette handle themselves, which no menu holds.
    static let views: [(String, [(String, String)])] = [
        ("The box", [
            ("↩", "send"), ("⇧↩ or ⌥↩", "a new line"), ("↑ from an empty box", "select the newest message"),
            ("@", "a group's members"), (":", "emoji by name"), ("Esc", "drop the reply or the edit"),
        ]),
        ("A selected message", [
            ("↑ ↓", "the one before or after"), ("1 to 6", "a quick reaction"),
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
                    ForEach(sections, id: \.0) { title, keys in
                        GridRow { Text(title).textStyle(.callout).fontWeight(.semibold).padding(.top, 8).gridCellColumns(2) }
                        ForEach(keys, id: \.0) { key, what in
                            GridRow {
                                Text(key).textStyle(.mono)
                                Text(what).textStyle(.callout).foregroundStyle(.secondary)
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
