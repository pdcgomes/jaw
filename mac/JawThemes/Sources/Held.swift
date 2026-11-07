import Foundation

/// The colours JawUI draws as text or as marks, held readable wherever it draws them, after a theme's
/// own mapping and whichever source it comes from (Owner, 6 Nov): each moved toward the text, by the
/// straight line `theme.rs` moves a colour along, until it reads at 3:1 on the worst of the grounds
/// JawUI draws it on, the tints it lays over a ground among them, a selected row's online at 4.5:1 on
/// its selection. The accent's fills (the unread badge, the tints themselves, the focus outline), the
/// grounds, the text and the borders keep the colours the theme wrote.
public extension Roles {
    /// A ground JawUI fills with a theme's colours: the window's (a conversation in lines, the composer,
    /// the panes, the library, the glance), the surface (the chat list, the palette, the status line,
    /// the box's field, their bubbles), a selected row, your bubbles, and the band a selected message
    /// is drawn on in lines.
    enum Base: String, CaseIterable, Sendable {
        case ground, surface, selection, yours, band
    }

    /// A ground JawUI draws text or marks on: a base, or a tint it lays over one.
    enum Ground: Hashable, Sendable {
        /// The base itself.
        case plain(Base)
        /// A card's well on it (`well`): a picture's, an album's, a link's (its thumbnail's placeholder
        /// too) or a contact card's, and the library's tiles and, on a row not selected, link icons.
        case well(Base)
        /// A quote's well on it (`quoteWell`).
        case quote(Base)
        /// The accent's tint on it (`accentTint`), behind a card's symbols, the library's file symbol
        /// on a row not selected, and its header's mark.
        case accentTint(Base)
        /// The box's chip (`chipTint`), on its field, the surface.
        case chip
        /// The colour drawn on it, at 16% (`ownTint(of:)`): a mention of you.
        case ownTint(Base)
    }

    /// A colour JawUI draws as text or as a mark.
    enum Ink: String, CaseIterable, Sendable {
        case secondary, tertiary, accentText, link, mention, online, selectedOnline, failed, senders
    }

    /// A colour, the grounds JawUI draws it on, and the contrast it's held at on each.
    struct Hold: Sendable, Hashable {
        public var ink: Ink
        public var on: [Ground]
        public var at: Double

        public init(_ ink: Ink, on: [Ground], at: Double = 3) {
            self.ink = ink
            self.on = on
            self.at = at
        }
    }

    /// Each colour and the grounds JawUI's draw sites put it on. Cards, with their wells and the
    /// accent's tints, sit on the window's ground, the surface, your bubbles and a selected message's
    /// band, quotes on the surface and your bubbles; a selected library row draws its file and link
    /// marks on the selection itself, without their tints. The dim text is held on the window's ground
    /// and the surface alone: everywhere else JawUI draws it, on a selected row, your bubbles, the band
    /// (`faint(on:)`) and a card's or a tile's well, its words and marks take the secondary text's
    /// colour, as the dim text held at 3:1 there would come level with the secondary text, or near it.
    /// A selected row draws online as `selectedOnline`.
    static let holds: [Hold] = {
        let bases = Base.allCases.map(Ground.plain)
        let wells: [Ground] = [.well(.ground), .well(.surface), .well(.yours), .well(.band)]
        let quotes: [Ground] = [.quote(.surface), .quote(.yours)]
        let tints: [Ground] = [.accentTint(.ground), .accentTint(.surface), .accentTint(.yours), .accentTint(.band)]
        return [
            Hold(.secondary, on: bases + wells + quotes),
            Hold(.tertiary, on: [.plain(.ground), .plain(.surface)]),
            Hold(.accentText, on: bases + wells + quotes + tints + [.chip]),
            Hold(.link, on: bases + wells),
            Hold(.mention, on: bases + [.ownTint(.ground), .ownTint(.surface), .ownTint(.yours), .ownTint(.band)]),
            Hold(.online, on: [.plain(.ground), .plain(.surface)]),
            Hold(.selectedOnline, on: [.plain(.selection)], at: 4.5),
            Hold(.failed, on: bases),
            Hold(.senders, on: bases + quotes),
        ]
    }()

    /// A card's well: the text at 7%, 6% in light.
    var well: RGBA {
        text.opacity(isDark ? 0.07 : 0.06)
    }

