import AppKit
import JawKit
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
/// the title bar's buttons over it where the window puts them, and over both, when one is open, the
/// palette, the glance or Settings, each a view where the window puts its panel or its sheet, so a
/// picture holds it.
struct ScreenSpecimen: View {
    /// What's open over the window, which the app shows in a window of its own above it.
    enum Overlay {
        case palette, glance, settings
    }

    let screen: Screen
    let conditions: Conditions
    var overlay: Overlay?

    var body: some View {
        ZStack(alignment: .topLeading) {
            ControllerSpecimen { screen.content(conditions) }
                .frame(width: screen.width, height: screen.height)
            WindowButtons(roles: conditions.roles)
            switch overlay {
            case .palette: PaletteOver(roles: conditions.roles)
            case .glance: GlanceOver(screen: screen, roles: conditions.roles)
            case .settings: SettingsOver(conditions: conditions)
            case nil: EmptyView()
            }
        }
        .frame(width: screen.width, height: screen.height)
    }
}

/// The palette with "ar" typed and jawd's answer from `protocol/fixtures/find.result.json`, as the
/// palette scene makes it, where `PalettePanel.show(over:)` puts it: centred, its top 60 points below
/// the window's, 620 points wide and as tall as the palette is, as its panel takes its view's size.
private struct PaletteOver: View {
    let roles: Roles
    @State private var model: PaletteModel = {
        let model = PaletteModel()
        model.query = "ar"
        model.answered(id: 1, Fixtures.found)
        return model
    }()

    var body: some View {
        PaletteView(model: model, roles: roles) {}
            .fixedSize(horizontal: false, vertical: true)
            .padding(.top, 60)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }
}

/// The glance as `Glance.show(over:roles:)` opens it: the chats that need you that no pane shows,
/// each with its newest three messages as a watch of `.newest(3)` gives them, which only Lisbon
/// Climbing has among `Samples`' chats; 520 points wide, as tall as five chats' cards at most and no
/// less than the 260 points its panel takes, and centred in the window.
private struct GlanceOver: View {
    let roles: Roles
    @State private var state: GlanceState

    init(screen: Screen, roles: Roles) {
        self.roles = roles
        let state = GlanceState()
        state.chats = Situation.needs(screen.chats, shown: Set(screen.open.map(\.id)))
        for chat in state.chats where chat.id == Samples.group.id {
            let messages = Samples.messages
            var rows = Messages<MessageRow>(chat: chat.id, place: .newest(3))
            rows.apply(Update(seq: 1, upsert: Array(messages.sorted { $0.sort < $1.sort }.suffix(3)), total: UInt64(messages.count),
                              older: true, newer: 0, moved: Moved(place: .newest(3), newer: 0)))
            state.rows[chat.id] = rows
        }
        _state = State(initialValue: state)
    }

    var body: some View {
        GlanceView(state: state, roles: roles) { _, _ in }
            .frame(width: 520, height: max(GlanceView.height(min(state.chats.count, 5)), 260))
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Settings as `MainWindowController.showSettings()` shows them, in a sheet at its own size, centred
/// in the window as macOS puts a sheet, on the window's colour, which a sheet sits on, with a shadow
/// from the theme's separator. Its preferences are the harness's own, emptied each time they're
/// made, so they're the app's defaults but for the look, the density, the text size and the font the
/// scene is drawn in, whatever was chosen in Settings the last time it was shown.
private struct SettingsOver: View {
    let roles: Roles
    @State private var preferences: Preferences

    init(conditions: Conditions) {
        roles = conditions.roles
        let suite = "\(Bundle.main.bundleIdentifier ?? "JawHarness").settings"
        let defaults = UserDefaults(suiteName: suite)!
        defaults.removePersistentDomain(forName: suite)
        let preferences = Preferences(defaults: defaults)
        preferences.look = conditions.look
        preferences.density = conditions.density
        preferences.textSize = conditions.textSize
        preferences.font = conditions.font
        _preferences = State(initialValue: preferences)
    }

    var body: some View {
        SettingsView(preferences: preferences, accent: roles.accent) {}
            .fixedSize()
            .background(Color(nsColor: .windowBackgroundColor))
            .clipShape(RoundedRectangle(cornerRadius: Radius.panel))
            .shadow(color: roles.separator.color, radius: Space.l)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// The title bar's buttons where the window puts them. The window's three are real buttons made for
/// the app's window, which draw as its own do, grey in a window that isn't key, as a picture's never
/// is; a button made on its own sends its action to the window with the keys, the harness's, so these
/// send none. Beside them, the sidebar's button as `TitleBar` makes it, in `roles`, toggling nothing.
private struct WindowButtons: NSViewRepresentable {
    let roles: Roles

    func makeNSView(context _: Context) -> NSView {
        let view = TopLeft()
        for (kind, frame) in Screen.titleBar.buttons {
            guard let button = NSWindow.standardWindowButton(kind, for: TitleBar.style) else { continue }
            button.frame = frame
            button.action = nil
            view.addSubview(button)
        }
        let sidebar = TitleBar.sidebarButton {}
        sidebar.roles = roles
        view.addSubview(sidebar)
        NSLayoutConstraint.activate([
            sidebar.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: Screen.titleBar.sidebar.minX),
            sidebar.topAnchor.constraint(equalTo: view.topAnchor, constant: Screen.titleBar.sidebar.minY),
        ])
        return view
    }

    func updateNSView(_: NSView, context _: Context) {}

    func sizeThatFits(_: ProposedViewSize, nsView _: NSView, context _: Context) -> CGSize? {
        let frames = Screen.titleBar.buttons.map(\.frame) + [Screen.titleBar.sidebar]
        return CGSize(width: frames.map(\.maxX).max() ?? 0, height: frames.map(\.maxY).max() ?? 0)
    }
}

/// A view whose subviews' frames run from its top left, as SwiftUI's do.
private final class TopLeft: NSView {
    override var isFlipped: Bool {
        true
    }
}
