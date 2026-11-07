import AppKit
import JawThemes
import SwiftUI

/// Settings › Appearance: dark, light or the system's; the slots, Dark and Light, each a tile of what it
/// draws; and the gallery that fills the outlined one, Jaw's, Omarchy's and Redlamp's themes each a
/// card. It scrolls as a whole, as a grouped form does, the slots and the gallery open under their
/// headers as the wireframes draw them, so an arrow's card can be scrolled to, set in as Messages' and
/// Panes' grouped forms are; the arrows move through the cards from the moment it opens, previewing
/// each as they reach it.
public struct AppearancePane: View {
    @Bindable var preferences: Preferences
    @Environment(\.colorScheme) private var scheme
    /// Counted up by each click in the pane, beside what the click does there, and by each card or tile
    /// chosen with VoiceOver or a card's menu, which gives the gallery the window's keys again.
    @State private var clicked = 0

    public init(preferences: Preferences) {
        self.preferences = preferences
    }

    /// The slot the gallery chooses for: the one picked, or the one in use.
    var slot: Slot {
        preferences.choosing ?? (scheme == .dark ? .dark : .light)
    }

    public var body: some View {
        let gallery = Gallery(preferences.themes, for: slot, omarchyLine: preferences.omarchyLine)
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: Space.xl) {
                    GroupBox {
                        HStack {
                            Text("Appearance").accessibilityHidden(true)
                            Spacer(minLength: Space.m)
                            Picker("Appearance", selection: $preferences.appearance) {
                                Text("The system's").tag(Appearance.system)
                                Text("Dark").tag(Appearance.dark)
                                Text("Light").tag(Appearance.light)
                            }
                            .pickerStyle(.radioGroup)
                            .horizontalRadioGroupLayout()
                            .labelsHidden()
                        }
                        .padding(.horizontal, Space.xs)
                        .padding(.vertical, Space.s)
                    }
                    SettingsSection("Themes", footer: "Jaw draws the dark theme while the Mac is dark, and the light one while it's light.") {
                        HStack(spacing: Space.m) {
                            ForEach(Slot.allCases, id: \.self) { each in
                                SlotTileView(tile: SlotTile(each, slots: preferences.slots, themes: preferences.themes,
                                                            served: preferences.served != nil, appearance: preferences.appearance,
                                                            dark: scheme == .dark),
                                             roles: preferences.roles(each), outlined: each == slot)
                                    .onTapGesture { pick(each) }
                                    .accessibilityAction { pick(each) }
                            }
                        }
                        .fixedSize(horizontal: false, vertical: true)
                    }
                    ForEach(gallery.sections) { section in
                        GallerySection(section: section, slot: slot, slots: preferences.slots) { theme, only in
                            choose(theme, only: only)
                        }
                    }
                }
                .padding(SettingsPane.margin)
                .contentShape(Rectangle())
                .simultaneousGesture(TapGesture().onEnded { [preferences] in
                    clicked += 1
                    // Ends the run once a card's own tap, which SwiftUI runs after this one, has chosen
                    // from its start.
                    DispatchQueue.main.async { preferences.arrowsFrom = nil }
                })
            }
            .background(ArrowKeys(clicked: clicked) { arrow in move(arrow, in: gallery, proxy) })
        }
        .frame(width: SettingsPane.appearance.size.width, height: SettingsPane.appearance.size.height)
    }

    /// Makes `each` the slot the gallery chooses for, as a click on its tile does, which chooses no theme:
    /// it ends the arrows' run keeping what they show, as Settings closing does.
    func pick(_ each: Slot) {
        preferences.choosing = each
        clicked += 1
    }

    /// Fills the slots with `theme` as a click on its card does, from the owner's own slots, which ends
    /// the arrows' run, and gives the gallery the keys again.
    func choose(_ theme: Theme, only: Bool) {
        preferences.choose(theme, for: slot, only: only)
        clicked += 1
    }

    /// Previews the card an arrow reaches from the slots the arrows started from, scrolls it into
    /// sight and says for VoiceOver what's chosen, as nothing VoiceOver's cursor is on changes; says
    /// whether there was a card.
    func move(_ arrow: Gallery.Arrow, in gallery: Gallery, _ proxy: ScrollViewProxy) -> Bool {
        guard let theme = gallery.moving(from: preferences.slots[slot], arrow) else { return false }
        preferences.preview(theme, for: slot)
        proxy.scrollTo(theme.id)
        NSAccessibility.post(element: NSApp as Any, notification: .announcementRequested, userInfo: [
            .announcement: Gallery.label(theme, for: slot, preferences.slots),
            .priority: NSAccessibilityPriorityLevel.high.rawValue,
        ])
        return true
    }
}

