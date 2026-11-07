// Redlamp's theme catalog: the data of packages/RedlampDesign/Sources/Themes/ThemeCatalog.swift and the brand's colours
// in Tokens/Brand.swift, at Redlamp's 9fa9d915540782ce600ee6e4854dc3f2ccd15250, relicensed by its owner for jaw under
// jaw's MIT licence. scripts/sync-themes keeps each region between its `// sync-themes:` markers line for line as Redlamp has it.

import Foundation

/// A theme's ten colours, in the layout omoi-station's themes are authored in. jaw's roles come from
/// these by `RedlampRoles`.
public struct ThemePalette: Sendable, Hashable {
    public var background, card, foreground, muted, primary, accent, border, success, destructive, neutral: RGBA
    /// The tokens that take `primary`. The rest take `neutral`, so a theme whose primary
    /// must stay rare (the brand red) can keep it to one place.
    public var primaryRoles: PrimaryRoles

    public init(
        background: RGBA, card: RGBA, foreground: RGBA, muted: RGBA, primary: RGBA, accent: RGBA,
        border: RGBA, success: RGBA, destructive: RGBA, neutral: RGBA, primaryRoles: PrimaryRoles = .all,
    ) {
        self.background = background
        self.card = card
        self.foreground = foreground
        self.muted = muted
        self.primary = primary
        self.accent = accent
        self.border = border
        self.success = success
        self.destructive = destructive
        self.neutral = neutral
        self.primaryRoles = primaryRoles
    }
}

public struct PrimaryRoles: OptionSet, Sendable, Hashable {
    public let rawValue: Int

    public init(rawValue: Int) {
        self.rawValue = rawValue
    }

    public static let accent = PrimaryRoles(rawValue: 1 << 0)
    public static let selection = PrimaryRoles(rawValue: 1 << 1)
    public static let editedDot = PrimaryRoles(rawValue: 1 << 2)
    public static let all: PrimaryRoles = [.accent, .selection, .editedDot]
}

/// Redlamp's families of ten colours, each a dark and a light half, in Redlamp's slot order. Neutral,
/// which Redlamp authors as its own greys rather than ten colours, isn't among them:
/// `RedlampRoles.neutral` has it.
public enum RedlampCatalog {
    /// A family: its ID and its name, as Redlamp has them, and its two halves.
    public struct Family: Sendable, Hashable {
        public var id: String
        public var name: String
        public var dark: ThemePalette
        public var light: ThemePalette
    }

