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

/// A whole window's content at the screen's size, as the window holds it, made from a `Screen`.
struct ScreenSpecimen: View {
    let screen: Screen
    let conditions: Conditions

    var body: some View {
        ControllerSpecimen { screen.content(conditions) }
            .frame(width: screen.width, height: screen.height)
    }
}
