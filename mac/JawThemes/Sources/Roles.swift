import Foundation

/// Every colour the app draws, by what it's for, as the terminal's `theme.rs` asks for them, never
/// by hue, so another theme is another set of roles.
public struct Roles: Sendable, Hashable {
    public var isDark: Bool
    /// The window's ground.
    public var ground: RGBA
    /// The sidebar, the list, and their bubbles.
    public var surface: RGBA
    /// The selected row.
    public var selection: RGBA
    /// Text that matters.
    public var text: RGBA
    /// Secondary text: previews, times.
    public var secondary: RGBA
    /// Quieter still: ticks not yet read, placeholders.
    public var tertiary: RGBA
    /// Separators and borders at rest.
    public var separator: RGBA
    /// Focus, selection, unread counts: the accent's fills and outlines, as the theme wrote it.
    public var accent: RGBA
    /// The accent where JawUI draws it as text or as a mark: an unread chat's time, Draft:, a mention of
    /// someone else, Show more, You in a reply, the palette's and the library's marks, a card's action;
    /// the accent, until `held()` moves it to read where it's drawn.
    public var accentText: RGBA
    /// Links and read ticks: the accent in Jaw's and Redlamp's themes, the terminal's cyan in Omarchy's.
    public var link: RGBA
    /// Your bubbles.
    public var yours: RGBA
    /// Mentions, and the "new" marker.
    public var mention: RGBA
    /// Online, typing.
    public var online: RGBA
    /// Online and typing on the selected row, as `online` reads too faintly on some selections.
    public var selectedOnline: RGBA
    /// Failed, destructive.
    public var failed: RGBA
    /// Group senders' names, by the index jawd gives each member.
    public var senders: [RGBA]

    public func sender(_ colour: Int) -> RGBA {
        senders[((colour % senders.count) + senders.count) % senders.count]
    }
}

/// A theme as a matched dark and light pair, so a choice survives an appearance change.
public struct ThemeFamily: Sendable, Hashable, Identifiable {
    public var id: String
    public var name: String
    public var dark: Roles
    public var light: Roles

    public func roles(dark isDark: Bool) -> Roles {
        isDark ? dark : light
    }
}

/// Dark, light, or the system's.
public enum Appearance: String, Sendable, Hashable, CaseIterable {
    case system, dark, light
}

public enum Catalog {
    /// jaw's own, first and the default: ooze's Tokyo Night, the terminal's palette, with Tokyo Night
    /// Day as its light half, both held as every theme is (`Roles.held()`). `Catalog.themes` lists it
    /// first, as `jawTheme`, before Omarchy's and Redlamp's.
    public static let jaw = ThemeFamily(id: "jaw", name: "jaw", dark: tokyoNight.held(), light: tokyoNightDay.held())

    /// Tokyo Night, as `crates/jaw/src/theme.rs` has it.
    static let tokyoNight: Roles = {
        let bg = RGBA(0x1A1B26), blue = RGBA(0x7AA2F7)
        let (red, yellow, purple, cyan, green, orange, teal) =
            (RGBA(0xF7768E), RGBA(0xE0AF68), RGBA(0xBB9AF7), RGBA(0x7DCFFF), RGBA(0x9ECE6A), RGBA(0xFF9E64), RGBA(0x73DACA))
        return Roles(
            isDark: true,
            ground: bg,
            surface: RGBA(0x24283B),
            selection: RGBA(0x283457),
            text: RGBA(0xC0CAF5),
            secondary: RGBA(0x737AA2),
            tertiary: RGBA(0x565F89),
            separator: RGBA(0x3B4261),
            accent: blue, accentText: blue, link: blue,
            yours: blue.opacity(0.18).over(bg),
            mention: orange,
            online: green,
            selectedOnline: green,
            failed: red,
            senders: [red, yellow, blue, purple, cyan, green, orange, teal],
        )
    }()

    /// Tokyo Night Day, Tokyo Night's own light palette, from tokyonight.nvim's day style.
    static let tokyoNightDay: Roles = {
        let bg = RGBA(0xE1E2E7), blue = RGBA(0x2E7DE9)
        let (red, yellow, purple, cyan, green, orange, teal) =
            (RGBA(0xF52A65), RGBA(0x8C6C3E), RGBA(0x7847BD), RGBA(0x007197), RGBA(0x587539), RGBA(0xB15C00), RGBA(0x118C74))
        return Roles(
            isDark: false,
            ground: bg,
            surface: RGBA(0xE9E9EC),
            selection: RGBA(0xB7C1E3),
            // Tokyo Night Day's foreground reads at 4.0:1 on its ground and 3.3:1 on its selection;
            // taken 40% toward Tokyo Night's ground, messages read at 4.5:1 on both.
            text: RGBA(0x3760BF).mixed(with: RGBA(0x1A1B26), 0.4),
            secondary: RGBA(0x6172B0).mixed(with: RGBA(0x1A1B26), 0.15),
            tertiary: RGBA(0x848CB5),
            separator: RGBA(0xA8AECB),
            accent: blue, accentText: blue, link: blue,
            yours: blue.opacity(0.12).over(bg),
            mention: orange,
            online: green,
            // Its green reads at 2.9:1 on its selection; taken 40% toward Tokyo Night's ground, as its
            // text is, a selected row's typing reads at 4.8:1 there and stays green.
            selectedOnline: green.mixed(with: RGBA(0x1A1B26), 0.4),
            failed: red,
            senders: [red, yellow, blue, purple, cyan, green, orange, teal],
        )
    }()
}