    public static let families: [Family] = [
        Family(id: "redlamp", name: "Redlamp", dark: redlampDark, light: redlampLight),
        // swiftformat:disable all
        // sync-themes: families
        // omoi-station classic palettes
        family("kanagawa", "Kanagawa",
               dark: pal("240 16% 14%", "240 16% 11%", "46 37% 77%", "258 13% 60%", "222 50% 67%", "34 100% 62%", "240 10% 26%", "88 32% 58%", "354 65% 65%", "258 13% 60%"),
               light: pal("40 40% 95%", "40 30% 92%", "240 16% 20%", "42 26% 46%", "222 50% 52%", "34 100% 48%", "40 15% 82%", "88 32% 45%", "0 83% 48%", "42 26% 46%")),
        family("tokyo-night", "Tokyo Night",
               dark: pal("230 19% 18%", "230 23% 16%", "226 84% 88%", "228 20% 44%", "217 92% 72%", "267 82% 75%", "230 16% 30%", "88 57% 61%", "355 75% 72%", "228 20% 44%"),
               light: pal("234 14% 89%", "233 11% 84%", "230 13% 20%", "230 18% 50%", "217 76% 55%", "266 87% 64%", "233 11% 78%", "90 34% 34%", "348 93% 56%", "230 18% 50%")),
        family("moonlight", "Moonlight",
               dark: pal("233 27% 17%", "233 25% 13%", "224 82% 88%", "228 29% 52%", "220 100% 75%", "19 100% 71%", "231 20% 30%", "85 65% 73%", "355 100% 73%", "228 29% 52%"),
               light: pal("235 25% 92%", "235 20% 88%", "230 23% 14%", "231 15% 45%", "224 70% 50%", "19 100% 60%", "235 15% 80%", "155 50% 38%", "355 90% 55%", "231 15% 50%")),
        family("catppuccin", "Catppuccin",
               dark: pal("240 21% 15%", "240 21% 12%", "226 64% 88%", "228 17% 63%", "267 84% 81%", "23 92% 75%", "237 12% 31%", "115 54% 76%", "351 74% 73%", "228 17% 63%"),
               light: pal("220 23% 95%", "220 22% 92%", "234 16% 35%", "233 10% 47%", "266 85% 58%", "22 99% 52%", "225 14% 77%", "109 58% 40%", "347 87% 44%", "228 11% 53%")),
        family("rose-pine", "Rosé Pine",
               dark: pal("249 22% 12%", "249 14% 15%", "245 50% 91%", "253 10% 47%", "343 76% 68%", "35 91% 72%", "250 12% 25%", "197 49% 38%", "343 76% 68%", "253 10% 47%"),
               light: pal("33 54% 95%", "33 100% 97%", "248 19% 40%", "257 7% 62%", "343 35% 55%", "35 84% 56%", "25 25% 88%", "197 50% 33%", "343 35% 55%", "257 7% 62%")),
        family("nord", "Nord",
               dark: pal("220 16% 22%", "220 16% 26%", "219 28% 88%", "220 16% 55%", "213 32% 63%", "193 43% 67%", "220 16% 36%", "92 28% 65%", "354 42% 56%", "220 16% 55%"),
               light: pal("220 16% 94%", "220 16% 90%", "220 16% 22%", "220 16% 36%", "213 32% 52%", "92 28% 65%", "220 16% 82%", "92 28% 50%", "354 42% 56%", "220 16% 50%")),
        family("everforest", "Everforest",
               dark: pal("192 15% 19%", "192 12% 24%", "38 23% 74%", "120 5% 50%", "95 35% 63%", "24 74% 68%", "195 10% 33%", "95 35% 63%", "359 73% 70%", "120 5% 50%"),
               light: pal("47 55% 94%", "48 38% 90%", "195 12% 40%", "110 8% 55%", "92 99% 32%", "24 93% 55%", "45 18% 84%", "92 99% 32%", "1 92% 65%", "110 8% 55%")),
        family("gruvbox", "Gruvbox",
               dark: pal("0 0% 16%", "20 5% 22%", "40 57% 81%", "35 16% 59%", "27 99% 55%", "42 95% 58%", "27 10% 36%", "64 65% 44%", "6 96% 59%", "0 7% 52%"),
               light: pal("47 76% 93%", "44 68% 90%", "0 0% 16%", "30 12% 42%", "24 88% 45%", "42 95% 58%", "33 30% 76%", "60 73% 35%", "2 75% 46%", "0 7% 52%")),
        family("dracula", "Dracula",
               dark: pal("231 15% 18%", "232 14% 22%", "60 30% 96%", "225 27% 51%", "265 89% 78%", "326 100% 74%", "232 14% 31%", "135 94% 65%", "0 100% 67%", "225 27% 51%"),
               light: pal("231 40% 96%", "231 30% 93%", "231 15% 18%", "225 20% 45%", "265 89% 55%", "326 80% 52%", "231 20% 83%", "135 60% 38%", "0 100% 50%", "225 20% 50%")),
        family("solarized", "Solarized",
               dark: pal("193 100% 11%", "192 81% 14%", "180 7% 73%", "194 14% 40%", "205 82% 48%", "45 100% 35%", "192 40% 20%", "68 100% 30%", "1 71% 52%", "194 14% 40%"),
               light: pal("44 87% 94%", "44 44% 90%", "192 81% 14%", "194 14% 40%", "205 82% 48%", "45 100% 35%", "44 25% 83%", "68 100% 30%", "1 71% 52%", "194 14% 40%")),
        family("one-dark", "One Dark",
               dark: pal("220 13% 18%", "220 14% 15%", "219 14% 71%", "220 14% 45%", "207 82% 66%", "286 60% 67%", "220 13% 28%", "95 38% 62%", "355 65% 65%", "220 14% 45%"),
               light: pal("230 8% 97%", "230 6% 93%", "230 8% 24%", "230 6% 50%", "222 87% 55%", "301 62% 40%", "230 5% 83%", "120 34% 42%", "4 72% 55%", "230 6% 50%")),
        family("monokai", "Monokai",
               dark: pal("70 8% 15%", "70 8% 11%", "60 30% 96%", "50 11% 41%", "338 95% 56%", "80 76% 53%", "70 6% 25%", "80 76% 53%", "338 95% 56%", "50 11% 41%"),
               light: pal("60 10% 96%", "60 8% 93%", "70 8% 15%", "50 11% 41%", "338 95% 45%", "80 76% 42%", "50 8% 82%", "80 76% 38%", "338 95% 45%", "50 11% 45%")),
        family("ocean", "Ocean",
               dark: pal("222 47% 11%", "217 33% 17%", "214 32% 91%", "215 20% 65%", "170 70% 59%", "38 92% 50%", "217 15% 28%", "160 60% 55%", "0 91% 71%", "215 20% 55%"),
               light: pal("170 50% 98%", "170 35% 95%", "215 28% 17%", "215 16% 47%", "174 61% 31%", "38 92% 50%", "170 15% 85%", "160 60% 35%", "0 84% 60%", "215 16% 47%")),
        family("sakura", "Sakura",
               dark: pal("260 20% 12%", "260 15% 16%", "20 30% 88%", "260 8% 50%", "340 68% 69%", "120 25% 65%", "260 10% 27%", "120 30% 60%", "0 80% 65%", "260 10% 55%"),
               light: pal("20 60% 98%", "20 35% 95%", "260 15% 15%", "260 8% 50%", "340 55% 55%", "120 25% 56%", "20 15% 85%", "120 30% 45%", "0 70% 55%", "260 8% 55%")),
        family("copper", "Copper",
               dark: pal("0 0% 11%", "0 0% 15%", "30 18% 82%", "20 6% 50%", "28 59% 60%", "0 30% 63%", "0 0% 25%", "140 35% 55%", "0 72% 62%", "20 6% 55%"),
               light: pal("30 24% 95%", "30 18% 91%", "0 0% 12%", "20 6% 45%", "28 60% 47%", "0 30% 63%", "30 12% 80%", "140 35% 40%", "0 65% 50%", "20 6% 50%")),
        // omoi-station originals
        family("mc-vinyl-noir", "Vinyl Noir",
               dark: pal("30 8% 10%", "30 6% 14%", "35 18% 78%", "30 10% 48%", "35 55% 58%", "42 40% 62%", "30 6% 22%", "145 25% 55%", "10 55% 58%", "30 10% 48%"),
               light: pal("35 20% 95%", "35 15% 91%", "30 5% 15%", "30 8% 45%", "32 45% 42%", "42 35% 50%", "35 10% 80%", "145 25% 40%", "10 50% 48%", "30 8% 48%")),
        family("mc-neon-dusk", "Neon Dusk",
               dark: pal("250 15% 12%", "250 12% 16%", "260 15% 80%", "260 8% 48%", "320 40% 65%", "175 35% 58%", "250 8% 26%", "160 30% 55%", "350 45% 62%", "260 8% 50%"),
               light: pal("260 12% 96%", "260 10% 93%", "250 10% 18%", "260 6% 48%", "320 35% 48%", "175 30% 42%", "260 6% 82%", "160 25% 42%", "350 40% 48%", "260 6% 50%")),
        family("mc-burnt-tape", "Burnt Tape",
               dark: pal("20 15% 10%", "20 12% 14%", "30 15% 76%", "20 8% 46%", "18 50% 56%", "38 45% 58%", "20 8% 22%", "150 22% 52%", "5 50% 55%", "20 8% 48%"),
               light: pal("30 30% 94%", "30 22% 90%", "20 12% 18%", "20 8% 48%", "16 45% 42%", "38 40% 48%", "28 12% 80%", "150 22% 42%", "5 45% 45%", "20 8% 50%")),
        family("mc-phantom-frequency", "Phantom Frequency",
               dark: pal("220 18% 11%", "220 15% 15%", "210 18% 78%", "215 10% 46%", "240 28% 65%", "168 30% 55%", "220 10% 24%", "155 25% 55%", "345 40% 60%", "215 10% 48%"),
               light: pal("210 20% 96%", "210 15% 93%", "220 15% 18%", "215 10% 48%", "240 22% 48%", "168 25% 48%", "210 10% 82%", "155 22% 42%", "345 35% 48%", "215 10% 50%")),
        family("mc-velvet-static", "Velvet Static",
               dark: pal("280 15% 10%", "280 12% 14%", "300 12% 80%", "290 8% 46%", "335 35% 62%", "45 35% 62%", "280 8% 24%", "155 22% 55%", "350 45% 60%", "290 8% 48%"),
               light: pal("300 10% 96%", "300 8% 93%", "280 10% 16%", "290 6% 48%", "330 28% 42%", "45 30% 52%", "300 5% 82%", "155 20% 42%", "350 40% 45%", "290 6% 48%")),
        // sync-themes: end
        // swiftformat:enable all
    ]

