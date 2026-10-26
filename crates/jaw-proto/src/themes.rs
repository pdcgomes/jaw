//! Omarchy's themes, as Omarchy wrote them: protocol/themes/<id>.toml is each theme's colors.toml
//! at the commit scripts/omarchy-themes pins (MIT, docs/licenses/omarchy.txt). jawd serves them
//! with `themes`, and each interface maps their colours onto what it draws.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The theme every interface starts in.
pub const DEFAULT: &str = "tokyo-night";

/// Each theme's ID and its file, ordered by ID.
const FILES: [(&str, &str); 23] = [
    ("catppuccin", include_str!("../../../protocol/themes/catppuccin.toml")),
    ("catppuccin-latte", include_str!("../../../protocol/themes/catppuccin-latte.toml")),
    ("ethereal", include_str!("../../../protocol/themes/ethereal.toml")),
    ("everforest", include_str!("../../../protocol/themes/everforest.toml")),
    ("flexoki-light", include_str!("../../../protocol/themes/flexoki-light.toml")),
    ("gruvbox", include_str!("../../../protocol/themes/gruvbox.toml")),
    ("hackerman", include_str!("../../../protocol/themes/hackerman.toml")),
    ("kanagawa", include_str!("../../../protocol/themes/kanagawa.toml")),
    ("last-horizon", include_str!("../../../protocol/themes/last-horizon.toml")),
    ("lumon", include_str!("../../../protocol/themes/lumon.toml")),
    ("lupine", include_str!("../../../protocol/themes/lupine.toml")),
    ("matte-black", include_str!("../../../protocol/themes/matte-black.toml")),
    ("miasma", include_str!("../../../protocol/themes/miasma.toml")),
    ("nord", include_str!("../../../protocol/themes/nord.toml")),
    ("osaka-jade", include_str!("../../../protocol/themes/osaka-jade.toml")),
    ("retro-82", include_str!("../../../protocol/themes/retro-82.toml")),
    ("ristretto", include_str!("../../../protocol/themes/ristretto.toml")),
    ("rose-pine", include_str!("../../../protocol/themes/rose-pine.toml")),
    ("solitude", include_str!("../../../protocol/themes/solitude.toml")),
    ("starship", include_str!("../../../protocol/themes/starship.toml")),
    ("tokyo-night", include_str!("../../../protocol/themes/tokyo-night.toml")),
    ("vantablack", include_str!("../../../protocol/themes/vantablack.toml")),
    ("white", include_str!("../../../protocol/themes/white.toml")),
];

/// Whether a theme is light text on a dark ground, or dark on light.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Dark,
    Light,
}

/// A colour as 0xRRGGBB, written `#rrggbb` on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub u32);

impl Rgb {
    /// `#rrggbb`, in either case.
    pub fn parse(text: &str) -> Option<Rgb> {
        let hex = text.strip_prefix('#')?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(hex, 16).ok().map(Rgb)
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("#{:06x}", self.0))
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Rgb, D::Error> {
        let text = String::deserialize(d)?;
        Rgb::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("{text:?} isn't a #rrggbb colour")))
    }
}

/// A theme's colours, by Omarchy's names for them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Colors {
    pub accent: Rgb,
    pub selection: Rgb,
    pub muted: Rgb,
    pub background: Rgb,
    pub dark_background: Rgb,
    pub darker_background: Rgb,
    pub lighter_background: Rgb,
    pub foreground: Rgb,
    pub dark_foreground: Rgb,
    pub light_foreground: Rgb,
    pub bright_foreground: Rgb,
    pub red: Rgb,
    pub yellow: Rgb,
    pub orange: Rgb,
    pub green: Rgb,
    pub cyan: Rgb,
    pub blue: Rgb,
    pub magenta: Rgb,
    pub brown: Rgb,
    pub bright_red: Rgb,
    pub bright_yellow: Rgb,
    pub bright_green: Rgb,
    pub bright_cyan: Rgb,
    pub bright_blue: Rgb,
    pub bright_magenta: Rgb,
}

/// One of Omarchy's themes, as `themes` serves it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub id: String,
    /// What it's shown as: "Tokyo Night".
    pub name: String,
    pub mode: Mode,
    pub colors: Colors,
}

/// `themes`'s result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Themes {
    pub themes: Vec<Theme>,
}

/// Every theme, ordered by ID, each read from its file once.
pub fn all() -> &'static [Theme] {
    static ALL: OnceLock<Vec<Theme>> = OnceLock::new();
    ALL.get_or_init(|| {
        FILES
            .iter()
            .map(|(id, text)| parse(id, text).unwrap_or_else(|e| panic!("protocol/themes/{id}.toml: {e}")))
            .collect()
    })
}

pub fn get(id: &str) -> Option<&'static Theme> {
    all().iter().find(|t| t.id == id)
}

/// The theme `id` names, or the default where it names none.
pub fn or_default(id: &str) -> &'static Theme {
    get(id).or_else(|| get(DEFAULT)).expect("the default theme is one of them")
}