/// Settings › Messages: bubbles or lines, comfortable or compact, the text size, the font, and
/// whether photos and videos go in HD.
public struct MessagesPane: View {
    @Bindable var preferences: Preferences

    public init(preferences: Preferences) {
        self.preferences = preferences
    }

    public var body: some View {
        Form {
            Section {
                Picker("Messages look like", selection: $preferences.look) {
                    ForEach(Look.allCases, id: \.self) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
            } footer: {
                Text(preferences.look == .bubbles
                    ? "Theirs on the left, yours on the right."
                    : "The terminal's layout: names on the left, yours from a third of the way across.")
                    .font(.callout).foregroundStyle(.secondary)
            }
            Section {
                Picker("Spacing", selection: $preferences.density) {
                    ForEach(Density.allCases, id: \.self) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                LabeledContent("Text size") {
                    HStack {
                        Slider(value: $preferences.textSize, in: Preferences.textSizes, step: 1)
                        Text("\(Int(preferences.textSize)) pt").monospacedDigit().frame(width: 40, alignment: .trailing)
                    }
                }
                Picker("Font", selection: $preferences.font) {
                    ForEach(MessageFont.allCases, id: \.self) { Text($0.title).tag($0) }
                }
            }
            Section {
                Toggle("Send photos and videos in HD", isOn: $preferences.sendHD)
            } footer: {
                Text("A sharper copy of each goes as well, as WhatsApp's HD sends it. Tab in the box switches what's there; ⌥ as you click Send sends HD.")
                    .font(.callout).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .frame(width: SettingsPane.messages.size.width, height: SettingsPane.messages.size.height)
    }
}

/// Settings › Panes: how much the panes without the keys are dimmed, and the focused pane's outline.
public struct PanesPane: View {
    @Bindable var preferences: Preferences
    @Environment(\.colorScheme) private var scheme

    public init(preferences: Preferences) {
        self.preferences = preferences
    }

    /// The theme's accent in the appearance Settings is drawn in, the outline's colour unless one's picked.
    var accent: RGBA {
        preferences.style(dark: scheme == .dark).roles.accent
    }

    public var body: some View {
        Form {
            Section {
                LabeledContent("Dim the other panes") {
                    HStack {
                        Slider(value: $preferences.paneDim, in: Preferences.paneDims)
                        Text("\(Int((preferences.paneDim * 100).rounded())) %").monospacedDigit().frame(width: 40, alignment: .trailing)
                    }
                }
                Toggle("Outline the focused pane", isOn: $preferences.paneOutline)
                LabeledContent("Outline colour") {
                    HStack {
                        ColorPicker("", selection: Binding(
                            get: { (preferences.paneOutlineColour ?? accent).color },
                            set: { preferences.paneOutlineColour = RGBA(NSColor($0)) },
                        ), supportsOpacity: false)
                            .labelsHidden()
                        Button("The theme's accent") { preferences.paneOutlineColour = nil }
                            .disabled(preferences.paneOutlineColour == nil)
                    }
                }
                .disabled(!preferences.paneOutline)
            } footer: {
                Text("With more than one pane, the one with the keys is outlined inside its edge, and the others dimmed, as iTerm does.")
                    .font(.callout).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .frame(width: SettingsPane.panes.size.width, height: SettingsPane.panes.size.height)
    }
}

extension RGBA {
    /// From a colour AppKit gives, a colour picked or one of the system's, in sRGB.
    init(_ colour: NSColor) {
        let c = colour.usingColorSpace(.sRGB) ?? colour
        self.init(r: Double(c.redComponent), g: Double(c.greenComponent), b: Double(c.blueComponent))
    }
}