    // MARK: - Redlamp

    // The brand's room: warm near-black, paper text, steel edges. The red is kept to the accent, since
    // the brand allows it once per view (Redlamp's docs/brand/README.md).
    // sync-themes: redlamp
    static let redlampDark = ThemePalette(
        background: Brand.wall, card: Brand.bakelite, foreground: Brand.paper,
        muted: Brand.ring.mixed(with: Brand.steel, amount: 0.5),
        primary: Brand.safelight, accent: Brand.filament, border: Brand.steel,
        success: hsl("95 22% 52%"), destructive: hsl("6 40% 50%"), neutral: Brand.ring,
        primaryRoles: .accent,
    )

    static let redlampLight = ThemePalette(
        background: Brand.paper, card: Brand.paper.mixed(with: Brand.ink, amount: 0.04), foreground: Brand.ink,
        muted: Brand.steel.mixed(with: Brand.paper, amount: 0.3), primary: Brand.safelightOnLight,
        accent: Brand.safelight, border: Brand.ring,
        success: hsl("95 25% 38%"), destructive: hsl("6 45% 40%"), neutral: Brand.steel,
        primaryRoles: .accent,
    )
    // sync-themes: end

    /// Redlamp's brand colours, which the Redlamp family is made of.
    enum Brand {
        // sync-themes: brand
        static let safelight = RGBA(red: 224 / 255, green: 64 / 255, blue: 46 / 255)
        static let safelightOnLight = RGBA(red: 216 / 255, green: 53 / 255, blue: 42 / 255)
        static let filament = RGBA(red: 255 / 255, green: 176 / 255, blue: 138 / 255)
        static let wall = RGBA(red: 10 / 255, green: 7 / 255, blue: 7 / 255)
        static let bakelite = RGBA(red: 34 / 255, green: 28 / 255, blue: 26 / 255)
        static let steel = RGBA(red: 87 / 255, green: 80 / 255, blue: 78 / 255)
        static let ring = RGBA(red: 217 / 255, green: 208 / 255, blue: 203 / 255)
        static let paper = RGBA(red: 243 / 255, green: 238 / 255, blue: 232 / 255)
        static let ink = RGBA(red: 26 / 255, green: 20 / 255, blue: 20 / 255)
        // sync-themes: end
    }