/// What a theme is shown as: its ID's words capitalised, and Rosé Pine with its accent.
pub fn name(id: &str) -> String {
    if id == "rose-pine" {
        return "Rosé Pine".into();
    }
    id.split('-')
        .map(|word| {
            let mut letters = word.chars();
            letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// A colors.toml read: its `key = "value"` lines, `mode` and the colours, with any other key left
/// alone; `orange` and `brown` are `yellow` where it leaves them out, as Omarchy makes them.
pub fn parse(id: &str, text: &str) -> Result<Theme, String> {
    let mut values: HashMap<&str, &str> = HashMap::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("{line:?} isn't key = \"value\""));
        };
        values.insert(key.trim(), value.trim().trim_matches('"'));
    }
    let mode = match values.get("mode").copied() {
        Some("dark") => Mode::Dark,
        Some("light") => Mode::Light,
        other => return Err(format!("its mode is {other:?}, not dark or light")),
    };
    let colour = |key: &str| -> Result<Rgb, String> {
        let value = values.get(key).ok_or_else(|| format!("it has no {key}"))?;
        Rgb::parse(value).ok_or_else(|| format!("its {key} is {value:?}, not #rrggbb"))
    };
    let yellow = colour("yellow")?;
    let or_yellow = |key: &str| if values.contains_key(key) { colour(key) } else { Ok(yellow) };
    Ok(Theme {
        id: id.to_string(),
        name: name(id),
        mode,
        colors: Colors {
            accent: colour("accent")?,
            selection: colour("selection")?,
            muted: colour("muted")?,
            background: colour("background")?,
            dark_background: colour("dark_background")?,
            darker_background: colour("darker_background")?,
            lighter_background: colour("lighter_background")?,
            foreground: colour("foreground")?,
            dark_foreground: colour("dark_foreground")?,
            light_foreground: colour("light_foreground")?,
            bright_foreground: colour("bright_foreground")?,
            red: colour("red")?,
            yellow,
            orange: or_yellow("orange")?,
            green: colour("green")?,
            cyan: colour("cyan")?,
            blue: colour("blue")?,
            magenta: colour("magenta")?,
            brown: or_yellow("brown")?,
            bright_red: colour("bright_red")?,
            bright_yellow: colour("bright_yellow")?,
            bright_green: colour("bright_green")?,
            bright_cyan: colour("bright_cyan")?,
            bright_blue: colour("bright_blue")?,
            bright_magenta: colour("bright_magenta")?,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_file_reads_with_its_mode_and_every_colour() {
        let ids: Vec<&str> = all().iter().map(|t| t.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "catppuccin",
                "catppuccin-latte",
                "ethereal",
                "everforest",
                "flexoki-light",
                "gruvbox",
                "hackerman",
                "kanagawa",
                "last-horizon",
                "lumon",
                "lupine",
                "matte-black",
                "miasma",
                "nord",
                "osaka-jade",
                "retro-82",
                "ristretto",
                "rose-pine",
                "solitude",
                "starship",
                "tokyo-night",
                "vantablack",
                "white",
            ]
        );
        let light: Vec<&str> = all().iter().filter(|t| t.mode == Mode::Light).map(|t| t.id.as_str()).collect();
        assert_eq!(light, ["catppuccin-latte", "flexoki-light", "lupine", "rose-pine", "white"]);
        let tokyo = get("tokyo-night").unwrap();
        assert_eq!(tokyo.name, "Tokyo Night");
        assert_eq!(
            (tokyo.colors.background, tokyo.colors.accent, tokyo.colors.orange),
            (Rgb(0x1a1b26), Rgb(0x7aa2f7), Rgb(0xeb927b))
        );
        assert_eq!(get("flexoki-light").unwrap().colors.background, Rgb(0xfffcf0), "hex in either case");
    }

    #[test]
    fn orange_and_brown_are_yellow_where_a_theme_leaves_them_out() {
        for id in ["last-horizon", "solitude", "white"] {
            let c = get(id).unwrap().colors;
            assert_eq!((c.orange, c.brown), (c.yellow, c.yellow), "{id}");
        }
        let gruvbox = get("gruvbox").unwrap().colors;
        assert_ne!(gruvbox.orange, gruvbox.yellow, "a theme's own orange is kept");
    }

    #[test]
    fn a_file_missing_a_colour_or_with_one_that_isnt_hex_is_refused() {
        let gruvbox = FILES.iter().find(|(id, _)| *id == "gruvbox").unwrap().1;
        assert!(parse("x", &gruvbox.replace("accent = ", "# accent = ")).unwrap_err().contains("accent"));
        assert!(
            parse("x", &gruvbox.replace("accent = \"#7daea3\"", "accent = \"teal\"")).unwrap_err().contains("accent")
        );
        assert!(parse("x", &gruvbox.replace("mode = \"dark\"", "mode = \"dim\"")).unwrap_err().contains("mode"));
        let more = format!("{gruvbox}\nhyprland_active_border = \"rgba(26a269ee) 45deg\"\n");
        assert_eq!(parse("gruvbox", &more).unwrap(), *get("gruvbox").unwrap(), "a key it doesn't use is left alone");
    }

    #[test]
    fn a_theme_is_shown_by_its_ids_words_and_an_unknown_one_is_the_default() {
        assert_eq!(name("catppuccin-latte"), "Catppuccin Latte");
        assert_eq!(name("retro-82"), "Retro 82");
        assert_eq!(name("rose-pine"), "Rosé Pine");
        assert_eq!(or_default("no-such-theme").id, DEFAULT);
        assert_eq!(or_default("nord").id, "nord");
    }

    #[test]
    fn colours_are_written_as_lowercase_hex() {
        assert_eq!(serde_json::to_value(Rgb(0x0b0c16)).unwrap(), "#0b0c16");
        assert_eq!(serde_json::from_value::<Rgb>(serde_json::json!("#FFFCF0")).unwrap(), Rgb(0xfffcf0));
        assert!(serde_json::from_value::<Rgb>(serde_json::json!("fffcf0")).is_err());
        assert!(serde_json::from_value::<Rgb>(serde_json::json!("#fffcf")).is_err());
    }
}
