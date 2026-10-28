import AppKit
import JawThemes
import JawUI
import SwiftUI

/// What a band of specimens says to look for. The bands a scene draws hand theirs up, so a sweep's
/// manifest carries each scene's notes and the review canvas shows them beside its pictures.
struct Note: Hashable, Codable, Sendable {
    let title: String
    let note: String
}

struct NotesKey: PreferenceKey {
    static let defaultValue: [Note] = []

    static func reduce(value: inout [Note], nextValue: () -> [Note]) {
        value += nextValue()
    }
}

/// A titled band of specimens with a note on what would be wrong with them: a gallery shows you a
/// component, and a note on what to look for turns looking into reviewing.
struct SpecimenGroup<Content: View>: View {
    let title: String
    let note: String
    let roles: Roles
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: Space.l) {
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(title).textStyle(.heading).tone(.primary, roles)
                Text(note).textStyle(.callout).tone(.secondary, roles)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: 640, alignment: .leading)
            }
            content
        }
        .padding(.bottom, Space.xxl)
        .preference(key: NotesKey.self, value: [Note(title: title, note: note)])
    }
}

/// One component with a monospaced caption under it.
struct Specimen<Content: View>: View {
    let caption: String
    let roles: Roles
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            content
            Text(caption).textStyle(.mono).tone(.tertiary, roles)
        }
    }
}

/// An AppKit controller where SwiftUI puts it, at the frame it's given: a chat list, a conversation.
struct ControllerSpecimen: NSViewControllerRepresentable {
    let make: @MainActor () -> NSViewController

    func makeNSViewController(context _: Context) -> NSViewController {
        make()
    }

    func updateNSViewController(_: NSViewController, context _: Context) {}
}

/// A whole window's content at the screen's size, as the window holds it, made from a `Screen`, with
/// the window's buttons over it where the window puts them.
struct ScreenSpecimen: View {
    let screen: Screen
    let conditions: Conditions

    var body: some View {
        ControllerSpecimen { screen.content(conditions) }
            .frame(width: screen.width, height: screen.height)
            .overlay(alignment: .topLeading) { WindowButtons() }
    }
}

/// The window's three buttons where the window puts them: real buttons made for the app's window,
/// which draw as its own do, grey in a window that isn't key, as a picture's never is. A button made
/// on its own sends its action to the window with the keys, the harness's, so these send none.
private struct WindowButtons: NSViewRepresentable {
    func makeNSView(context _: Context) -> NSView {
        let view = TopLeft()
        for (kind, frame) in Screen.titleBar.buttons {
            guard let button = NSWindow.standardWindowButton(kind, for: Screen.style) else { continue }
            button.frame = frame
            button.action = nil
            view.addSubview(button)
        }
        return view
    }

    func updateNSView(_: NSView, context _: Context) {}

    func sizeThatFits(_: ProposedViewSize, nsView _: NSView, context _: Context) -> CGSize? {
        let frames = Screen.titleBar.buttons.map(\.frame)
        return CGSize(width: frames.map(\.maxX).max() ?? 0, height: frames.map(\.maxY).max() ?? 0)
    }
}

/// A view whose subviews' frames run from its top left, as SwiftUI's do.
private final class TopLeft: NSView {
    override var isFlipped: Bool {
        true
    }
}