    // MARK: - Builder

    private static func family(_ id: String, _ name: String, dark: ThemePalette, light: ThemePalette) -> Family {
        Family(id: id, name: name, dark: dark, light: light)
    }

    private static func pal(
        _ background: String, _ card: String, _ foreground: String, _ muted: String, _ primary: String,
        _ accent: String, _ border: String, _ success: String, _ destructive: String, _ neutral: String,
    ) -> ThemePalette {
        ThemePalette(
            background: hsl(background), card: hsl(card), foreground: hsl(foreground), muted: hsl(muted),
            primary: hsl(primary), accent: hsl(accent), border: hsl(border), success: hsl(success),
            destructive: hsl(destructive), neutral: hsl(neutral),
        )
    }

    /// Parses `"H S% L%"`. The palettes are authored data, so malformed input only guards
    /// typos and yields black.
    static func hsl(_ string: String) -> RGBA {
        let parts = string.split(whereSeparator: { $0 == " " || $0 == "%" }).compactMap { Double($0) }
        guard parts.count >= 3 else { return RGBA(white: 0) }
        return RGBA(hue: parts[0], saturation: parts[1] / 100, lightness: parts[2] / 100)
    }
}

extension RGBA {
    /// From its channels, as Redlamp writes its brand's colours.
    init(red: Double, green: Double, blue: Double, alpha: Double = 1) {
        self.init(r: red, g: green, b: blue, a: alpha)
    }

    /// A grey.
    init(white: Double, alpha: Double = 1) {
        self.init(r: white, g: white, b: white, a: alpha)
    }

    /// Hue in degrees (wrapped), saturation and lightness 0...1: the HSL model themes are
    /// authored in.
    init(hue degrees: Double, saturation: Double, lightness: Double, alpha: Double = 1) {
        let hue = ((degrees.truncatingRemainder(dividingBy: 360)) + 360).truncatingRemainder(dividingBy: 360)
        let a = saturation * min(lightness, 1 - lightness)
        func channel(_ n: Double) -> Double {
            let k = (n + hue / 30).truncatingRemainder(dividingBy: 12)
            return lightness - a * max(-1, min(k - 3, 9 - k, 1))
        }
        self.init(r: channel(0), g: channel(8), b: channel(4), a: alpha)
    }

    /// `amount` of the way to `other`, as Redlamp writes it.
    func mixed(with other: RGBA, amount: Double) -> RGBA {
        mixed(with: other, amount)
    }
}
