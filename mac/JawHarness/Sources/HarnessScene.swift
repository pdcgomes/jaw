import JawThemes
import JawUI
import SwiftUI

/// Where a scene sits in the sidebar, in sidebar order.
enum HarnessSection: String, CaseIterable, Identifiable {
    case foundations = "Foundations"
    case controls = "Controls"
    case chatList = "Chat list"
    case conversation = "Conversation"
    case panes = "Panes"
    case overlays = "Palette and overlays"
    case states = "States"
    case screens = "Screens"
    case live = "Live"
    case performance = "Performance"

    var id: String {
        rawValue
    }

    var symbol: String {
        switch self {
        case .foundations: "paintpalette"
        case .controls: "slider.horizontal.3"
        case .chatList: "list.bullet.rectangle"
        case .conversation: "bubble.left.and.bubble.right"
        case .panes: "rectangle.split.3x1"
        case .overlays: "command"
        case .states: "moon.zzz"
        case .screens: "macwindow"
        case .live: "play.circle"
        case .performance: "gauge.with.dots.needle.67percent"
        }
    }
}

/// What a scene is drawn in: the controls under the scene list, as values a scene hands to the
/// components it shows, as the app hands them its preferences.
struct Conditions: Hashable, Sendable {
    var theme = Catalog.jaw.id
    var dark = true
    var look: Look = .bubbles
    var density: Density = .comfortable
    var textSize: CGFloat = 13
    var font: MessageFont = .system
    var reduceMotion = false

    var roles: Roles {
        Catalog.family(theme).roles(dark: dark)
    }

    var style: Style {
        Style(roles: roles, look: look, density: density, textSize: textSize, font: font)
    }
}

/// What a sweep draws a scene in besides dark and light, which it draws every scene in.
struct Axes: OptionSet, Hashable, Sendable {
    let rawValue: Int

    /// Bubbles and lines, for a scene that draws messages.
    static let look = Axes(rawValue: 1 << 0)
    /// Comfortable and compact, where density changes what's drawn.
    static let density = Axes(rawValue: 1 << 1)
}

/// One of the conditions a sweep draws a scene in, named as its picture is: `dark`, `light-lines`,
/// `dark-bubbles-compact`.
struct Variant: Hashable, Sendable {
    var dark: Bool
    var look: Look?
    var density: Density?

    var name: String {
        [dark ? "dark" : "light", look?.rawValue, density == .compact ? "compact" : nil]
            .compactMap { $0 }
            .joined(separator: "-")
    }

    /// `conditions` in this variant's appearance, and in its look and density where it names them.
    func applied(to conditions: Conditions) -> Conditions {
        var c = conditions
        c.dark = dark
        if let look { c.look = look }
        if let density { c.density = density }
        return c
    }

    /// Dark before light, bubbles before lines, comfortable before compact.
    static func all(_ axes: Axes) -> [Variant] {
        let looks: [Look?] = axes.contains(.look) ? Look.allCases.map(Optional.some) : [nil]
        let densities: [Density?] = axes.contains(.density) ? Density.allCases.map(Optional.some) : [nil]
        return [true, false].flatMap { dark in
            looks.flatMap { look in densities.map { Variant(dark: dark, look: look, density: $0) } }
        }
    }
}

/// One entry in the harness: a component, or a screen, in the states worth reviewing.
struct HarnessScene: Identifiable {
    let id: String
    let title: String
    let symbol: String
    /// One line on what to look at, shown under the window's title: what turns a gallery into a
    /// review.
    let synopsis: String
    let section: HarnessSection
    /// The `JawUI` types it shows, which `scripts/check-harness` reads.
    let covers: [Any.Type]
    /// What a sweep draws it in besides dark and light.
    let axes: Axes
    /// How wide it's drawn, in points; it's as tall as it needs.
    let width: CGFloat
    /// Built only when it's shown, so a scene that isn't costs nothing.
    let content: @MainActor (Conditions) -> AnyView
    /// A pane of knobs beside the stage, for a scene that has one.
    var inspector: (@MainActor () -> AnyView)?

    init(id: String, title: String, symbol: String, synopsis: String, section: HarnessSection, covers: [Any.Type],
         axes: Axes = [], width: CGFloat = 960, @ViewBuilder content: @escaping @MainActor (Conditions) -> some View)
    {
        self.id = id
        self.title = title
        self.symbol = symbol
        self.synopsis = synopsis
        self.section = section
        self.covers = covers
        self.axes = axes
        self.width = width
        self.content = { AnyView(content($0)) }
    }

    /// The conditions a sweep draws it in.
    var variants: [Variant] {
        Variant.all(axes)
    }
}

/// The scenes, in the order they're registered. Built once and kept: a scene's knobs keep their
/// models in it, and a catalog made again would quietly hand the stage new ones.
struct HarnessCatalog {
    private(set) var scenes: [HarnessScene] = []

    mutating func register(_ scene: HarnessScene) {
        precondition(!scenes.contains { $0.id == scene.id }, "two harness scenes are called \(scene.id)")
        scenes.append(scene)
    }

    /// The sections that have scenes, in sidebar order, so an empty one never shows as a heading.
    var sections: [HarnessSection] {
        HarnessSection.allCases.filter { section in scenes.contains { $0.section == section } }
    }

    func scenes(in section: HarnessSection) -> [HarnessScene] {
        scenes.filter { $0.section == section }
    }

    func scene(id: String?) -> HarnessScene? {
        scenes.first { $0.id == id }
    }
}