    /// A quote's well: the text at 6%, 5% in light.
    var quoteWell: RGBA {
        text.opacity(isDark ? 0.06 : 0.05)
    }

    /// The accent's tint behind its symbols: the accent at 15%.
    var accentTint: RGBA {
        accent.opacity(0.15)
    }

    /// The box's chip: the accent at 16%, 12% in light.
    var chipTint: RGBA {
        accent.opacity(isDark ? 0.16 : 0.12)
    }

    /// What a mention of you is drawn on: its own colour's tint.
    var mentionTint: RGBA {
        ownTint(of: mention)
    }

    /// The band a selected message is drawn on in lines: the selection at 55%.
    var bandTint: RGBA {
        selection.opacity(0.55)
    }

    /// The tint of a colour drawn on its own colour: it at 16%.
    func ownTint(of colour: RGBA) -> RGBA {
        colour.opacity(0.16)
    }

    /// A base's colour, the band as it's drawn on the window's ground.
    subscript(_ base: Base) -> RGBA {
        switch base {
        case .ground: ground
        case .surface: surface
        case .selection: selection
        case .yours: yours
        case .band: bandTint.over(ground)
        }
    }

    /// The colour of `place` with `colour` drawn on it, which a colour's own tint is made from.
    func colour(of place: Ground, under colour: RGBA) -> RGBA {
        switch place {
        case let .plain(base): self[base]
        case let .well(base): well.over(self[base])
        case let .quote(base): quoteWell.over(self[base])
        case let .accentTint(base): accentTint.over(self[base])
        case .chip: chipTint.over(surface)
        case let .ownTint(base): ownTint(of: colour).over(self[base])
        }
    }

    /// A colour's values: the eight senders', or the one.
    func colours(_ ink: Ink) -> [RGBA] {
        switch ink {
        case .secondary: [secondary]
        case .tertiary: [tertiary]
        case .accentText: [accentText]
        case .link: [link]
        case .mention: [mention]
        case .online: [online]
        case .selectedOnline: [selectedOnline]
        case .failed: [failed]
        case .senders: senders
        }
    }

    /// What JawUI draws the dim text's words and marks in on `base`: the dim text on the window's
    /// ground and the surface, where it's held, and the secondary text's colour on a selected row, your
    /// bubbles and a selected message's band, where only that is.
    func faint(on base: Base) -> RGBA {
        base == .ground || base == .surface ? tertiary : secondary
    }

    /// `colour` moved toward the text until it reads at `at` on each of `grounds`, one after another
    /// with `Contrast.fit`, in passes, each working the grounds out from the colour the pass starts
    /// from, as a colour's own tint moves with it, until a pass moves nothing; and how many passes
    /// moved it. A pass moves each channel toward the text's or leaves it, never past it, so no more
    /// than 765 can move a colour, three channels of 255 steps, and the loop ends at 766 whatever the
    /// grounds.
    func lifted(_ colour: RGBA, on grounds: [Ground], at: Double) -> (colour: RGBA, passes: Int) {
        let end = text.hex
        var now = colour
        for passes in 0 ..< 766 {
            let start = now
            let moved = grounds.reduce(start.hex) { hex, place in
                Contrast.fit(hex, on: self.colour(of: place, under: start).hex, toward: end, low: at, target: at)
            }
            if moved == start.hex { return (start, passes) }
            now = RGBA(moved)
        }
        return (now, 766)
    }

    /// These roles with each colour in `holds` lifted to read at its contrast on every one of its
    /// grounds. A colour that reads already is kept exactly as it is, so roles that are held come
    /// through unmoved.
    func held() -> Roles {
        var out = self
        for hold in Self.holds {
            func lift(_ colour: RGBA) -> RGBA {
                lifted(colour, on: hold.on, at: hold.at).colour
            }
            switch hold.ink {
            case .secondary: out.secondary = lift(secondary)
            case .tertiary: out.tertiary = lift(tertiary)
            case .accentText: out.accentText = lift(accentText)
            case .link: out.link = lift(link)
            case .mention: out.mention = lift(mention)
            case .online: out.online = lift(online)
            case .selectedOnline: out.selectedOnline = lift(selectedOnline)
            case .failed: out.failed = lift(failed)
            case .senders: out.senders = senders.map(lift)
            }
        }
        return out
    }
}
