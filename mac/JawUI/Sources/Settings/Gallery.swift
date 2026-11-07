import JawThemes

/// The gallery for a slot: Jaw's, Omarchy's and Redlamp's sections, each the themes with a half for
/// the slot in the catalog's order, three across, with what's said in place of Omarchy's while jawd
/// hasn't served them; where the arrows go; and what VoiceOver reads a card as.
public struct Gallery: Sendable, Hashable {
    /// A section: its source, its themes, and the line said in place of those jawd hasn't served.
    public struct Section: Sendable, Hashable, Identifiable {
        public var source: ThemeSource
        public var themes: [Theme]
        public var line: String?

        public init(source: ThemeSource, themes: [Theme], line: String? = nil) {
            self.source = source
            self.themes = themes
            self.line = line
        }

        public var id: ThemeSource {
            source
        }

        /// Its heading: its source's name, and where they come from while jawd's themes aren't in.
        public var header: String {
            line == nil ? source.title : "\(source.title), from jawd"
        }

        /// What's said under it, as the wireframes have it: the line in jawd's state while jawd's
        /// themes aren't in, or what the section holds.
        public var footer: String {
            if let line { return line }
            switch source {
            case .jaw: return "Jaw's own Tokyo Night, with Tokyo Night Day as its light half."
            case .omarchy: return "As the terminal draws them; jawd serves them."
            case .redlamp: return "Redlamp's theme families, each a dark and a light half."
            }
        }
    }

    /// An arrow key.
    public enum Arrow: Sendable {
        case left, right, up, down
    }

    public var slot: Slot
    public var sections: [Section]

    public init(_ themes: [Theme], for slot: Slot, omarchyLine: String?) {
        self.slot = slot
        sections = ThemeSource.allCases.map { source in
            Section(source: source, themes: themes.filter { $0.source == source && $0.roles(slot) != nil },
                    line: source == .omarchy ? omarchyLine : nil)
        }
    }

    /// Every card, in the gallery's order, across its sections.
    public var cards: [Theme] {
        sections.flatMap(\.themes)
    }

    /// The card an arrow reaches from the theme `id`: ← and → the one before or after in the gallery's
    /// order, across its sections; ↑ and ↓ the one above or below in its column, three across, past a
    /// section's edge the next section's first row or the last row of the one before, the row's last
    /// card where that row is shorter. Nil past the gallery's ends, or from a theme it hasn't.
    public func moving(from id: String, _ arrow: Arrow) -> Theme? {
        let cards = cards
        guard let at = cards.firstIndex(where: { $0.id == id }) else { return nil }
        switch arrow {
        case .left:
            return at > 0 ? cards[at - 1] : nil
        case .right:
            return at + 1 < cards.count ? cards[at + 1] : nil
        case .up, .down:
            let shown = sections.map(\.themes).filter { !$0.isEmpty }
            guard let s = shown.firstIndex(where: { $0.contains { $0.id == id } }),
                  let i = shown[s].firstIndex(where: { $0.id == id })
            else { return nil }
            let (row, column) = (i / 3, i % 3)
            if arrow == .down {
                if (row + 1) * 3 < shown[s].count { return shown[s][min((row + 1) * 3 + column, shown[s].count - 1)] }
                return s + 1 < shown.count ? shown[s + 1][min(column, shown[s + 1].count - 1)] : nil
            }
            if row > 0 { return shown[s][(row - 1) * 3 + column] }
            guard s > 0 else { return nil }
            let above = shown[s - 1]
            return above[min((above.count - 1) / 3 * 3 + column, above.count - 1)]
        }
    }

    /// What VoiceOver reads a card as: the name of the half it shows for `slot`, its section and the
    /// slots it fills, as in "Gruvbox, Omarchy's, the dark theme".
    public static func label(_ theme: Theme, for slot: Slot, _ slots: Slots) -> String {
        let fills = Slot.allCases.filter { slots[$0] == theme.id }.map(\.rawValue)
        let filling = fills.isEmpty ? "" : ", the \(fills.joined(separator: " and ")) theme\(fills.count > 1 ? "s" : "")"
        return "\(theme.name(slot)), \(theme.source.title)\(filling)"
    }
}

/// What a slot's tile says: its theme's name and source, or why it draws Tokyo Night, and that Jaw
/// draws it now, or when it would.
public struct SlotTile: Sendable, Hashable {
    public var slot: Slot
    public var name: String
    /// "In use", "Used when Appearance is Light" while Appearance holds the other, or nil.
    public var note: String?

    /// `served` is whether jawd has served Omarchy's themes, and `dark` whether Jaw draws dark now.
    /// Where the slot draws Jaw's in its theme's place, it names the half of Jaw's it draws.
    public init(_ slot: Slot, slots: Slots, themes: [Theme], served: Bool, appearance: Appearance, dark: Bool) {
        self.slot = slot
        let id = slots[slot], instead = Catalog.jawTheme.name(slot)
        if let theme = themes.first(where: { $0.id == id }) {
            name = theme.roles(slot) == nil ? "\(instead), as \(theme.name) has no \(slot.rawValue) half" : "\(theme.name(slot)), \(theme.source.title)"
        } else if id.hasPrefix("omarchy:") {
            let named = Omarchy.name(String(id.dropFirst("omarchy:".count)))
            name = served ? "\(named) isn't among jawd's themes" : "\(named), once jawd serves it"
        } else {
            name = "\(instead), as no source has \(id)"
        }
        note = slot.isDark == dark ? "In use" : appearance == .system ? nil : "Used when Appearance is \(slot.title)"
    }
}
