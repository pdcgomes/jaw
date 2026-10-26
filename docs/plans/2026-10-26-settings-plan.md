# Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Settings kept by jawd in a domain for each frontend (the terminal's: Omarchy's 23 themes, and where your messages sit), with a panel in jaw, the IRC layout and `jaw settings`, as `docs/plans/2026-10-26-settings-design.md` specifies.

**Architecture:** jaw-proto defines the terminal's settings and carries Omarchy's palettes, `protocol/themes/*.toml`, parsed once. jawd keeps each frontend's settings in a new table of jaw's own (migration 11), keyed by domain and setting, the domain being the `client` a connection's hello gives; it changes them one at a time with a synced commit, publishes each domain's on a tokio `watch` channel that its connections push from, and sends a connection its domain's in hello's answer. jaw maps a palette onto its colour roles with bands of contrast, follows the settings from hello, answers and pushes, draws the panel in the chat list's place, and lays your messages out under "You" in the IRC layout.

**Tech Stack:** Rust 2024 workspace (jaw-proto, jawd, jaw-client, jaw), rusqlite with SQLite bundled, tokio, ratatui and crossterm, serde; Python 3 for the theme script.

## Global Constraints

- The spec is `docs/plans/2026-10-26-settings-design.md`. Where this plan and the spec differ, the spec wins: stop and say so.
- Work only in the worktree `/Users/pedrogomes/src/jaw-settings`, on the branch `settings`. Never push, never pass `--no-verify`, never rewrite commits that aren't yours.
- The data layer (`crates/jawd/src/store/`, `sync*`, `feed/`, `find*`, `send*`, `engine/`, `sock.rs`, and from Task 4 `crates/jawd/src/settings*`) follows `.cursor/rules/data-integrity.mdc`. Read "Data integrity" and "Migrations" in `docs/plans/2026-10-16-jaw-design.md` before Task 3. Don't add `Data-Audit` trailers; Task 11 does.
- Migration 11 is new: a schema step, appended to `MIGRATIONS`, with its line in `migrations.lock`. A merged migration is never edited.
- No `OFFSET`, no scan of a table, no temporary b-tree; every new SQL statement's plan is pinned (`JAW_PLANS=write cargo test -p jawd plans`, then read the diff of `plans.txt`).
- Never stop processes by name, never copy the owner's `session.db`, never run the Go engine on a folder holding it; anything run by hand uses a temporary `JAW_HOME` and `--engine demo`.
- Commit messages are one long sentence in the project's voice (read `git log -5`), with no prefixes such as `feat:`.
- Code matches its surroundings: doc comments in plain sentences, British spelling (colour), no comments that narrate a change; `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Theme IDs, in order: `catppuccin`, `catppuccin-latte`, `ethereal`, `everforest`, `flexoki-light`, `gruvbox`, `hackerman`, `kanagawa`, `last-horizon`, `lumon`, `lupine`, `matte-black`, `miasma`, `nord`, `osaka-jade`, `retro-82`, `ristretto`, `rose-pine`, `solitude`, `starship`, `tokyo-night`, `vantablack`, `white`. The default is `tokyo-night`.
- Omarchy is `omacom/omarchy` at commit `4c1b4cb3a94447aff39af98e2fcc0490a9fb32e7`.
- `messages` takes `bubbles` (the default) or `irc`.
- Each frontend has a domain of settings of its own, named by the `client` its hello gives (owner, 26 October, after Tasks 1 and 2 had landed with one shared set; Task 2b moves them onto domains). The terminal's is `jaw`, `jaw_proto::settings::TERMINAL`, and it's the only domain jawd keeps so far; `jaw settings` and `jaw status` say hello as `jaw` too. A connection of a client with no domain gets no settings in hello's answer, no pushes, and error 3 from `settings.set`.
- Run a crate's tests with `cargo test -p <crate> <filter>`; the gate is `make test` and `make lint` (`make test` builds the Go engine; without `go`, run `cargo test --workspace` and say so).

## File structure

New:

- `scripts/omarchy-themes`: fetches `protocol/themes/*.toml` and `docs/licenses/omarchy.txt` at the pinned commit.
- `protocol/themes/<id>.toml`, 23 of them: Omarchy's `colors.toml`, as Omarchy wrote them.
- `docs/licenses/omarchy.txt`: Omarchy's MIT licence.
- `crates/jaw-proto/src/themes.rs`: the palettes, read once, and their types.
- `crates/jaw-proto/src/settings.rs`: the settings' types.
- `protocol/fixtures/settings.set.request.json`, `settings.set.result.json`, `settings.push.json`, `themes.request.json`, `themes.result.json`.
- `crates/jawd/src/settings.rs`: jawd's settings, loaded, checked, kept and published.
- `crates/jaw/src/view/settings.rs`: the panel.

Changed, task by task: `crates/jaw-proto/src/lib.rs`, `crates/jaw-proto/tests/fixtures.rs`, `protocol/fixtures/hello.result.json`, `protocol/fixtures/status.result.json`; `crates/jawd/src/store/{schema.rs, migrations.lock, write.rs, mod.rs, plans.txt}`, `crates/jawd/src/{sock.rs, state.rs, main.rs}`, `crates/jaw-client/src/lib.rs`, `scripts/check-data-audit.py`; `crates/jaw/src/{theme.rs, art.rs, layout.rs, data.rs, wire.rs, wire/tests.rs, demo.rs, view.rs, main.rs}`, `crates/jaw/src/view/{draw.rs, keys.rs, palette.rs, composer.rs, spike_tests.rs, tests.rs}`; `protocol/README.md`, `docs/plans/2026-10-16-jaw-design.md`, `docs/plans/2026-10-26-settings-design.md`, `README.md`.

---

### Task 1: Omarchy's themes in the protocol

**Files:**
- Create: `scripts/omarchy-themes`, `protocol/themes/*.toml` (written by the script), `docs/licenses/omarchy.txt` (written by the script), `crates/jaw-proto/src/themes.rs`
- Modify: `crates/jaw-proto/src/lib.rs`

**Interfaces:**
- Produces, in `jaw_proto::themes`:
  - `pub const DEFAULT: &str = "tokyo-night";`
  - `pub enum Mode { Dark, Light }`, serde snake_case
  - `pub struct Rgb(pub u32);` serialised as lowercase `#rrggbb`; `Rgb::parse(&str) -> Option<Rgb>`
  - `pub struct Colors { pub accent: Rgb, pub selection: Rgb, pub muted: Rgb, pub background: Rgb, pub dark_background: Rgb, pub darker_background: Rgb, pub lighter_background: Rgb, pub foreground: Rgb, pub dark_foreground: Rgb, pub light_foreground: Rgb, pub bright_foreground: Rgb, pub red: Rgb, pub yellow: Rgb, pub orange: Rgb, pub green: Rgb, pub cyan: Rgb, pub blue: Rgb, pub magenta: Rgb, pub brown: Rgb, pub bright_red: Rgb, pub bright_yellow: Rgb, pub bright_green: Rgb, pub bright_cyan: Rgb, pub bright_blue: Rgb, pub bright_magenta: Rgb }`
  - `pub struct Theme { pub id: String, pub name: String, pub mode: Mode, pub colors: Colors }`
  - `pub struct Themes { pub themes: Vec<Theme> }` (the `themes` method's result)
  - `pub fn all() -> &'static [Theme]`, `pub fn get(id: &str) -> Option<&'static Theme>`, `pub fn or_default(id: &str) -> &'static Theme`, `pub fn name(id: &str) -> String`, `pub fn parse(id: &str, text: &str) -> Result<Theme, String>`

- [ ] **Step 1: Write the script**

Create `scripts/omarchy-themes` and make it executable (`chmod +x scripts/omarchy-themes`):

```python
#!/usr/bin/env python3
"""Fetches Omarchy's themes into protocol/themes/, as Omarchy wrote them.

    scripts/omarchy-themes

Each theme's colors.toml at the commit pinned below goes to protocol/themes/<theme>.toml, and
Omarchy's licence to docs/licenses/omarchy.txt; Omarchy is MIT-licensed. To move to a newer
Omarchy, change OMARCHY_COMMIT, run this again and look at what changed: a theme added or gone
changes FILES in crates/jaw-proto/src/themes.rs and the tests that name the themes.
"""

import json
import os
import re
import sys
import urllib.request

OMARCHY = "omacom/omarchy"
OMARCHY_COMMIT = "4c1b4cb3a94447aff39af98e2fcc0490a9fb32e7"
REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
THEMES = os.path.join(REPO, "protocol", "themes")
LICENCE = os.path.join(REPO, "docs", "licenses", "omarchy.txt")


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "jaw-omarchy-themes"})
    with urllib.request.urlopen(request, timeout=30) as response:
        return response.read().decode()


def raw(path):
    return fetch(f"https://raw.githubusercontent.com/{OMARCHY}/{OMARCHY_COMMIT}/{path}")


def main():
    tree = json.loads(fetch(f"https://api.github.com/repos/{OMARCHY}/git/trees/{OMARCHY_COMMIT}?recursive=1"))
    found = (re.match(r"themes/([^/]+)/colors\.toml$", item["path"]) for item in tree["tree"])
    names = sorted({m.group(1) for m in found if m})
    if not names:
        sys.exit("omarchy-themes: no themes/<theme>/colors.toml at that commit")
    os.makedirs(THEMES, exist_ok=True)
    for stale in os.listdir(THEMES):
        if stale.endswith(".toml") and stale[: -len(".toml")] not in names:
            os.remove(os.path.join(THEMES, stale))
    for name in names:
        with open(os.path.join(THEMES, f"{name}.toml"), "w") as out:
            out.write(raw(f"themes/{name}/colors.toml"))
    os.makedirs(os.path.dirname(LICENCE), exist_ok=True)
    with open(LICENCE, "w") as out:
        out.write(raw("LICENSE"))
    print(f"{len(names)} themes from {OMARCHY} at {OMARCHY_COMMIT[:7]}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Run it**

Run: `scripts/omarchy-themes && ls protocol/themes | wc -l && head -3 docs/licenses/omarchy.txt`
Expected: `23 themes from omacom/omarchy at 4c1b4cb`, then `23`, then `Copyright (c) David Heinemeier Hansson`. GitHub is reachable from the sandbox; if it isn't, stop and report.

- [ ] **Step 3: Write the failing tests**

Create `crates/jaw-proto/src/themes.rs` with only this test module for now, and add `pub mod themes;` to `crates/jaw-proto/src/lib.rs` after `pub mod sync;`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_file_reads_with_its_mode_and_every_colour() {
        let ids: Vec<&str> = all().iter().map(|t| t.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "catppuccin", "catppuccin-latte", "ethereal", "everforest", "flexoki-light", "gruvbox", "hackerman",
                "kanagawa", "last-horizon", "lumon", "lupine", "matte-black", "miasma", "nord", "osaka-jade",
                "retro-82", "ristretto", "rose-pine", "solitude", "starship", "tokyo-night", "vantablack", "white",
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
        assert!(parse("x", &gruvbox.replace("accent = \"#7daea3\"", "accent = \"teal\"")).unwrap_err().contains("accent"));
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
```

- [ ] **Step 4: Run them to see them fail**

Run: `cargo test -p jaw-proto themes`
Expected: FAIL to compile: `cannot find function all`, `cannot find type Rgb`, and the like.

- [ ] **Step 5: Write the module**

Put this above the test module in `crates/jaw-proto/src/themes.rs`:

```rust
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
        FILES.iter().map(|(id, text)| parse(id, text).unwrap_or_else(|e| panic!("protocol/themes/{id}.toml: {e}"))).collect()
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
```

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p jaw-proto themes && cargo clippy -p jaw-proto --all-targets -- -D warnings`
Expected: 5 passed; clippy clean.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all
git add scripts/omarchy-themes protocol/themes docs/licenses/omarchy.txt crates/jaw-proto/src/themes.rs crates/jaw-proto/src/lib.rs
git commit -m "Omarchy's 23 themes in the protocol, as Omarchy wrote them: each theme's colors.toml at 4c1b4cb in protocol/themes/, with Omarchy's MIT licence in docs/licenses/omarchy.txt and scripts/omarchy-themes to fetch them again from a newer commit; jaw-proto reads them once into palettes of Omarchy's 25 colours and its mode, orange and brown taken from yellow where a theme leaves them out as Omarchy does, refusing a file that lacks a colour or has one that isn't #rrggbb, and names each from its ID, Rosé Pine with its accent"
```

---

### Task 2: The settings on the wire

**Files:**
- Create: `crates/jaw-proto/src/settings.rs`, `protocol/fixtures/settings.set.request.json`, `protocol/fixtures/settings.set.result.json`, `protocol/fixtures/settings.push.json`, `protocol/fixtures/themes.request.json`, `protocol/fixtures/themes.result.json`
- Modify: `crates/jaw-proto/src/lib.rs` (`Welcome`, `Status`, `pub mod settings`), `protocol/fixtures/hello.result.json`, `protocol/fixtures/status.result.json`, `crates/jaw-proto/tests/fixtures.rs`, `crates/jawd/src/state.rs`

**Interfaces:**
- Consumes: `jaw_proto::themes::DEFAULT` (Task 1).
- Produces, in `jaw_proto::settings`: `Messages { Bubbles, Irc }` with `Messages::ALL` and `name(self) -> &'static str`; `Settings { theme: String, messages: Messages }` with `Default` (tokyo-night, bubbles); `SettingsSet { theme: Option<String>, messages: Option<Messages> }` (`settings.set`'s params); `SettingsKept { settings: Settings }` (its result); `Ignored { key: String, value: serde_json::Value }`.
- Produces: `Welcome.settings: Settings`; `Status.settings: Option<Settings>`; `Status.settings_ignored: Vec<Ignored>`.

- [ ] **Step 1: Write the fixtures and their checks**

Change `protocol/fixtures/hello.result.json` to:

```json
{"id":1,"result":{"protocol":1,"jawd":"0.1.0","root":"/Users/me/Library/Application Support/jaw","seq":81233,"account":{"state":"linked","me":{"id":"person:1","name":"Pedro","phone":"+351 912 000 000"}},"connection":{"state":"online"},"settings":{"theme":"tokyo-night","messages":"bubbles"}}}
```

Change `protocol/fixtures/status.result.json` to:

```json
{"id":2,"result":{"jawd":"0.1.0","protocol":1,"root":"/Users/me/Library/Application Support/jaw","account":{"state":"unlinked"},"connection":{"state":"offline"},"engine":{"kind":"go","state":"running","version":"jaw-engine 0.1.0 (whatsmeow v0.0.0-20261015105041-df33a485cc46)"},"missing":[{"what":"ffmpeg","needed_for":"sending videos, GIFs and audio files"}],"settings":{"theme":"tokyo-night","messages":"irc"},"settings_ignored":[{"key":"theme","value":"rose-pine-moon"}]}}
```

Create `protocol/fixtures/settings.set.request.json`:

```json
{"id":20,"method":"settings.set","params":{"theme":"gruvbox"}}
```

Create `protocol/fixtures/settings.set.result.json`:

```json
{"id":20,"result":{"settings":{"theme":"gruvbox","messages":"bubbles"}}}
```

Create `protocol/fixtures/settings.push.json`:

```json
{"method":"settings","params":{"theme":"gruvbox","messages":"irc"}}
```

Create `protocol/fixtures/themes.request.json`:

```json
{"id":21,"method":"themes"}
```

Create `protocol/fixtures/themes.result.json` (two themes, to pin the shape):

```json
{"id":21,"result":{"themes":[{"id":"catppuccin","name":"Catppuccin","mode":"dark","colors":{"accent":"#89b4fa","selection":"#45475a","muted":"#585b70","background":"#1e1e2e","dark_background":"#161622","darker_background":"#101019","lighter_background":"#313244","foreground":"#cdd6f4","dark_foreground":"#6c7086","light_foreground":"#bac2de","bright_foreground":"#cdd6f4","red":"#f38ba8","yellow":"#f9e2af","orange":"#f6b6ab","green":"#a6e3a1","cyan":"#94e2d5","blue":"#89b4fa","magenta":"#f5c2e7","brown":"#7b5b55","bright_red":"#f38ba8","bright_yellow":"#f9e2af","bright_green":"#a6e3a1","bright_cyan":"#94e2d5","bright_blue":"#89b4fa","bright_magenta":"#f5c2e7"}},{"id":"catppuccin-latte","name":"Catppuccin Latte","mode":"light","colors":{"accent":"#1e66f5","selection":"#ccd0da","muted":"#acb0be","background":"#eff1f5","dark_background":"#e3e4e8","darker_background":"#d7d8dc","lighter_background":"#dce0e8","foreground":"#4c4f69","dark_foreground":"#9ca0b0","light_foreground":"#5c5f77","bright_foreground":"#4c4f69","red":"#d20f39","yellow":"#df8e1d","orange":"#d84e2b","green":"#40a02b","cyan":"#179299","blue":"#1e66f5","magenta":"#ea76cb","brown":"#6c2715","bright_red":"#d20f39","bright_yellow":"#df8e1d","bright_green":"#40a02b","bright_cyan":"#179299","bright_blue":"#1e66f5","bright_magenta":"#ea76cb"}}]}}
```

In `crates/jaw-proto/tests/fixtures.rs`, add to the imports:

```rust
use jaw_proto::settings::{Settings, SettingsKept, SettingsSet};
use jaw_proto::themes::Themes;
```

In the `"request"` arm, replace `"status" => assert!(request.params.is_null(), "{name} has params"),` with:

```rust
                    "status" | "themes" => assert!(request.params.is_null(), "{name} has params"),
                    "settings.set" => params::<SettingsSet>(&name, &request.params),
```

In the `"result"` arm, add before `_ => panic!("no type for {name}'s result"),`:

```rust
                    "settings.set" => params::<SettingsKept>(&name, result),
                    "themes" => params::<Themes>(&name, result),
```

In the `"push"` arm, add before its `_ =>`:

```rust
                    "settings" => params::<Settings>(&name, &push.params),
```

And add this test at the end of the file:

```rust
/// The themes fixture is the first two themes exactly as jaw-proto reads them from protocol/themes.
#[test]
fn the_themes_fixture_is_what_the_theme_files_say() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../protocol/fixtures/themes.result.json");
    let value: Value = serde_json::from_str(&std::fs::read_to_string(folder).unwrap()).unwrap();
    let themes: Themes = serde_json::from_value(value["result"].clone()).unwrap();
    assert_eq!(themes.themes, jaw_proto::themes::all()[..2]);
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p jaw-proto --test fixtures`
Expected: FAIL to compile: `unresolved import jaw_proto::settings`.

- [ ] **Step 3: Write the types**

Create `crates/jaw-proto/src/settings.rs`:

```rust
//! The settings every interface shares, which jawd keeps in jaw.db: the theme, and where your
//! messages sit. protocol/README.md's "Settings" says how they're read, changed and pushed.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where your own messages sit in a conversation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Messages {
    /// In a column from a third of the way across.
    #[default]
    Bubbles,
    /// On the left under "You", as everyone's are, as IRC clients put them.
    Irc,
}

impl Messages {
    pub const ALL: [Messages; 2] = [Messages::Bubbles, Messages::Irc];

    /// Its name on the wire.
    pub fn name(self) -> &'static str {
        match self {
            Messages::Bubbles => "bubbles",
            Messages::Irc => "irc",
        }
    }
}

/// Every setting with the value in effect, as hello's answer, `settings.set`'s answer and the
/// `settings` push carry them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// An Omarchy theme's ID, as `themes` lists them.
    pub theme: String,
    pub messages: Messages,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { theme: crate::themes::DEFAULT.to_string(), messages: Messages::default() }
    }
}

/// `settings.set`'s params: the settings to change, at least one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsSet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messages: Option<Messages>,
}

/// `settings.set`'s result: every setting as it now is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsKept {
    pub settings: Settings,
}

/// A setting whose value in jaw.db this jawd doesn't take, so its default is in effect.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ignored {
    pub key: String,
    pub value: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_tokyo_night_with_yours_on_the_right_and_a_change_names_only_what_it_changes() {
        assert_eq!(Settings::default(), Settings { theme: "tokyo-night".into(), messages: Messages::Bubbles });
        assert_eq!(Messages::ALL.map(Messages::name), ["bubbles", "irc"]);
        let change = SettingsSet { theme: Some("gruvbox".into()), messages: None };
        assert_eq!(serde_json::to_value(change).unwrap(), serde_json::json!({"theme": "gruvbox"}));
    }
}
```

In `crates/jaw-proto/src/lib.rs`, add `pub mod settings;` after `pub mod send;`. Add to `Welcome`, after `connection`:

```rust
    /// Every setting with the value in effect.
    #[serde(default)]
    pub settings: settings::Settings,
```

Add to `Status`, after `storage`:

```rust
    /// The settings in effect, and each whose value in jaw.db this jawd doesn't take.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<settings::Settings>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings_ignored: Vec<settings::Ignored>,
```

In `crates/jawd/src/state.rs`, so jawd still builds until Task 4 fills them in: in `welcome()` add `settings: Default::default(),` after `connection: self.connection(),`; in `status()` add `settings: None,` and `settings_ignored: Vec::new(),` after `storage: …`.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p jaw-proto && cargo build -p jawd -p jaw -p jaw-client`
Expected: every jaw-proto test passes, the fixtures test included; the three crates build.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/jaw-proto protocol/fixtures crates/jawd/src/state.rs
git commit -m "The settings on the wire: hello's answer carries every setting with the value in effect, the theme and where your messages sit (bubbles or irc), settings.set takes any of them and answers with all, a settings push carries all of them, themes answers with every theme's colours, and status says the settings in effect and any jawd doesn't take, each pinned by a fixture, the themes fixture being the theme files' first two as jaw-proto reads them"
```

---

### Task 2b: A domain for each frontend, on the wire

Task 2 put one set of settings, shared by every interface, on the wire. The owner has since chosen a domain of settings for each frontend, named by the `client` its hello gives, so a terminal and a Mac app on one machine keep separate preferences. This task moves Task 2's types onto domains; the shapes of `settings.set`, its answer and the `settings` push don't change, since each is the connection's own domain's.

**Files:**
- Modify: `crates/jaw-proto/src/settings.rs`, `crates/jaw-proto/src/lib.rs` (`Welcome`, `Status`), `crates/jaw-proto/tests/fixtures.rs`, `protocol/fixtures/status.result.json`, `crates/jawd/src/state.rs`

**Interfaces:**
- Consumes: Task 2's `jaw_proto::settings::{Settings, Ignored}`, `Welcome.settings`, `Status.settings`.
- Produces, in `jaw_proto::settings`: `pub const TERMINAL: &str = "jaw"`; `Ignored { domain: String, key: String, value: serde_json::Value }`.
- Produces: `Welcome.settings: Option<Settings>`, the connection's own domain's settings, absent for a client jawd keeps none for; `Status.settings: BTreeMap<String, serde_json::Map<String, Value>>`, each domain's settings in effect by its name.

- [ ] **Step 1: Write the fixture and the failing checks**

Change `protocol/fixtures/status.result.json` so its last two fields read:

```json
"settings":{"jaw":{"messages":"irc","theme":"tokyo-night"}},"settings_ignored":[{"domain":"jaw","key":"theme","value":"rose-pine-moon"}]}}
```

(the rest of the line as it is). Add to the test module of `crates/jaw-proto/src/settings.rs`:

```rust
    #[test]
    fn the_terminals_domain_is_the_client_its_hello_gives_and_a_hello_for_another_carries_no_settings() {
        assert_eq!(TERMINAL, "jaw");
        let hello: crate::Welcome = serde_json::from_str(
            r#"{"protocol":1,"jawd":"0.1.0","root":"/r","seq":1,"account":{"state":"unlinked"},"connection":{"state":"offline"}}"#,
        )
        .unwrap();
        assert_eq!(hello.settings, None);
        let ignored = Ignored { domain: TERMINAL.into(), key: "theme".into(), value: serde_json::json!("x") };
        assert_eq!(serde_json::to_value(ignored).unwrap()["domain"], "jaw");
    }
```

And add to the end of `crates/jaw-proto/tests/fixtures.rs`:

```rust
/// Hello's answer carries the terminal's settings, and status every domain's by its name.
#[test]
fn hello_carries_its_domains_settings_and_status_every_domains() {
    let read = |name: &str| -> Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../protocol/fixtures/{name}.json"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    };
    let hello: jaw_proto::Welcome = serde_json::from_value(read("hello.result")["result"].clone()).unwrap();
    assert_eq!(hello.settings, Some(Settings::default()));
    let status: jaw_proto::Status = serde_json::from_value(read("status.result")["result"].clone()).unwrap();
    assert_eq!(status.settings["jaw"]["messages"], "irc");
    assert_eq!(status.settings_ignored[0].domain, "jaw");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p jaw-proto`
Expected: FAIL to compile: no `TERMINAL`, `Ignored` has no field `domain`, and `Welcome.settings` isn't an `Option`.

- [ ] **Step 3: Move the types onto domains**

In `crates/jaw-proto/src/settings.rs`, replace the module's doc with:

```rust
//! Each frontend's settings, which jawd keeps in jaw.db in a domain of its own, named by the
//! `client` its hello gives. The terminal's, in the `jaw` domain, are the theme and where your
//! messages sit. protocol/README.md's "Settings" says how they're read, changed and pushed.
```

add, after the `use` lines:

```rust
/// The terminal's domain: the `client` its hello gives, which `jaw settings` and `jaw status` give
/// too.
pub const TERMINAL: &str = "jaw";
```

change `Settings`' doc to `/// The terminal's settings, each with the value in effect, as hello's answer, settings.set's answer and the settings push carry them to a connection of the jaw domain.`, and replace `Ignored` with:

```rust
/// A setting whose value in jaw.db this jawd doesn't take, so its default is in effect: its domain,
/// its key and that value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ignored {
    pub domain: String,
    pub key: String,
    pub value: Value,
}
```

In `crates/jaw-proto/src/lib.rs`, add `use std::collections::BTreeMap;` to the imports, replace `Welcome.settings` and its doc with:

```rust
    /// The settings of the connection's own domain, the one its `client` names, each with the
    /// value in effect; absent for a client jawd keeps no settings for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<settings::Settings>,
```

and `Status.settings` and its doc with:

```rust
    /// Each domain's settings in effect, by the domain's name, and each setting whose value in
    /// jaw.db this jawd doesn't take.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, serde_json::Map<String, Value>>,
```

leaving `settings_ignored` as it is. In `crates/jawd/src/state.rs`, so jawd still builds until Task 4: in `welcome()` make it `settings: None,` and in `status()` `settings: std::collections::BTreeMap::new(),`.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p jaw-proto && cargo build -p jawd -p jaw -p jaw-client`
Expected: every jaw-proto test passes, the fixtures' included; the three crates build.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/jaw-proto protocol/fixtures crates/jawd/src/state.rs
git commit -m "A domain of settings for each frontend, on the wire, as the owner chose so a terminal and a Mac app on one machine keep separate preferences: a frontend's domain is the client its hello gives, the terminal's jaw; hello's answer carries the connection's own domain's settings and none for a client jawd keeps none for; status says every domain's settings in effect by its name, and each setting jawd doesn't take with its domain; settings.set, its answer and the settings push keep their shapes, each being the connection's own domain's"
```

---

### Task 3: jaw.db keeps the settings (data layer)

**Files:**
- Modify: `crates/jawd/src/store/schema.rs` (migration 11), `crates/jawd/src/store/migrations.lock`, `crates/jawd/src/store/write.rs` (`Change::Settings`), `crates/jawd/src/store/mod.rs` (`Store::setting`, a test), `crates/jawd/src/store/plans.txt` (re-pinned)

**Interfaces:**
- Produces: `Change::Settings { domain: String, values: Vec<(String, String)>, at: i64 }`, a frontend's settings changed together, each as its key and its JSON text, kept all or none in one step, and `at` milliseconds since the epoch; `Store::setting(&self, domain: &str, key: &str) -> anyhow::Result<Option<String>>`, what jaw.db holds for `key` in `domain` as its JSON.

- [ ] **Step 1: Write the failing test**

In `crates/jawd/src/store/mod.rs`'s test module (it has `query(&store, |db| …)` and `use super::*`), add:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn a_setting_is_kept_once_with_a_synced_commit_and_read_back_by_its_domain_and_key() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        assert_eq!(store.setting("jaw", "theme").unwrap(), None, "a fresh jaw.db holds no settings");
        let keep = |domain: &str, value: &str, at| Change::Settings {
            domain: domain.into(),
            values: vec![("theme".into(), value.into())],
            at,
        };
        let synced = store.storage().synced_commits;
        store.apply_durable(vec![keep("jaw", "\"gruvbox\"", 1)]).await.unwrap();
        assert_eq!(store.storage().synced_commits, synced + 1);
        assert_eq!(store.setting("jaw", "theme").unwrap().as_deref(), Some("\"gruvbox\""));
        assert_eq!(store.setting("jaw-mac", "theme").unwrap(), None, "each domain is its own");
        store.apply_durable(vec![keep("jaw", "\"gruvbox\"", 2)]).await.unwrap();
        let at: i64 = query(&store, |db| {
            Ok(db.query_row("SELECT at FROM settings WHERE domain = 'jaw' AND key = 'theme'", [], |r| r.get(0))?)
        })
        .await;
        assert_eq!(at, 1, "the same value again changes nothing, not even when it was chosen");
        store.apply_durable(vec![keep("jaw-mac", "\"nord\"", 3)]).await.unwrap();
        assert_eq!(store.setting("jaw-mac", "theme").unwrap().as_deref(), Some("\"nord\""));
        assert_eq!(store.setting("jaw", "theme").unwrap().as_deref(), Some("\"gruvbox\""), "another domain's leaves it");
        let refused = store.apply_durable(vec![keep("jaw", "not json", 4)]).await.unwrap();
        assert_eq!(refused.quarantined, 1, "a value that isn't JSON is refused by the table's check");
        assert_eq!(store.setting("jaw", "theme").unwrap().as_deref(), Some("\"gruvbox\""));
        let both = |messages: &str, at| Change::Settings {
            domain: "jaw".into(),
            values: vec![("theme".into(), "\"nord\"".into()), ("messages".into(), messages.into())],
            at,
        };
        let refused = store.apply_durable(vec![both("not json", 5)]).await.unwrap();
        assert_eq!(refused.quarantined, 1);
        assert_eq!(store.setting("jaw", "theme").unwrap().as_deref(), Some("\"gruvbox\""), "kept all or none");
        assert_eq!(store.setting("jaw", "messages").unwrap(), None);
        store.apply_durable(vec![both("\"irc\"", 6)]).await.unwrap();
        assert_eq!(store.setting("jaw", "theme").unwrap().as_deref(), Some("\"nord\""));
        assert_eq!(store.setting("jaw", "messages").unwrap().as_deref(), Some("\"irc\""));
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p jawd a_setting_is_kept_once`
Expected: FAIL to compile: no variant `Change::Settings`, no method `setting`.

- [ ] **Step 3: Add migration 11**

Append to `MIGRATIONS` in `crates/jawd/src/store/schema.rs`, after migration 10's string:

```rust
    // 11, a schema step: each frontend's settings, one new table, which costs the same however big
    // jaw.db is.
    "
    -- Class: jaw's own. Each frontend's settings, in a domain named by the client its hello gives, a
    -- row for each setting chosen: never dropped or rebuilt, and copied as they are by any rebuild. A
    -- setting without a row has its default. `value` is the setting's JSON, and `at` when it was
    -- chosen, in milliseconds.
    CREATE TABLE settings (
        domain TEXT NOT NULL,
        key TEXT NOT NULL,
        value TEXT NOT NULL CHECK (json_valid(value)),
        at INTEGER NOT NULL,
        PRIMARY KEY (domain, key)
    ) STRICT, WITHOUT ROWID;
    ",
```

Run: `cargo test -p jawd each_merged_migration_is_still_the_text_it_was_merged_with`
Expected: FAIL with `migration 11 has no line in migrations.lock: add \`11 schema <hash>\``. Append that exact line to `crates/jawd/src/store/migrations.lock`, under line 10.

- [ ] **Step 4: Add the change and the read**

In `crates/jawd/src/store/write.rs`, add to `enum Change`, after `Remember`:

```rust
    /// A frontend's settings changed together, in its domain, each as its key and its JSON, kept
    /// as jaw's own in one step, so they're kept all or none; `at` is when they were chosen, which
    /// a value already held doesn't change.
    Settings {
        domain: String,
        values: Vec<(String, String)>,
        at: i64,
    },
```

In `provenance`, after the `Change::Remember` arm:

```rust
        Some(Change::Settings { domain, .. }) => ("settings", domain.clone(), None),
```

In `apply`, after the `Change::Remember` arm:

```rust
        Change::Settings { domain, values, at } => {
            let mut keep = db.prepare_cached(
                "INSERT INTO settings (domain, key, value, at) VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT (domain, key) DO UPDATE SET value = excluded.value, at = excluded.at
                WHERE value IS NOT excluded.value",
            )?;
            for (key, value) in values {
                keep.execute(params![domain, key, value, at])?;
            }
            Ok(false)
        }
```

Each change is applied in a savepoint of its own, so one that fails partway leaves none of its settings kept.

In `crates/jawd/src/store/mod.rs`, after `remembered`:

```rust
    /// What jaw.db holds for the setting `key` of `domain`, as its JSON, read on a connection of its
    /// own.
    pub fn setting(&self, domain: &str, key: &str) -> Result<Option<String>> {
        let db = connect(&self.path, Role::Reader)?;
        Ok(db
            .query_row("SELECT value FROM settings WHERE domain = ?1 AND key = ?2", [domain, key], |r| r.get(0))
            .optional()?)
    }
```

- [ ] **Step 5: Pin the plans**

Run: `JAW_PLANS=write cargo test -p jawd plans && git diff crates/jawd/src/store/plans.txt`
Expected: the diff adds `SELECT value FROM settings WHERE domain = ?1 AND key = ?2` planned as `SEARCH settings USING PRIMARY KEY (domain=? AND key=?)`, and the `INSERT INTO settings …` statement; nothing else changes. Then `cargo test -p jawd plans` passes.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p jawd a_setting_is_kept_once && cargo test -p jawd schema && cargo test -p jawd store`
Expected: all pass, the migrations' tests included (upgrades from every earlier version, a fault at every statement).

- [ ] **Step 7: Commit**

```bash
cargo fmt --all
git add crates/jawd/src/store
git commit -m "jaw.db keeps each frontend's settings: migration 11, a schema step, adds settings, a STRICT table of jaw's own with a row for each setting chosen in each domain, the domain being the client a frontend's hello gives, its value JSON by its check and when it was chosen; the writer keeps those a change carries together, all or none in one step, each an upsert that changes nothing, not even when it was chosen, for the value already held, a value that isn't JSON quarantining the whole change, and jawd reads one back by its domain and key, a search of the primary key, both statements' plans pinned; nothing outside the tests uses the change or the read until jawd's settings module does, so clippy calls them dead code until then"
```

---

### Task 4: jawd serves the settings (data layer)

**Files:**
- Create: `crates/jawd/src/settings.rs`
- Modify: `crates/jawd/src/main.rs`, `crates/jawd/src/state.rs`, `crates/jawd/src/sock.rs`, `crates/jaw-client/src/lib.rs`, `scripts/check-data-audit.py`

**Interfaces:**
- Consumes: `Change::Settings` and `Store::setting` (Task 3), which nothing outside the tests used before this task, so Task 3's commit leaves clippy's two dead-code errors for this one to clear; `jaw_proto::settings::*` and `jaw_proto::themes::*` (Tasks 1, 2 and 2b).
- Produces: `crate::settings::Settings` with `load(store: Store) -> anyhow::Result<Settings>`, `current(&self) -> jaw_proto::settings::Settings` (the terminal's), `subscribe(&self, client: &str) -> Option<watch::Receiver<jaw_proto::settings::Settings>>` (none for a client with no domain), `by_domain(&self) -> BTreeMap<String, serde_json::Map<String, Value>>`, `ignored(&self) -> Vec<Ignored>`, `async set(&self, client: &str, params: Value) -> Result<jaw_proto::settings::Settings, jaw_proto::Error>`; `State::with_settings(Arc<Settings>)` and `State::settings() -> Option<&Settings>`.

- [ ] **Step 1: Write the module's failing tests**

Create `crates/jawd/src/settings.rs` holding only its test module for now, and add `mod settings;` to `crates/jawd/src/main.rs` after `mod send;`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use jaw_proto::settings::{Messages, TERMINAL};
    use serde_json::json;

    fn opened() -> (tempfile::TempDir, Store, Settings) {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let settings = Settings::load(store.clone()).unwrap();
        (home, store, settings)
    }

    async fn refused(settings: &Settings, params: Value) -> Error {
        settings.set(TERMINAL, params).await.unwrap_err()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_change_is_checked_kept_with_a_synced_commit_and_then_published() {
        let (_home, store, settings) = opened();
        assert_eq!(settings.current(), Shared::default());
        let mut heard = settings.subscribe(TERMINAL).unwrap();
        let synced = store.storage().synced_commits;
        let now = settings.set(TERMINAL, json!({"theme": "gruvbox"})).await.unwrap();
        assert_eq!(now, Shared { theme: "gruvbox".into(), messages: Messages::Bubbles });
        assert_eq!(store.storage().synced_commits, synced + 1);
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().as_deref(), Some("\"gruvbox\""));
        assert_eq!(store.setting(TERMINAL, "messages").unwrap(), None, "only what was asked is kept");
        assert!(heard.has_changed().unwrap());
        assert_eq!(*heard.borrow_and_update(), now);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_client_without_a_domain_has_no_settings_and_cant_change_any() {
        let (_home, store, settings) = opened();
        assert!(settings.subscribe("test").is_none());
        let e = settings.set("test", json!({"theme": "gruvbox"})).await.unwrap_err();
        assert_eq!((e.code, e.data), (code::NOT_FOUND, Some(json!({"client": "test"}))));
        assert_eq!(store.setting(TERMINAL, "theme").unwrap(), None, "nothing kept");
        let in_effect = settings.by_domain();
        assert_eq!(in_effect.keys().collect::<Vec<_>>(), [TERMINAL], "the terminal's is the only domain");
        assert_eq!(in_effect[TERMINAL]["theme"], "tokyo-night");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn every_setting_asked_is_checked_before_any_is_kept() {
        let (_home, store, settings) = opened();
        let e = refused(&settings, json!({"colour": "teal"})).await;
        assert_eq!((e.code, e.data), (code::INVALID_PARAMS, Some(json!({"param": "colour"}))));
        let e = refused(&settings, json!({"theme": "solarized"})).await;
        assert_eq!(e.code, code::INVALID_PARAMS);
        let data = e.data.unwrap();
        assert_eq!(data["param"], "theme");
        assert_eq!(data["allowed"].as_array().unwrap().len(), 23);
        let e = refused(&settings, json!({"messages": 2})).await;
        assert_eq!(e.data.unwrap()["allowed"], json!(["bubbles", "irc"]));
        for nothing in [json!({}), Value::Null, json!(["theme"])] {
            assert_eq!(refused(&settings, nothing).await.code, code::INVALID_PARAMS);
        }
        let e = refused(&settings, json!({"theme": "gruvbox", "messages": "sideways"})).await;
        assert_eq!(e.data.unwrap()["param"], "messages");
        assert_eq!(store.setting(TERMINAL, "theme").unwrap(), None, "a refused ask keeps none of its settings");
        assert_eq!(settings.current(), Shared::default());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_value_jaw_db_holds_already_is_answered_at_once_and_writes_nothing() {
        let (_home, store, settings) = opened();
        settings.set(TERMINAL, json!({"messages": "irc"})).await.unwrap();
        let synced = store.storage().synced_commits;
        let heard = settings.subscribe(TERMINAL).unwrap();
        let now = settings.set(TERMINAL, json!({"messages": "irc"})).await.unwrap();
        assert_eq!(now.messages, Messages::Irc);
        assert_eq!(store.storage().synced_commits, synced, "nothing written");
        assert!(!heard.has_changed().unwrap(), "and nothing published");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_restart_keeps_them_and_a_value_jawd_doesnt_take_is_kept_and_read_as_the_default() {
        let home = tempfile::tempdir().unwrap();
        {
            let settings = Settings::load(Store::open(home.path()).unwrap()).unwrap();
            settings.set(TERMINAL, json!({"theme": "nord", "messages": "irc"})).await.unwrap();
        }
        {
            let settings = Settings::load(Store::open(home.path()).unwrap()).unwrap();
            assert_eq!(settings.current(), Shared { theme: "nord".into(), messages: Messages::Irc });
            assert!(settings.ignored().is_empty());
        }
        let outside = rusqlite::Connection::open(home.path().join("jaw.db")).unwrap();
        outside
            .execute("UPDATE settings SET value = '\"rose-pine-moon\"' WHERE domain = 'jaw' AND key = 'theme'", [])
            .unwrap();
        outside
            .execute(
                "INSERT INTO settings (domain, key, value, at)
                VALUES ('jaw', 'font', '\"Berkeley Mono\"', 1), ('jaw-mac', 'theme', '\"lumon\"', 1)",
                [],
            )
            .unwrap();
        drop(outside);
        let store = Store::open(home.path()).unwrap();
        let settings = Settings::load(store.clone()).unwrap();
        assert_eq!(settings.current(), Shared { theme: themes::DEFAULT.into(), messages: Messages::Irc });
        let ignored = Ignored { domain: TERMINAL.into(), key: "theme".into(), value: json!("rose-pine-moon") };
        assert_eq!(settings.ignored(), vec![ignored]);
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().as_deref(), Some("\"rose-pine-moon\""), "kept as it was");
        assert_eq!(store.count("settings").unwrap(), 4, "a setting or a domain jawd doesn't know is left alone");
        assert_eq!(settings.by_domain().keys().collect::<Vec<_>>(), [TERMINAL], "and not read");
        settings.set(TERMINAL, json!({"theme": "tokyo-night"})).await.unwrap();
        assert!(settings.ignored().is_empty(), "choosing the default replaces what wasn't understood");
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().as_deref(), Some("\"tokyo-night\""));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_change_that_cant_be_kept_publishes_nothing() {
        let (home, store, settings) = opened();
        let outside = rusqlite::Connection::open(home.path().join("jaw.db")).unwrap();
        outside
            .execute_batch("CREATE TRIGGER refused BEFORE INSERT ON settings BEGIN SELECT RAISE(ABORT, 'refused'); END")
            .unwrap();
        let heard = settings.subscribe(TERMINAL).unwrap();
        assert_eq!(refused(&settings, json!({"theme": "nord"})).await.code, code::INTERNAL);
        assert_eq!(settings.current(), Shared::default());
        assert!(!heard.has_changed().unwrap(), "nothing published");
        assert_eq!(store.setting(TERMINAL, "theme").unwrap(), None);
        outside.execute_batch("DROP TRIGGER refused").unwrap();
        assert_eq!(settings.set(TERMINAL, json!({"theme": "nord"})).await.unwrap().theme, "nord", "and it can be asked again");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn changes_from_many_interfaces_at_once_end_with_jaw_db_and_what_was_published_agreeing() {
        let (_home, store, settings) = opened();
        let settings = std::sync::Arc::new(settings);
        let mut heard = settings.subscribe(TERMINAL).unwrap();
        let asks: Vec<_> = (0..24)
            .map(|i| {
                let settings = settings.clone();
                let theme = themes::all()[i % 23].id.clone();
                let messages = if i % 2 == 0 { "irc" } else { "bubbles" };
                tokio::spawn(async move { settings.set(TERMINAL, json!({"theme": theme, "messages": messages})).await.unwrap() })
            })
            .collect();
        for ask in asks {
            ask.await.unwrap();
        }
        let now = settings.current();
        assert_eq!(*heard.borrow_and_update(), now, "the last published is the one in effect");
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().unwrap(), serde_json::to_string(&now.theme).unwrap());
        assert_eq!(store.setting(TERMINAL, "messages").unwrap().unwrap(), serde_json::to_string(&now.messages).unwrap());
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p jawd settings::tests`
Expected: FAIL to compile: `cannot find type Settings`, `Shared`, and the like.

The domain is a connection's `client`, which `sock.rs` passes to `set` and `subscribe`; jawd keeps only the terminal's domain so far, so the module holds one domain's settings, and another is another entry here when it comes.

- [ ] **Step 3: Write the module**

Put this above the test module in `crates/jawd/src/settings.rs`:

```rust
//! Each frontend's settings, kept in jaw.db as jaw's own in a domain named by the client its hello
//! gives. The terminal's, in the `jaw` domain, are the only ones so far: the theme and where your
//! messages sit. They're read once at start and changed one at a time: checked, kept with a synced
//! commit, then published on the domain's watch channel, which always holds the latest, for each
//! of its connections to push.

use std::collections::BTreeMap;

use jaw_proto::settings::{Ignored, Messages, Settings as Shared, TERMINAL};
use jaw_proto::{Error, code, themes};
use serde_json::{Value, json};
use tokio::sync::{Mutex, watch};

use crate::store::{Change, Store};

/// The terminal's settings, by the names they're asked and kept by.
const KNOWN: [&str; 2] = ["theme", "messages"];

pub struct Settings {
    store: Store,
    /// What jaw.db holds for each of the terminal's settings chosen, as its JSON. A change holds it
    /// from its check to its publishing, so changes are made one at a time, in the order their
    /// commits are.
    held: Mutex<BTreeMap<&'static str, String>>,
    published: watch::Sender<Shared>,
    ignored: std::sync::Mutex<Vec<Ignored>>,
}

impl Settings {
    /// Reads what jaw.db holds for each setting of each domain jawd knows: the terminal's.
    pub fn load(store: Store) -> anyhow::Result<Settings> {
        let mut held = BTreeMap::new();
        for key in KNOWN {
            if let Some(value) = store.setting(TERMINAL, key)? {
                held.insert(key, value);
            }
        }
        let (shared, ignored) = in_effect(&held);
        Ok(Settings {
            store,
            held: Mutex::new(held),
            published: watch::channel(shared).0,
            ignored: std::sync::Mutex::new(ignored),
        })
    }

    /// The terminal's settings in effect.
    pub fn current(&self) -> Shared {
        self.published.borrow().clone()
    }

    /// A receiver for the settings of `client`'s domain, which has seen those in effect now and
    /// wakes for each later change; none for a client jawd keeps no settings for.
    pub fn subscribe(&self, client: &str) -> Option<watch::Receiver<Shared>> {
        (client == TERMINAL).then(|| self.published.subscribe())
    }

    /// Each domain's settings in effect, by its name, as `status` says them.
    pub fn by_domain(&self) -> BTreeMap<String, serde_json::Map<String, Value>> {
        match serde_json::to_value(self.current()) {
            Ok(Value::Object(terminal)) => BTreeMap::from([(TERMINAL.to_string(), terminal)]),
            _ => BTreeMap::new(),
        }
    }

    /// Each setting whose value in jaw.db this jawd doesn't take.
    pub fn ignored(&self) -> Vec<Ignored> {
        self.ignored.lock().expect("the ignored settings").clone()
    }

    /// `settings.set` from a connection of `client`: checks that its domain is one jawd keeps and
    /// every setting asked before changing any, keeps those that differ from what jaw.db holds in
    /// one synced commit, and only then publishes the domain's settings in effect and answers with
    /// them. What jaw.db holds already is answered at once, and written again never.
    pub async fn set(&self, client: &str, params: Value) -> Result<Shared, Error> {
        if client != TERMINAL {
            let mut error = Error::new(code::NOT_FOUND, format!("jawd keeps no settings for {client}"));
            error.data = Some(json!({ "client": client }));
            return Err(error);
        }
        let asked = checked(params)?;
        let mut held = self.held.lock().await;
        let at = now_ms();
        let values: Vec<(String, String)> = asked
            .iter()
            .filter(|(key, value)| held.get(key) != Some(value))
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect();
        if values.is_empty() {
            return Ok(self.current());
        }
        let change = Change::Settings { domain: TERMINAL.to_string(), values, at };
        match self.store.apply_durable(vec![change]).await {
            Ok(applied) if applied.quarantined == 0 => {}
            Ok(_) => return Err(Error::new(code::INTERNAL, "jawd couldn't keep that setting, so nothing changed")),
            Err(e) => return Err(Error::new(code::INTERNAL, format!("jawd couldn't keep that setting: {e:#}"))),
        }
        held.extend(asked);
        let (shared, ignored) = in_effect(&held);
        *self.ignored.lock().expect("the ignored settings") = ignored;
        self.published.send_replace(shared.clone());
        Ok(shared)
    }
}

/// `settings.set`'s params checked: at least one setting, each one jawd knows with a value it
/// takes, as the JSON jaw.db keeps.
fn checked(params: Value) -> Result<Vec<(&'static str, String)>, Error> {
    let asked = match params {
        Value::Object(asked) if !asked.is_empty() => asked,
        _ => {
            return Err(Error::new(code::INVALID_PARAMS, "settings.set takes one setting or more, such as {\"theme\": \"gruvbox\"}"));
        }
    };
    let mut out = Vec::new();
    for (key, value) in asked {
        let Some(known) = KNOWN.into_iter().find(|k| *k == key) else {
            let mut error = Error::new(code::INVALID_PARAMS, format!("jawd doesn't know the setting {key}"));
            error.data = Some(json!({ "param": key }));
            return Err(error);
        };
        let allowed: Vec<&str> = match known {
            "theme" => themes::all().iter().map(|t| t.id.as_str()).collect(),
            _ => Messages::ALL.iter().map(|m| m.name()).collect(),
        };
        match value.as_str().filter(|v| allowed.contains(v)) {
            Some(v) => out.push((known, Value::from(v).to_string())),
            None => {
                let mut error = Error::new(code::INVALID_PARAMS, format!("{key} takes one of {}", allowed.join(", ")));
                error.data = Some(json!({ "param": key, "allowed": allowed }));
                return Err(error);
            }
        }
    }
    Ok(out)
}

/// The settings in effect from what jaw.db holds, and each value held that this jawd doesn't take,
/// whose setting has its default instead.
fn in_effect(held: &BTreeMap<&'static str, String>) -> (Shared, Vec<Ignored>) {
    let mut shared = Shared::default();
    let mut ignored = Vec::new();
    for (key, json) in held {
        let value: Value = serde_json::from_str(json).unwrap_or(Value::Null);
        let taken = match *key {
            "theme" => match value.as_str().filter(|id| themes::get(id).is_some()) {
                Some(id) => {
                    shared.theme = id.to_string();
                    true
                }
                None => false,
            },
            _ => match serde_json::from_value::<Messages>(value.clone()) {
                Ok(messages) => {
                    shared.messages = messages;
                    true
                }
                Err(_) => false,
            },
        };
        if !taken {
            ignored.push(Ignored { domain: TERMINAL.to_string(), key: key.to_string(), value });
        }
    }
    (shared, ignored)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}
```

Run: `cargo test -p jawd settings::tests`
Expected: 7 passed.

- [ ] **Step 4: Write the socket's failing tests**

In `crates/jawd/src/sock.rs`'s test module, add these imports beside the others:

```rust
    use jaw_proto::settings::{Messages, Settings, TERMINAL};
```

and these helpers and tests:

```rust
    /// jawd serving jaw.db with its settings, on a socket in a folder of its own.
    async fn serving_settings() -> (tempfile::TempDir, std::path::PathBuf, Store) {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let settings = Arc::new(crate::settings::Settings::load(store.clone()).unwrap());
        let socket = jaw_proto::local::socket(home.path());
        let listener = listen(&socket).unwrap();
        let state = State::new(
            home.path(),
            Err(("go", "no engine in this test".into())),
            Vec::new(),
            Arc::default(),
            Some(store.clone()),
            broadcast::channel(16).0,
        )
        .unwrap()
        .with_settings(settings);
        tokio::spawn(serve(Arc::new(state), listener));
        (home, socket, store)
    }

    /// The answer to a hello from `client` as jawd wrote it, since `Settings` reads an empty or
    /// unknown value as the default.
    fn raw_hello(socket: &Path, client: &str) -> Value {
        let stream = std::os::unix::net::UnixStream::connect(socket).unwrap();
        let mut heard = BufReader::new(stream.try_clone().unwrap());
        let hello = json!({"id": 1, "method": "hello", "params": {"protocol": 1, "client": client, "version": "0"}});
        (&stream).write_all(format!("{hello}\n").as_bytes()).unwrap();
        let mut line = String::new();
        heard.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn hello_carries_a_domains_settings_and_each_change_is_pushed_to_its_connections_alone() {
        let (_home, socket, _store) = serving_settings().await;
        let checked = tokio::task::spawn_blocking(move || {
            let fresh = raw_hello(&socket, TERMINAL);
            assert_eq!(fresh["result"]["settings"], json!({"theme": "tokyo-night", "messages": "bubbles"}));
            let (mut a, _) = jaw_client::Connection::open(&socket, TERMINAL, "0").unwrap();
            let (mut b, _) = jaw_client::Connection::open(&socket, TERMINAL, "0").unwrap();
            // A client of no domain, read line by line, so a push to it would show.
            let other = std::os::unix::net::UnixStream::connect(&socket).unwrap();
            let mut heard = BufReader::new(other.try_clone().unwrap());
            let mut line = String::new();
            let mut say = |request: &str, line: &mut String| {
                (&other).write_all(format!("{request}\n").as_bytes()).unwrap();
                line.clear();
                heard.read_line(line).map(|_| serde_json::from_str::<Value>(line).unwrap())
            };
            let hello = r#"{"id":1,"method":"hello","params":{"protocol":1,"client":"test","version":"0"}}"#;
            let welcome = say(hello, &mut line).unwrap();
            assert!(welcome["result"].get("settings").is_none(), "a client jawd keeps no settings for gets none");
            let kept = a.call("settings.set", json!({"theme": "gruvbox", "messages": "irc"})).unwrap();
            assert_eq!(kept, json!({"settings": {"theme": "gruvbox", "messages": "irc"}}));
            let gruvbox = Settings { theme: "gruvbox".into(), messages: Messages::Irc };
            for connection in [&mut a, &mut b] {
                let push = connection.push().unwrap();
                assert_eq!(push.method, "settings");
                assert_eq!(serde_json::from_value::<Settings>(push.params).unwrap(), gruvbox);
            }
            let asked = r#"{"id":2,"method":"settings.set","params":{"theme":"nord"}}"#;
            let answer = say(asked, &mut line).unwrap();
            assert_eq!(answer["id"], 2, "the next line it hears is its answer, not a push: {answer}");
            assert_eq!(answer["error"]["code"], code::NOT_FOUND);
            assert_eq!(answer["error"]["data"], json!({"client": "test"}));
            let (_, later) = jaw_client::Connection::open(&socket, TERMINAL, "0").unwrap();
            assert_eq!(later.settings, Some(gruvbox), "a later hello carries them");
            let refused = a.call("settings.set", json!({"theme": "solarized"})).unwrap_err();
            assert!(refused.to_string().contains("theme takes one of"), "{refused}");
        });
        tokio::time::timeout(Duration::from_secs(20), checked).await.expect("no push or answer went missing").unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn settings_set_answers_only_once_its_commit_is_synced() {
        let (home, socket, store) = serving_settings().await;
        let path = socket.clone();
        let (mut connection, _) =
            tokio::task::spawn_blocking(move || jaw_client::Connection::open(&path, TERMINAL, "0").unwrap()).await.unwrap();
        // jaw.db's write lock, held from outside, so the setting can't be kept until it's let go.
        let holder = rusqlite::Connection::open(home.path().join("jaw.db")).unwrap();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();
        let synced = store.storage().synced_commits;
        let (answered, mut answer) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let _ = answered.send(connection.call("settings.set", json!({"theme": "nord"})).map_err(|e| e.to_string()));
        });
        let early = tokio::time::timeout(Duration::from_millis(400), &mut answer).await;
        assert!(early.is_err(), "answered while jaw.db couldn't keep it: {early:?}");
        holder.execute_batch("COMMIT").unwrap();
        let answer = tokio::time::timeout(Duration::from_secs(5), answer).await.expect("answered once it could").unwrap();
        assert_eq!(answer.unwrap()["settings"]["theme"], "nord");
        assert_eq!(store.storage().synced_commits - synced, 1, "kept with one synced commit");
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().as_deref(), Some("\"nord\""));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_connection_that_reads_nothing_while_settings_change_ends_on_the_latest() {
        let (_home, socket, _store) = serving_settings().await;
        let seen = tokio::task::spawn_blocking(move || {
            let stream = std::os::unix::net::UnixStream::connect(&socket).unwrap();
            let mut writer = stream.try_clone().unwrap();
            writer
                .write_all(b"{\"id\":1,\"method\":\"hello\",\"params\":{\"protocol\":1,\"client\":\"jaw\",\"version\":\"0\"}}\n")
                .unwrap();
            let (mut fast, _) = jaw_client::Connection::open(&socket, TERMINAL, "0").unwrap();
            for i in 0..150 {
                fast.call("settings.set", json!({"theme": ["gruvbox", "nord"][i % 2]})).unwrap();
            }
            fast.call("settings.set", json!({"theme": "kanagawa"})).unwrap();
            stream.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
            let mut reader = BufReader::new(stream);
            let (mut last, mut line) = (None, String::new());
            while reader.read_line(&mut line).is_ok_and(|n| n > 0) {
                let value: Value = serde_json::from_str(&line).unwrap();
                if value["method"] == "settings" {
                    last = value["params"]["theme"].as_str().map(str::to_string);
                }
                line.clear();
            }
            last
        });
        let seen = tokio::time::timeout(Duration::from_secs(60), seen).await.unwrap().unwrap();
        assert_eq!(seen.as_deref(), Some("kanagawa"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn themes_lists_every_theme_in_order_with_its_colours() {
        let (_home, socket) = serving().await;
        let themes = tokio::task::spawn_blocking(move || {
            let (mut connection, _) = jaw_client::Connection::open(&socket, "test", "0").unwrap();
            connection.call("themes", Value::Null).unwrap()
        })
        .await
        .unwrap();
        let themes: jaw_proto::themes::Themes = serde_json::from_value(themes).unwrap();
        assert_eq!(themes.themes.as_slice(), jaw_proto::themes::all());
        assert_eq!(themes.themes[20].id, "tokyo-night");
    }
```

Run: `cargo test -p jawd sock::tests`
Expected: FAIL to compile: no method `with_settings`.

- [ ] **Step 5: Wire the settings into the state and the socket**

In `crates/jawd/src/state.rs`, add the field `settings: Option<Arc<crate::settings::Settings>>,` to `State` (after `sending`), `settings: None,` in `State::new`, and:

```rust
    /// Keeps each frontend's settings in `settings`.
    pub fn with_settings(mut self, settings: Arc<crate::settings::Settings>) -> State {
        self.settings = Some(settings);
        self
    }

    pub fn settings(&self) -> Option<&crate::settings::Settings> {
        self.settings.as_deref()
    }
```

Leave `welcome()`'s `settings: None`: the socket fills it in with the connection's own domain's, at its hello. In `status()`, replace the two placeholders with:

```rust
            settings: self.settings.as_ref().map(|s| s.by_domain()).unwrap_or_default(),
            settings_ignored: self.settings.as_ref().map(|s| s.ignored()).unwrap_or_default(),
```

In `crates/jawd/src/main.rs`'s `start`, before `let state = state::State::new(…)`, add `let settings = Arc::new(settings::Settings::load(store.clone())?);`, and chain `.with_settings(settings)` after `.with_sending(sending)`.

In `crates/jawd/src/sock.rs`:

- Import `use jaw_proto::settings::{Settings as Shared, SettingsKept};`, `use jaw_proto::themes::Themes;` and add `watch` to `use tokio::sync::{broadcast, mpsc};`.
- Add to `Session`:

```rust
    /// The client its hello gave, which names its domain of settings.
    client: String,
    /// Its domain's settings as this connection was last told them: made at its hello, whose
    /// answer carries what this receiver has seen, so no change between that answer and its first
    /// push is missed; none for a client jawd keeps no settings for.
    settings: Option<watch::Receiver<Shared>>,
```

- In `one`, add `client: String::new(),` and `settings: None,` to the `Session` it makes, and a branch to its `select!`, after the `windows` branch:

```rust
            settings = changed(&mut session.settings, session.greeted) => {
                let push = Push {
                    method: "settings".into(),
                    params: serde_json::to_value(settings).expect("jawd's own types serialize"),
                };
                out.send(encode(&push)).await.context("the interface stopped reading")?;
            }
```

- Add, after `async fn windows`:

```rust
/// The settings once they've changed since this connection was last told them: only after its
/// hello, and never once jawd has stopped publishing them. A push waits for room as a reply
/// does, so none is dropped, and changes made meanwhile end in the latest.
async fn changed(settings: &mut Option<watch::Receiver<Shared>>, greeted: bool) -> Shared {
    match settings {
        Some(settings) if greeted => match settings.changed().await {
            Ok(()) => settings.borrow_and_update().clone(),
            Err(_) => std::future::pending().await,
        },
        _ => std::future::pending().await,
    }
}
```

- In `answer`'s `"hello"` arm, replace `succeeded(id, &state.welcome())` with:

```rust
                session.settings = state.settings().and_then(|s| s.subscribe(&hello.client));
                session.client = hello.client;
                let mut welcome = state.welcome();
                welcome.settings = session.settings.as_mut().map(|s| s.borrow_and_update().clone());
                succeeded(id, &welcome)
```

- Add to `answer`'s `match`, before `"history.more"`:

```rust
        "settings.set" => match state.settings() {
            Some(settings) => match settings.set(&session.client, request.params).await {
                Ok(settings) => succeeded(id, &SettingsKept { settings }),
                Err(error) => failed(id, error),
            },
            None => failed(id, Error::new(code::INTERNAL, "this jawd has no jaw.db open, so it can't keep settings")),
        },
        "themes" => succeeded(id, &Themes { themes: jaw_proto::themes::all().to_vec() }),
```

- [ ] **Step 6: Say them in status, and add the module to the data layer's paths**

In `crates/jaw-client/src/lib.rs`'s `describe`, after the `windows` block:

```rust
    for (domain, settings) in &status.settings {
        let said: Vec<String> = settings
            .iter()
            .map(|(key, value)| format!("{key} {}", value.as_str().map_or_else(|| value.to_string(), str::to_string)))
            .collect();
        let mut line = format!("settings  {domain}: {}", said.join(", "));
        for ignored in status.settings_ignored.iter().filter(|i| &i.domain == domain) {
            line.push_str(&format!("; {} {} isn't one this jawd takes, so its default is used", ignored.key, ignored.value));
        }
        lines.push(line);
    }
```

and to its test module:

```rust
    #[test]
    fn status_says_the_settings_and_any_this_jawd_doesnt_take() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../protocol/fixtures/status.result.json")).unwrap();
        let status: jaw_proto::Status = serde_json::from_value(fixture["result"].clone()).unwrap();
        let said = super::describe(&status);
        assert!(
            said.contains(
                "settings  jaw: messages irc, theme tokyo-night; theme \"rose-pine-moon\" isn't one this jawd takes, so its default is used"
            ),
            "{said}"
        );
    }
```

In `scripts/check-data-audit.py`, add `"crates/jawd/src/settings",` to `DATA_PATHS`, after `"crates/jawd/src/sock.rs",`.

- [ ] **Step 7: Run the tests**

Run: `cargo test -p jawd settings && cargo test -p jawd sock && cargo test -p jaw-client && JAW_PLANS=write cargo test -p jawd plans && git diff --stat crates/jawd/src/store/plans.txt`
Expected: all pass; `plans.txt` unchanged (the settings module holds no SQL).

- [ ] **Step 8: Commit**

```bash
cargo fmt --all
git add crates/jawd crates/jaw-client/src/lib.rs scripts/check-data-audit.py
git commit -m "jawd serves each frontend's settings, the terminal's being the only domain so far: read from jaw.db at start, sent in a connection's hello's answer for the domain its client names, from a receiver made at that hello so no change between its answer and the first push is missed, and none for a client jawd keeps no settings for, whose settings.set is error 3; settings.set checks every setting asked before changing any, refusing an unknown one or value with -32602 naming it and what it takes, keeps those that differ from jaw.db in one synced commit, one change at a time, and only then publishes them on a watch channel and answers, the value already held answered at once with nothing written; each greeted connection of a domain pushes its settings as they change, waiting for room as a reply does, so none is dropped, and connections of other domains hear nothing; a value jaw.db holds that this jawd doesn't take is kept and read as its default, and status says every domain's settings and so; themes answers with every theme's colours; and the module joins the data layer's paths"
```

---

### Task 5: jaw's colours from Omarchy's palettes

**Files:**
- Modify: `crates/jaw/src/theme.rs` (rewritten), `crates/jaw/src/art.rs` (`qr`'s polarity), `crates/jaw/src/view/draw.rs` (the call to `qr`), `crates/jaw/src/view/spike_tests.rs` (`AS_IT_WAS` re-pinned)

**Interfaces:**
- Consumes: `jaw_proto::themes::{all, or_default, DEFAULT, Colors, Mode, Rgb}`.
- Produces: `Theme::named(id: &str, truecolor: bool) -> Theme`; `Theme::with(truecolor)` is now `Theme::named(DEFAULT, truecolor)`; `Theme.light: bool`; `Roles::of(&Colors) -> Roles` (the role colours as hex, for tests); `pub fn contrast(a: u32, b: u32) -> f64`; `art::qr(code: &str, light: bool) -> Vec<String>`.

- [ ] **Step 1: Write the failing tests**

Replace the test module at the bottom of `crates/jaw/src/theme.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_fall_back_to_their_nearest_of_the_256() {
        assert_eq!(nearest_256(0, 0, 0), 16);
        assert_eq!(nearest_256(255, 255, 255), 231);
        assert_eq!(nearest_256(0x1a, 0x1b, 0x26), 234, "Tokyo Night's background is a near-black grey");
        assert_eq!(nearest_256(0x7a, 0xa2, 0xf7), 111, "and its blue a soft blue");
    }

    #[test]
    fn every_theme_keeps_its_greys_in_their_bands_and_in_order() {
        for theme in jaw_proto::themes::all() {
            let r = Roles::of(&theme.colors);
            let c = |x: u32| contrast(x, r.bg);
            let id = &theme.id;
            assert!((1.4..=2.61).contains(&c(r.gutter)), "{id}: borders {:.2}", c(r.gutter));
            assert!((2.5..=4.51).contains(&c(r.comment)), "{id}: dim text {:.2}", c(r.comment));
            assert!(c(r.gutter) < c(r.comment) && c(r.comment) < c(r.muted), "{id}: borders, dim, secondary");
            assert!(c(r.muted) < c(r.text) && c(r.text) <= c(r.fg), "{id}: secondary, text, text that matters");
            assert!(c(r.text) >= 4.5, "{id}: text {:.2}", c(r.text));
            assert!(c(r.surface) >= 1.05 && c(r.selection) >= 1.15, "{id}: surface and selection stand out");
            assert!(contrast(r.fg, r.selection) >= 4.5, "{id}: text on the selection");
        }
    }

    #[test]
    fn every_colour_reads_at_least_three_to_one() {
        for theme in jaw_proto::themes::all() {
            let r = Roles::of(&theme.colors);
            for (what, colour) in [("link", r.link), ("mention", r.mention), ("online", r.online), ("notice", r.notice)]
                .into_iter()
                .chain(r.senders.iter().map(|s| ("sender", *s)))
            {
                assert!(contrast(colour, r.bg) >= 3.0, "{}: {what} {:.2}", theme.id, contrast(colour, r.bg));
            }
        }
    }

    #[test]
    fn a_grey_outside_its_band_is_moved_toward_the_ground_or_the_text_keeping_its_hue() {
        let theme = |id: &str| jaw_proto::themes::get(id).unwrap().colors;
        let near = |a: u32, b: u32| (0..3).all(|i| ((a >> (8 * i)) & 0xff).abs_diff((b >> (8 * i)) & 0xff) <= 2);
        let everforest = theme("everforest");
        let raised = Roles::of(&everforest).comment;
        assert_ne!(raised, everforest.dark_foreground.0, "Everforest's dim text is too dim, so it's raised");
        let toward_text = |i: i32| mix(everforest.dark_foreground.0, everforest.foreground.0, f64::from(i) / 200.0);
        assert!((0..=200).any(|i| near(toward_text(i), raised)), "on the line from its own grey to the text");
        let osaka = theme("osaka-jade");
        assert!(contrast(Roles::of(&osaka).comment, osaka.background.0) < contrast(osaka.dark_foreground.0, osaka.background.0));
    }

    #[test]
    fn tokyo_night_is_omarchys_and_an_unknown_theme_is_tokyo_night() {
        let r = Roles::of(&jaw_proto::themes::get("tokyo-night").unwrap().colors);
        assert_eq!(
            (r.bg, r.surface, r.selection, r.fg, r.text, r.comment, r.gutter, r.accent, r.link),
            (0x1a1b26, 0x24283b, 0x292e42, 0xc0caf5, 0xa9b1d6, 0x565f89, 0x414868, 0x7aa2f7, 0x449dab)
        );
        assert_eq!(Theme::named("no-such-theme", true), Theme::with(true));
        assert!(!Theme::with(true).light && Theme::named("catppuccin-latte", true).light);
    }
}
```

In `crates/jaw/src/art.rs`'s tests, change the existing QR test's call to `qr("2@jaw-demo,made-up,linking-code", false)` and add:

```rust
    #[test]
    fn in_a_light_theme_a_qr_code_is_dark_on_light() {
        let dark = qr("2@jaw-demo,made-up,linking-code", false);
        let light = qr("2@jaw-demo,made-up,linking-code", true);
        assert_eq!(dark.len(), light.len());
        assert!(light[0].chars().all(|c| c == ' '), "the quiet zone is the light ground: {}", light[0]);
        let swap = |c: char| match c {
            '█' => ' ',
            ' ' => '█',
            '▀' => '▄',
            '▄' => '▀',
            other => other,
        };
        for (d, l) in dark.iter().zip(&light).take(dark.len() - 1) {
            assert_eq!(d.chars().map(swap).collect::<String>(), *l, "each module the other way round");
        }
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p jaw theme:: && cargo test -p jaw art::`
Expected: FAIL to compile: no `Roles`, no `contrast`, `qr` takes one argument.

- [ ] **Step 3: Rewrite the theme**

Replace everything in `crates/jaw/src/theme.rs` above `pub fn color(` with:

```rust
//! The look: ooze's, in one of Omarchy's themes, falling back to the nearest of the 256 standard
//! colours where the terminal has no 24-bit colour. A theme is Omarchy's palette mapped onto what
//! each colour is for; drawing asks `Theme` for a role, never for a hue, and each role is kept
//! readable against the ground whatever the palette.

use jaw_proto::themes::{Colors, Mode};
use ratatui::style::Color;

/// Each role's colour as hex, from a theme's palette, before the terminal's colours are chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roles {
    pub bg: u32,
    pub surface: u32,
    pub selection: u32,
    pub fg: u32,
    pub text: u32,
    pub muted: u32,
    pub comment: u32,
    pub gutter: u32,
    pub accent: u32,
    pub link: u32,
    pub mention: u32,
    pub online: u32,
    pub notice: u32,
    pub senders: [u32; 8],
}

impl Roles {
    /// Omarchy's keys mapped onto jaw's roles. A grey outside its band of contrast with the ground
    /// is moved toward the ground or the text until it's in, keeping its hue, and every colour
    /// reads at least 3:1.
    pub fn of(c: &Colors) -> Roles {
        let bg = c.background.0;
        let (fg, text) = if contrast(c.bright_foreground.0, bg) >= contrast(c.foreground.0, bg) {
            (c.bright_foreground.0, c.foreground.0)
        } else {
            (c.foreground.0, c.bright_foreground.0)
        };
        let fit = |colour: u32, low: f64, high: f64, target: f64| fit(colour, bg, fg, low, high, target);
        let comment = fit(c.dark_foreground.0, 2.5, 4.5, 3.0);
        let hue = |colour: jaw_proto::themes::Rgb| fit(colour.0, 3.0, f64::INFINITY, 3.0);
        Roles {
            bg,
            surface: fit(c.lighter_background.0, 1.05, f64::INFINITY, 1.1),
            selection: fit(c.selection.0, 1.15, f64::INFINITY, 1.25),
            fg,
            text,
            muted: mix(comment, text, 0.5),
            comment,
            gutter: fit(c.muted.0, 1.4, 2.6, 1.8),
            accent: c.accent.0,
            link: hue(c.cyan),
            mention: hue(c.orange),
            online: hue(c.green),
            notice: hue(c.yellow),
            senders: [c.red, c.yellow, c.blue, c.magenta, c.cyan, c.green, c.orange, c.bright_cyan].map(hue),
        }
    }
}

/// WCAG's relative luminance of a colour.
fn luminance(c: u32) -> f64 {
    let channel = |shift: u32| {
        let v = f64::from((c >> shift) & 0xff) / 255.0;
        if v <= 0.039_28 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

/// WCAG's contrast between two colours, from 1 to 21.
pub fn contrast(a: u32, b: u32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `a` moved `t` of the way to `b`, channel by channel.
pub fn mix(a: u32, b: u32, t: f64) -> u32 {
    let channel = |shift: u32| {
        let (x, y) = (f64::from((a >> shift) & 0xff), f64::from((b >> shift) & 0xff));
        ((x + (y - x) * t).round() as u32).min(255) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// `colour` as it is when its contrast with `ground` is from `low` to `high`, and otherwise moved
/// toward `fg` or `ground` until its contrast is `target`.
fn fit(colour: u32, ground: u32, fg: u32, low: f64, high: f64, target: f64) -> u32 {
    let now = contrast(colour, ground);
    if (low..=high).contains(&now) {
        return colour;
    }
    let end = if now < low { fg } else { ground };
    let (mut near, mut far) = (0.0, 1.0);
    for _ in 0..30 {
        let t = (near + far) / 2.0;
        let reached = contrast(mix(colour, end, t), ground);
        if (now < low && reached < target) || (now > high && reached > target) {
            near = t;
        } else {
            far = t;
        }
    }
    mix(colour, end, far)
}

/// Each colour by what it's for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// The ground, a raised surface (overlays, monospace), and the selected row.
    pub bg: Color,
    pub surface: Color,
    pub selection: Color,
    /// Text that matters, ordinary text, secondary text, quieter still, and borders at rest.
    pub fg: Color,
    pub text: Color,
    pub muted: Color,
    pub comment: Color,
    pub gutter: Color,
    /// The name's pill in the header.
    pub brand: Color,
    /// The border of what has the keys, and the keys in the footer.
    pub focus: Color,
    pub link: Color,
    /// Where a search matched what it found.
    pub mark: Color,
    /// An unread count; a mention of you, and its count.
    pub unread: Color,
    pub mention: Color,
    /// Messages that came below while you read further up.
    pub arrived: Color,
    /// Someone typing, or online.
    pub online: Color,
    /// Ticks before they're read, and once they are.
    pub unseen: Color,
    pub read: Color,
    /// Something that needs you: a header in trouble, a message not sent.
    pub warn: Color,
    /// A passing notice in the footer.
    pub notice: Color,
    /// How far catching up has got.
    pub progress: Color,
    /// Group senders' names, by the index jawd gives each member.
    senders: [Color; 8],
    /// Whether colours are 24-bit, for the creature's own.
    pub truecolor: bool,
    /// Whether the theme is dark text on a light ground.
    pub light: bool,
}

impl Theme {
    pub fn new(r: &Roles, light: bool, truecolor: bool) -> Theme {
        let c = |h: u32| color(h, truecolor);
        Theme {
            bg: c(r.bg),
            surface: c(r.surface),
            selection: c(r.selection),
            fg: c(r.fg),
            text: c(r.text),
            muted: c(r.muted),
            comment: c(r.comment),
            gutter: c(r.gutter),
            brand: c(r.accent),
            focus: c(r.accent),
            link: c(r.link),
            mark: c(r.fg),
            unread: c(r.fg),
            mention: c(r.mention),
            arrived: c(r.mention),
            online: c(r.online),
            unseen: c(r.comment),
            read: c(r.link),
            warn: c(r.mention),
            notice: c(r.notice),
            progress: c(r.accent),
            senders: r.senders.map(c),
            truecolor,
            light,
        }
    }

    /// The theme `id` names, Tokyo Night where it names none jaw knows.
    pub fn named(id: &str, truecolor: bool) -> Theme {
        let theme = jaw_proto::themes::or_default(id);
        Theme::new(&Roles::of(&theme.colors), theme.mode == Mode::Light, truecolor)
    }

    pub fn with(truecolor: bool) -> Theme {
        Theme::named(jaw_proto::themes::DEFAULT, truecolor)
    }

    pub fn sender(&self, colour: u8) -> Color {
        self.senders[colour as usize % self.senders.len()]
    }
}
```

Keep `color`, `truecolor`, `macos_major`, `nearest_256` and `no_color` below it as they are.

- [ ] **Step 4: Draw the QR code dark on light in a light theme**

In `crates/jaw/src/art.rs`, replace `qr` and its doc comment with:

```rust
/// A QR code in half-blocks with its quiet zone, two modules to a cell, the top one as the glyph:
/// light on dark for a dark theme, its light modules drawn in the text colour, and dark on light
/// for a light theme, its dark modules drawn in it, so a phone reads it as printed codes are.
pub fn qr(code: &str, light: bool) -> Vec<String> {
    let Ok(qr) = qrcode::QrCode::new(code.as_bytes()) else { return vec![] };
    let n = qr.width();
    let quiet = 2;
    let dark = |x: isize, y: isize| {
        x >= 0 && y >= 0 && (x as usize) < n && (y as usize) < n && qr[(x as usize, y as usize)] == qrcode::Color::Dark
    };
    let side = (n + 2 * quiet) as isize;
    (0..side)
        .step_by(2)
        .map(|y| {
            (0..side)
                .map(|x| {
                    let (x, y) = (x - quiet as isize, y - quiet as isize);
                    let below = y + 1 < side - quiet as isize;
                    let drawn = if light {
                        (dark(x, y), dark(x, y + 1) && below)
                    } else {
                        (!dark(x, y), !dark(x, y + 1) && below)
                    };
                    match drawn {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        (false, false) => ' ',
                    }
                })
                .collect()
        })
        .collect()
}
```

In `crates/jaw/src/view/draw.rs`'s `draw_linking`, change `art::qr(code)` to `art::qr(code, t.light)`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p jaw theme:: && cargo test -p jaw art::`
Expected: all pass.

- [ ] **Step 6: Re-pin the spike's screens**

Every colour of the default screen changed on purpose, so `with_every_switch_off_the_screen_is_as_it_was_byte_for_byte` fails. In `crates/jaw/src/view/spike_tests.rs`, temporarily replace that test's `assert_eq!(now, was, …)` with `eprintln!("({state:?}, {w}, {h}, {keys:?}, 0x{now:016x}),");`, run `cargo test -p jaw with_every_switch_off -- --nocapture`, copy the 14 new digests into `AS_IT_WAS` in order (keep the comment line above the last entry), and put the `assert_eq!` back exactly as it was. The last entry keeps the same digest as the 140×40 one, as before.

Run: `cargo test -p jaw`
Expected: every test passes. If another test fails on a colour, it pinned Tokyo Night's old palette: change it to read the role from `Theme::with(true)`, and name it in the commit message.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all
git add crates/jaw/src/theme.rs crates/jaw/src/art.rs crates/jaw/src/view/draw.rs crates/jaw/src/view/spike_tests.rs
git commit -m "jaw draws in Omarchy's themes: each palette is mapped onto jaw's colour roles by one rule, the accent taking the pill, the focused border and progress, cyan links and read ticks, orange mentions and trouble, and every grey held in a band of contrast with the ground, borders 1.4 to 2.6:1, dim text 2.5 to 4.5:1, secondary text halfway to the text, a grey outside its band moved toward the ground or the text keeping its hue and every colour at least 3:1, tested in all 23; Tokyo Night becomes Omarchy's own, so the default screen's colours change and the spike's fourteen digests are pinned again; and a light theme draws the linking screen's QR code dark on light"
```

---

### Task 6: jaw follows the settings

**Files:**
- Modify: `crates/jaw/src/data.rs`, `crates/jaw/src/wire.rs`, `crates/jaw/src/wire/tests.rs`, `crates/jaw/src/demo.rs`, `crates/jaw/src/view.rs`, `crates/jaw/src/view/tests.rs`

**Interfaces:**
- Consumes: `jaw_proto::settings::{Settings, SettingsSet, SettingsKept, Messages}`; `Theme::named` (Task 5).
- Produces: `Update::Hello { …, settings: Settings }`; `Update::Settings(Settings)`; `Update::SettingsRefused(String)`; `Jawd::set_settings(&mut self, change: &SettingsSet)`; on `View`: fields `settings: Settings`, `settings_asked: Option<Settings>`, `theme_id: String`, `born: i64`, and methods `settings_shown(&self) -> &Settings`, `settings_came(&mut self, Settings)`, `ask_settings(&mut self, SettingsSet)`, `restyle(&mut self)`; `Demo::settings(&self) -> Settings` for tests; `const PATIENCE: i64 = 250` in `view.rs`.

- [ ] **Step 1: Write the failing tests**

In `crates/jaw/src/view/tests.rs`, add `use jaw_proto::settings::{Messages, Settings};` to the imports and:

```rust
#[test]
fn nothing_is_drawn_before_jawd_answers_and_the_first_frame_is_in_the_theme_hello_brings() {
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut v = View::new(Box::new(Demo::start(tx, Clock::Fixed(NOW), State::Online, false)));
    v.tick();
    assert!(!v.dirty, "nothing to draw before jawd says hello");
    let gruvbox = Settings { theme: "gruvbox".into(), messages: Messages::Irc };
    v.apply(Update::Hello {
        root: String::new(),
        linked: true,
        me: vec![],
        connection: Connection::new(ConnectionState::Online),
        settings: gruvbox.clone(),
    });
    assert!(v.dirty && v.awaited);
    assert_eq!(v.theme, Theme::named("gruvbox", v.theme.truecolor));
    assert_eq!(v.settings_shown(), &gruvbox);
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut quiet = View::new(Box::new(Demo::start(tx, Clock::Fixed(NOW), State::Online, false)));
    quiet.born -= PATIENCE;
    quiet.tick();
    assert!(quiet.dirty, "a jawd that says nothing for a quarter of a second still gets a frame saying so");
}

#[test]
fn a_change_shows_while_jawd_keeps_it_and_one_it_doesnt_keep_goes_back_and_says_why() {
    let (mut v, _rx) = view(140, 40);
    v.ask_settings(jaw_proto::settings::SettingsSet { theme: Some("nord".into()), messages: None });
    assert_eq!(v.theme, Theme::named("nord", true), "shown at once");
    v.apply(Update::SettingsRefused("jaw.db can't be written".into()));
    assert_eq!(v.theme, Theme::with(true), "back to what jawd has");
    assert!(frame(&mut v).lines().last().unwrap().contains("not kept: jaw.db can't be written"));
    v.apply(Update::Settings(Settings { theme: "kanagawa".into(), messages: Messages::Bubbles }));
    assert_eq!(v.theme, Theme::named("kanagawa", true), "another interface's change shows here too");
}
```

In the existing test at `view/tests.rs` that builds `let hello = |connection| Update::Hello { root: String::new(), linked: false, me: vec![], connection };`, add `settings: Settings::default()` to it.

In `crates/jaw/src/wire/tests.rs`, add to `answer`'s `match`, before `"unwatch" | …`:

```rust
        "settings.set" if params["theme"] == "solarized" => {
            let data = json!({ "param": "theme", "allowed": ["gruvbox", "nord"] });
            return error(code::INVALID_PARAMS, "theme takes one of gruvbox, nord", data);
        }
        "settings.set" => json!({ "settings": {
            "theme": params.get("theme").cloned().unwrap_or(json!("tokyo-night")),
            "messages": params.get("messages").cloned().unwrap_or(json!("bubbles")),
        } }),
```

and this test:

```rust
#[test]
fn the_settings_hello_brings_and_jawd_keeps_come_back_and_one_it_refuses_says_why() {
    let jawd = scripted();
    let (tx, rx) = channel();
    let mut wire = Wire::connect(jawd.socket.clone(), tx);
    let mut said = vec![];
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Hello { .. })));
    assert!(said.iter().any(|u| matches!(u, Update::Hello { settings, .. } if settings.theme == "tokyo-night")));
    wire.set_settings(&jaw_proto::settings::SettingsSet { theme: Some("gruvbox".into()), messages: None });
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Settings(_))));
    assert!(said.iter().any(|u| matches!(u, Update::Settings(s) if s.theme == "gruvbox")));
    wire.set_settings(&jaw_proto::settings::SettingsSet { theme: Some("solarized".into()), messages: None });
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::SettingsRefused(_))));
    assert!(said.iter().any(|u| matches!(u, Update::SettingsRefused(why) if why.contains("theme takes one of"))));
    assert_eq!(jawd.asked("settings.set"), [json!({ "theme": "gruvbox" }), json!({ "theme": "solarized" })]);
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p jaw first_frame && cargo test -p jaw settings_hello_brings`
Expected: FAIL to compile: `Update::Hello` has no field `settings`, and the like.

- [ ] **Step 3: Carry the settings in the updates and the trait**

In `crates/jaw/src/data.rs`:

- Add `settings: jaw_proto::settings::Settings,` to `Update::Hello`, after `connection`, with the doc line `/// The terminal's settings, as jawd keeps them in the jaw domain, or the defaults when hello brings none.`
- Add to `Update`, after `Connection(Connection)`:

```rust
    /// jawd's settings, as a push or `settings.set`'s answer carries them: all of them.
    Settings(jaw_proto::settings::Settings),
    /// jawd didn't keep the settings asked, and why.
    SettingsRefused(String),
```

- In `Update::awaited`, add `Update::Settings(_) => true,`.
- Add to `trait Jawd`, after `focus`:

```rust
    /// Asks jawd to keep these settings for the terminal; they come back as `Update::Settings`, or
    /// `Update::SettingsRefused` with why.
    fn set_settings(&mut self, change: &jaw_proto::settings::SettingsSet);
```

In `crates/jaw/src/wire.rs`:

- Add `Settings,` to `enum Asked`, with the doc line `/// A change to the settings, which the interface shows until jawd answers.`
- In `refused`, add `Asked::Settings => Some(Update::SettingsRefused(why_not(failure, "settings"))),`.
- In `event`'s `From::Connected(w)` arm, add `settings: w.settings.unwrap_or_default(),` to the `Update::Hello` it pushes; jaw says hello as `jaw`, the terminal's domain, so jawd sends them.
- In `event`'s reply match, add before `(asked, Err(failure)) => …`:

```rust
                    (Asked::Settings, Ok(result)) => {
                        if let Ok(kept) = serde_json::from_value::<jaw_proto::settings::SettingsKept>(result) {
                            out.push(Update::Settings(kept.settings));
                        }
                    }
```

- In `event`, add before `From::Push(push) if push.method == "library"`:

```rust
            From::Push(push) if push.method == "settings" => {
                out.extend(serde_json::from_value(push.params).ok().map(Update::Settings));
            }
```

- In `impl Jawd for Wire`, after `focus`:

```rust
    fn set_settings(&mut self, change: &jaw_proto::settings::SettingsSet) {
        let params = serde_json::to_value(change).unwrap_or(Value::Null);
        self.ask("settings.set", params, Asked::Settings);
    }
```

In `crates/jaw/src/demo.rs`:

- Add `settings: jaw_proto::settings::Settings,` to `Account` (doc line `/// The settings the made-up account keeps, as jawd would.`) and `settings: jaw_proto::settings::Settings::default(),` where `Account` is made.
- In `Account::hello`, add `settings: self.settings.clone()` to the `Update::Hello`.
- Add to `impl Account`:

```rust
    /// Keeps the settings asked, as jawd does: a theme it doesn't know is refused.
    fn on_set_settings(&mut self, change: jaw_proto::settings::SettingsSet) {
        if let Some(theme) = change.theme.as_deref().filter(|t| jaw_proto::themes::get(t).is_none()) {
            self.send(Update::SettingsRefused(format!("jawd doesn't know the theme {theme}")));
            return;
        }
        if let Some(theme) = change.theme {
            self.settings.theme = theme;
        }
        if let Some(messages) = change.messages {
            self.settings.messages = messages;
        }
        self.send(Update::Settings(self.settings.clone()));
    }
```

- Add to `impl Jawd for Demo`:

```rust
    fn set_settings(&mut self, change: &jaw_proto::settings::SettingsSet) {
        let change = change.clone();
        self.defer(move |a| a.on_set_settings(change));
    }
```

- Add to `impl Demo`, in `crates/jaw/src/demo.rs`, reading the account as `quick_look_watched` does:

```rust
    /// The settings the made-up account keeps, for tests.
    pub fn settings(&self) -> jaw_proto::settings::Settings {
        self.lock().settings.clone()
    }
```

- [ ] **Step 4: Follow them in the view**

In `crates/jaw/src/view.rs`:

- Add, beside the other constants:

```rust
/// How long jaw waits for jawd's hello before drawing anything, so its first frame is in the
/// theme jawd keeps rather than a flash of the default.
pub const PATIENCE: i64 = 250;
```

- Add to `View`, after `pub tz: FixedOffset,`:

```rust
    /// jawd's settings, as hello, a push or an answer last said, and what was last asked of it
    /// while its answer is on the way.
    settings: jaw_proto::settings::Settings,
    settings_asked: Option<jaw_proto::settings::Settings>,
    /// The theme the screen is drawn in, by ID.
    theme_id: String,
    /// When the view was made, by the wall clock, so it draws even when jawd says nothing.
    born: i64,
```

- In `View::new`, set `settings: jaw_proto::settings::Settings::default()`, `settings_asked: None`, `theme_id: jaw_proto::themes::DEFAULT.to_string()`, `born: chrono::Utc::now().timestamp_millis()`, and change `dirty: true` to `dirty: false`.
- In `apply`'s `Update::Hello` arm, destructure `settings` too and call `self.settings_came(settings);` before `self.connection = connection;`. Add arms:

```rust
            Update::Settings(settings) => self.settings_came(settings),
            Update::SettingsRefused(why) => {
                self.settings_asked = None;
                self.restyle();
                self.say(format!("not kept: {why}"));
            }
```

- Add to `impl View`:

```rust
    /// The settings the screen shows: what was last asked of jawd while its answer is on the way,
    /// or what jawd last said.
    pub fn settings_shown(&self) -> &jaw_proto::settings::Settings {
        self.settings_asked.as_ref().unwrap_or(&self.settings)
    }

    /// jawd's settings came, in hello, a push or an answer; they're what the screen shows now.
    fn settings_came(&mut self, settings: jaw_proto::settings::Settings) {
        self.settings = settings;
        self.settings_asked = None;
        self.restyle();
    }

    /// Asks jawd to keep `change` for the terminal, showing it meanwhile.
    fn ask_settings(&mut self, change: jaw_proto::settings::SettingsSet) {
        let mut asked = self.settings_shown().clone();
        if let Some(theme) = &change.theme {
            asked.theme = theme.clone();
        }
        if let Some(messages) = change.messages {
            asked.messages = messages;
        }
        self.settings_asked = Some(asked);
        self.jawd.set_settings(&change);
        self.restyle();
    }

    /// Draws the screen in the theme it shows; a theme that changes lays every message and chat
    /// row out again, in its colours.
    fn restyle(&mut self) {
        let id = self.settings_shown().theme.clone();
        if id != self.theme_id {
            self.theme = Theme::named(&id, self.theme.truecolor);
            self.theme_id = id;
            self.laid.clear();
            self.rows_laid.clear();
        }
        self.dirty = true;
    }
```

- In `tick`, replace

```rust
        if minute != self.drawn_at / 60 || (self.counting() && second != self.drawn_at) {
            self.dirty = true;
        }
```

with

```rust
        if self.reach == Reach::Waiting && self.drawn_at == 0 {
            self.dirty |= now - self.born >= PATIENCE;
        } else if minute != self.drawn_at / 60 || (self.counting() && second != self.drawn_at) {
            self.dirty = true;
        }
```

- In `next_wake`, after the `soon(...)` calls, add:

```rust
        if self.reach == Reach::Waiting && self.drawn_at == 0 {
            soon(Some(self.born + PATIENCE));
        }
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p jaw`
Expected: every test passes, the two new view tests and the new wire test included. A test that relied on a frame before jawd's hello now gets one only once an update arrives; if one fails for that, apply an update or call `run::settle` first, and name it in the commit message.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add crates/jaw/src
git commit -m "jaw follows the settings jawd keeps: hello's answer, settings pushes and settings.set's answers each bring all of them, and the screen shows what was last asked while jawd's answer is on its way, going back to what jawd has with the footer saying why when it isn't kept; a theme that changes lays every message and chat row out again in its colours; nothing is drawn until jawd says hello or that it can't, or a quarter of a second has passed, so the first frame is already in the theme kept; and the made-up account keeps settings as jawd does, refusing a theme it doesn't know"
```

---

### Task 7: The IRC layout

**Files:**
- Modify: `crates/jaw/src/layout.rs`, `crates/jaw/src/view.rs` (`shape`), `crates/jaw/src/view/draw.rs` (`detail`), `crates/jaw/src/view/tests.rs`

**Interfaces:**
- Consumes: `View::settings_shown()` (Task 6), `jaw_proto::settings::Messages`.
- Produces: `Shape.irc: bool`; `layout::YOU: &str = "You"`; `layout::margins(mine: bool, irc: bool, width: usize) -> (usize, usize)`, where a message's text starts and how much its right edge keeps.

- [ ] **Step 1: Write the failing tests**

In `crates/jaw/src/layout.rs`'s tests, change `lay` to build its `Shape` with `irc: false`, and add:

```rust
    fn lay_irc(m: &Message, width: usize, name: bool) -> Laidout {
        let theme = Theme::with(true);
        let me = ["person:0".to_string()];
        let ctx = Ctx { theme: &theme, tz: UTC, me: &me, pictures: false, cell: (8, 16), sharp: false };
        laid(m, Shape { width, name, time: true, unfolded: false, pictured: true, play: None, irc: true }, &ctx)
    }

    #[test]
    fn in_the_irc_layout_yours_sit_under_you_at_the_name_column_with_their_ticks_at_the_edge() {
        let mut m = theirs("yes, the one at the lake");
        m.mine = true;
        m.from = None;
        m.status = Some(Tick::Read);
        let first = lay_irc(&m, 60, true);
        assert_eq!(shown(&first.lines), [format!("You   yes, the one at the lake{}✓✓  13:58", " ".repeat(21))]);
        let you = &first.lines[0].spans[0];
        assert_eq!(you.content, "You");
        assert_eq!(you.style, Style::new().fg(Theme::with(true).fg).add_modifier(Modifier::BOLD));
        let next = shown(&lay_irc(&m, 60, false).lines);
        assert!(next[0].starts_with("      yes, the one at the lake") && next[0].contains("✓✓"), "{next:?}");
        m.reply = Some(Quote { id: None, from: Some("Ana".into()), colour: 0, summary: "trazes o vinho?".into() });
        m.reactions = vec![Reaction { emoji: "❤".into(), n: 2, mine: false }];
        let lines = shown(&lay_irc(&m, 60, true).lines);
        assert!(lines[0].starts_with("You   ┃ Ana trazes o vinho?"), "{lines:?}");
        assert!(lines.last().unwrap().starts_with("       ❤ 2"), "reactions start at the name column: {lines:?}");
        assert_eq!(margins(true, true, 60), (NAME, 10));
        assert_eq!(margins(true, false, 60), (20, 10));
        assert_eq!(margins(false, true, 60), (NAME, 7));
    }
```

In `crates/jaw/src/view/tests.rs`, add:

```rust
#[test]
fn in_the_irc_layout_yours_start_at_the_name_column_under_you() {
    let (mut v, _rx) = view(140, 40);
    v.apply(Update::Settings(Settings { theme: "tokyo-night".into(), messages: Messages::Irc }));
    let screen = frame(&mut v);
    let mine = screen.lines().find(|l| l.contains("cheguei, boa noite")).unwrap();
    let at = mine[..mine.find("cheguei").unwrap()].chars().count();
    assert_eq!(at, 26 + 34 + 2 + 6, "yours start at the name column:\n{mine}");
    assert!(screen.lines().any(|l| l.contains("You   ")), "under You:\n{screen}");
    assert!(mine.ends_with("✓✓  23:12 │"), "{mine}");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p jaw irc_layout`
Expected: FAIL to compile: `Shape` has no field `irc`; no `margins`.

- [ ] **Step 3: Lay yours out under "You"**

In `crates/jaw/src/layout.rs`:

- Add to `Shape`, after `play`:

```rust
    /// Yours on the left under "You", as everyone's are, as IRC clients put them, rather than in
    /// a column from a third of the way across.
    pub irc: bool,
```

- Add after `indent`:

```rust
/// What labels your messages in the IRC layout.
pub const YOU: &str = "You";

/// Where a message's text starts and what its right edge keeps: others', and yours in the IRC
/// layout, at the name column; yours otherwise a third of the way across, with room for ticks.
pub fn margins(mine: bool, irc: bool, width: usize) -> (usize, usize) {
    match (mine, irc) {
        (true, false) => (indent(width), TIME + TICKS),
        (true, true) => (NAME, TIME + TICKS),
        (false, _) => (NAME, TIME),
    }
}
```

- In `laid`, replace `let (left, right) = if m.mine { (indent(w), TIME + TICKS) } else { (NAME, TIME) };` with `let (left, right) = margins(m.mine, shape.irc, w);`.
- In `text`, replace the same line the same way, and replace

```rust
    let name = m.from.as_ref().filter(|_| shape.name && !m.mine).map(|f| fmt::fit(&f.name, 20));
```

with

```rust
    let name = if m.mine {
        (shape.irc && shape.name).then(|| YOU.to_string())
    } else {
        m.from.as_ref().filter(|_| shape.name).map(|f| fmt::fit(&f.name, 20))
    };
```

- In `text`'s loop over the wrapped lines, change `if m.mine {` to `if m.mine && !shape.irc {`, and in the `else if i == 0 && name_w > 0` branch replace the colour with the style each name takes:

```rust
        } else if i == 0 && name_w > 0 {
            let style = if m.mine {
                Style::new().fg(t.fg).add_modifier(Modifier::BOLD)
            } else {
                let colour = m.from.as_ref().map_or(t.sender(0), |f| t.sender(f.colour));
                Style::new().fg(colour).add_modifier(Modifier::BOLD)
            };
            let n = name.clone().unwrap_or_default();
            let pad = NAME.max(name_w) - n.width();
            spans.push(Span::styled(n, style));
            spans.push(Span::raw(" ".repeat(pad)));
            used += NAME.max(name_w);
        } else {
```

In `crates/jaw/src/view.rs`'s `shape`, add `irc: self.settings_shown().messages == jaw_proto::settings::Messages::Irc,` to the `Shape` it returns.

In `crates/jaw/src/view/draw.rs`'s `detail`, replace `let lead = if m.mine { layout::indent(w) } else { layout::NAME };` with:

```rust
        let irc = self.settings_shown().messages == jaw_proto::settings::Messages::Irc;
        let lead = layout::margins(m.mine, irc, w).0;
```

Add `irc: false` to any other `Shape { … }` the crate builds (`rg "Shape \{" crates/jaw/src`).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p jaw`
Expected: every test passes; `bubbles` screens are unchanged, so the spike's digests still match.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/jaw/src
git commit -m "The IRC layout: with messages set to irc, yours are laid out as everyone's are, under \"You\" in the name column in the brightest text, bold, apart from group senders' colours, shown when the sender changes, after half an hour or on a new day, with ticks and the time kept at the right edge, and a quote, reactions, pictures, a long message's more lines, not sent and the selected message's detail starting at the name column too; bubbles stays exactly as it was"
```

---

### Task 8: The settings panel

**Files:**
- Create: `crates/jaw/src/view/settings.rs`
- Modify: `crates/jaw/src/view.rs` (`mod settings;`, the panel's field, `restyle`), `crates/jaw/src/view/keys.rs`, `crates/jaw/src/view/draw.rs` (`draw_main`, `hints`, `draw_composer`, `draw_keys`), `crates/jaw/src/view/composer.rs` (the spike's box, if it draws the cursor), `crates/jaw/src/view/palette.rs`, `crates/jaw/src/view/spike_tests.rs` (digests that change)

**Interfaces:**
- Consumes: `ask_settings`, `restyle`, `settings_shown` (Task 6); `jaw_proto::themes::all`.
- Produces: `settings::Panel { row: Row, choosing: Option<usize> }`, `settings::Row { Theme, Messages }`; `View.settings_panel: Option<settings::Panel>`; `View::{open_settings(bool), close_settings(), on_settings_key(KeyEvent), settings_hints(), draw_settings(&mut Frame, Rect), previewed() -> Option<&'static str>, theme_index() -> usize}`; palette `Does::{Settings, Theme, Messages(Messages)}`.

- [ ] **Step 1: Write the failing tests**

Create `crates/jaw/src/view/settings.rs` with only this test module for now, and add `mod settings;` to `crates/jaw/src/view.rs` after `mod quick_look;`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Update;
    use crate::demo::{Clock, Demo, State};
    use crate::run::{self, Event};
    use crate::theme::Theme;
    use jaw_proto::settings::Settings;
    use std::sync::mpsc::Receiver;

    fn view(width: u16, height: u16) -> (View, Receiver<Event>, Demo) {
        let (tx, rx) = std::sync::mpsc::channel();
        let demo = Demo::start(tx, Clock::Fixed(crate::view::tests::NOW), State::Online, false);
        let mut v = View::new(Box::new(demo.clone()));
        v.clock = Some(crate::view::tests::NOW);
        v.tz = chrono::FixedOffset::east_opt(0).unwrap();
        v.theme = Theme::with(true);
        v.no_color = false;
        v.resize(width, height);
        run::settle(&mut v, &rx);
        (v, rx, demo)
    }

    fn frame(v: &mut View) -> String {
        let (w, h) = v.size().unwrap();
        run::snapshot(v, w, h)
    }

    #[test]
    fn ctrl_comma_opens_the_panel_in_the_chat_lists_place_and_closes_it_again() {
        let (mut v, rx, _) = view(140, 40);
        run::press(&mut v, &rx, "<c-,>").unwrap();
        let screen = frame(&mut v);
        assert!(screen.contains("╭ settings ─") && !screen.contains("╭ chats · All ─"), "{screen}");
        assert!(screen.contains("Theme") && screen.contains("Tokyo Night ›"), "{screen}");
        assert!(screen.contains("◉ on the right") && screen.contains("○ on the left, as IRC"), "{screen}");
        assert!(screen.contains("╭ Ana Ribeiro"), "the conversation stays beside it:\n{screen}");
        assert!(screen.lines().last().unwrap().contains("esc closes"), "{screen}");
        run::press(&mut v, &rx, "<c-,>").unwrap();
        assert!(frame(&mut v).contains("╭ chats · All ─"));
    }

    #[test]
    fn moving_through_the_themes_previews_each_esc_goes_back_and_enter_keeps_one() {
        let (mut v, rx, demo) = view(140, 40);
        run::press(&mut v, &rx, "<c-,>\n").unwrap();
        assert!(frame(&mut v).contains("› theme"), "Enter opens the theme list");
        run::press(&mut v, &rx, "<down>").unwrap();
        assert_eq!(v.theme, Theme::named("vantablack", true), "the theme under the keys is previewed");
        assert_eq!(demo.settings().theme, "tokyo-night", "and nothing is asked of jawd");
        run::press(&mut v, &rx, "<esc>").unwrap();
        assert_eq!(v.theme, Theme::with(true), "Esc goes back to the theme kept");
        run::press(&mut v, &rx, "\n<up>\n").unwrap();
        assert_eq!(demo.settings().theme, "starship");
        assert_eq!(v.theme, Theme::named("starship", true));
        assert!(frame(&mut v).contains("Starship ›"), "back in the panel, with the theme kept");
    }

    #[test]
    fn your_messages_switch_at_once_and_the_conversation_beside_shows_it() {
        let (mut v, rx, demo) = view(140, 40);
        run::press(&mut v, &rx, "<c-,><down><right>").unwrap();
        assert_eq!(demo.settings().messages, Messages::Irc);
        let screen = frame(&mut v);
        assert!(screen.contains("◉ on the left, as IRC") && screen.contains("You   "), "{screen}");
    }

    #[test]
    fn ctrl_comma_closes_it_from_the_theme_list_keeping_nothing_previewed() {
        let (mut v, rx, demo) = view(140, 40);
        run::press(&mut v, &rx, "<c-,>\n<down><down><c-,>").unwrap();
        assert!(v.settings_panel.is_none());
        assert_eq!(v.theme, Theme::with(true));
        assert_eq!(demo.settings().theme, "tokyo-night");
    }

    #[test]
    fn below_ninety_columns_the_panel_takes_the_screen() {
        let (mut v, rx, _) = view(80, 24);
        run::press(&mut v, &rx, "<c-,>").unwrap();
        let screen = frame(&mut v);
        let top = screen.lines().nth(1).unwrap();
        assert!(top.starts_with("╭ settings ─") && top.ends_with('╮'), "{screen}");
    }

    #[test]
    fn the_palette_finds_the_panel_and_its_settings_by_name() {
        let (mut v, rx, _) = view(140, 40);
        run::press(&mut v, &rx, "<c-k>theme").unwrap();
        assert!(frame(&mut v).contains("Change the theme"));
        run::press(&mut v, &rx, "<esc><c-k>irc").unwrap();
        assert!(frame(&mut v).contains("Your messages on the left, as IRC"));
        run::press(&mut v, &rx, "<esc><c-k>settings").unwrap();
        assert!(frame(&mut v).contains("ctrl-,"), "the palette teaches the key");
    }

    #[test]
    fn a_push_while_the_panel_is_open_changes_the_theme_it_shows_as_kept() {
        let (mut v, rx, _) = view(140, 40);
        run::press(&mut v, &rx, "<c-,>").unwrap();
        v.apply(Update::Settings(Settings { theme: "nord".into(), messages: Messages::Bubbles }));
        assert!(frame(&mut v).contains("Nord ›"));
        assert_eq!(v.theme, Theme::named("nord", true));
    }

    #[test]
    fn a_change_of_theme_redraws_the_screen_within_a_keypress() {
        let (mut v, _rx, _) = view(160, 45);
        frame(&mut v);
        let started = std::time::Instant::now();
        v.apply(Update::Settings(Settings { theme: "gruvbox".into(), messages: Messages::Irc }));
        frame(&mut v);
        let took = started.elapsed();
        assert!(took < std::time::Duration::from_millis(16), "{took:?}");
    }
}
```

Also add to `crates/jaw/src/view/tests.rs` a check that the keys sheet lists it:

```rust
#[test]
fn the_keys_sheet_lists_settings() {
    let (mut v, rx) = view(100, 30);
    press(&mut v, &rx, "?");
    assert!(frame(&mut v).contains("ctrl-,"));
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p jaw view::settings && cargo test -p jaw keys_sheet_lists_settings`
Expected: FAIL to compile: no field `settings_panel`.

- [ ] **Step 3: Write the panel**

Put this above the test module in `crates/jaw/src/view/settings.rs`:

```rust
//! The settings panel: the theme, and where your messages sit, in the chat list's place so the open
//! conversation shows each change on real messages. Moving through the themes previews each on
//! the whole screen, and Enter keeps one. Each change is asked of jawd, which keeps it for every
//! interface, and the screen follows what jawd says.

use super::View;
use crate::fmt;
use jaw_proto::settings::{Messages, SettingsSet};
use jaw_proto::themes::{self, Mode};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Padding;

/// The panel's rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Theme,
    Messages,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Panel {
    /// The row the keys are on.
    pub row: Row,
    /// While the theme list is open, the theme the keys are on, by its place in `themes::all()`,
    /// previewed on the whole screen.
    pub choosing: Option<usize>,
}

/// The strip of a theme's colours each row of the list shows.
const SWATCHES: usize = 5;

impl View {
    /// Opens the panel, on the theme list when `choosing`, closing the library and the quick look,
    /// whose places it takes.
    pub(super) fn open_settings(&mut self, choosing: bool) {
        self.close_library();
        self.close_quick_look();
        let choosing = choosing.then(|| self.theme_index());
        self.settings_panel = Some(Panel { row: Row::Theme, choosing });
        self.restyle();
    }

    /// Closes the panel, keeping nothing previewed.
    pub(super) fn close_settings(&mut self) {
        self.settings_panel = None;
        self.restyle();
    }

    /// The theme previewed while the list is open, by ID.
    pub(super) fn previewed(&self) -> Option<&'static str> {
        let at = self.settings_panel.as_ref()?.choosing?;
        themes::all().get(at).map(|t| t.id.as_str())
    }

    /// The place of the theme shown in `themes::all()`, the default's where it isn't one.
    fn theme_index(&self) -> usize {
        let id = &themes::or_default(&self.settings_shown().theme).id;
        themes::all().iter().position(|t| t.id == *id).unwrap_or(0)
    }

    pub(super) fn on_settings_key(&mut self, k: KeyEvent) {
        let Some(mut panel) = self.settings_panel.clone() else { return };
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char(',') {
            return self.close_settings();
        }
        let last = themes::all().len() - 1;
        match (panel.choosing, k.code) {
            (Some(at), KeyCode::Up | KeyCode::Char('k')) => panel.choosing = Some(at.saturating_sub(1)),
            (Some(at), KeyCode::Down | KeyCode::Char('j')) => panel.choosing = Some((at + 1).min(last)),
            (Some(at), KeyCode::PageUp) => panel.choosing = Some(at.saturating_sub(10)),
            (Some(at), KeyCode::PageDown) => panel.choosing = Some((at + 10).min(last)),
            (Some(_), KeyCode::Home) => panel.choosing = Some(0),
            (Some(_), KeyCode::End) => panel.choosing = Some(last),
            (Some(at), KeyCode::Enter) => {
                panel.choosing = None;
                self.settings_panel = Some(panel);
                let theme = themes::all()[at].id.clone();
                if theme != self.settings_shown().theme {
                    self.ask_settings(SettingsSet { theme: Some(theme), messages: None });
                }
                return self.restyle();
            }
            (Some(_), KeyCode::Esc) => panel.choosing = None,
            (None, KeyCode::Up | KeyCode::Char('k')) => panel.row = Row::Theme,
            (None, KeyCode::Down | KeyCode::Char('j')) => panel.row = Row::Messages,
            (None, KeyCode::Enter) if panel.row == Row::Theme => panel.choosing = Some(self.theme_index()),
            (None, KeyCode::Enter | KeyCode::Left | KeyCode::Right) if panel.row == Row::Messages => {
                let other = match self.settings_shown().messages {
                    Messages::Bubbles => Messages::Irc,
                    Messages::Irc => Messages::Bubbles,
                };
                self.settings_panel = Some(panel);
                return self.ask_settings(SettingsSet { theme: None, messages: Some(other) });
            }
            (None, KeyCode::Esc) => return self.close_settings(),
            _ => {}
        }
        self.settings_panel = Some(panel);
        self.restyle();
    }

    pub(super) fn settings_hints(&self) -> Vec<(&'static str, &'static str, bool)> {
        match &self.settings_panel {
            Some(Panel { choosing: Some(_), .. }) => {
                vec![("↑↓", "preview", true), ("⏎", "keeps it", true), ("esc", "goes back", true)]
            }
            Some(panel) => vec![
                ("↑↓", "move", true),
                ("⏎", "change", true),
                ("← →", "switch", panel.row == Row::Messages),
                ("esc", "closes", true),
            ],
            None => vec![],
        }
    }

    /// The panel in `area`, with the border of what has the keys: its rows, or the theme list.
    pub(super) fn draw_settings(&mut self, f: &mut Frame, area: Rect) {
        let Some(panel) = self.settings_panel.clone() else { return };
        let t = self.theme;
        let more = match panel.choosing {
            Some(_) => vec![Span::styled("› theme ", Style::new().fg(t.comment))],
            None => vec![],
        };
        let block = self.panel(t.focus).title(self.title("settings", more)).padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let (w, rows) = (inner.width as usize, inner.height as usize);
        let lines = match panel.choosing {
            Some(at) => self.theme_list(at, w, rows),
            None => self.settings_rows(&panel, w),
        };
        let buf = f.buffer_mut();
        for (i, (line, keys)) in lines.into_iter().take(rows).enumerate() {
            let y = inner.y + i as u16;
            buf.set_line(inner.x, y, &line, inner.width);
            if keys {
                let band = Rect::new(area.x + 1, y, area.width.saturating_sub(2), 1);
                buf.set_style(band, Style::new().bg(t.selection).add_modifier(Modifier::BOLD));
            }
        }
    }

    /// The panel's rows, each with whether the keys are on it.
    fn settings_rows(&self, panel: &Panel, w: usize) -> Vec<(Line<'static>, bool)> {
        let t = self.theme;
        let shown = self.settings_shown();
        let caption = |s: &str| (Line::from(Span::styled(s.to_string(), Style::new().fg(t.comment))), false);
        let mark = |row: Row| Span::styled(if panel.row == row { "▸ " } else { "  " }, Style::new().fg(t.focus));
        let name = format!("{} ›", themes::or_default(&shown.theme).name);
        let gap = w.saturating_sub(2 + "Theme".len() + fmt::width(&name));
        let mut out = vec![
            caption("Look"),
            (
                Line::from(vec![
                    mark(Row::Theme),
                    Span::styled("Theme", Style::new().fg(t.fg)),
                    Span::raw(" ".repeat(gap)),
                    Span::styled(name, Style::new().fg(t.text)),
                ]),
                panel.row == Row::Theme,
            ),
            (Line::default(), false),
            caption("Chat"),
            (
                Line::from(vec![mark(Row::Messages), Span::styled("Your messages", Style::new().fg(t.fg))]),
                panel.row == Row::Messages,
            ),
        ];
        for (messages, said) in [(Messages::Bubbles, "on the right"), (Messages::Irc, "on the left, as IRC")] {
            let on = shown.messages == messages;
            out.push((
                Line::from(vec![
                    Span::styled(if on { "    ◉ " } else { "    ○ " }, Style::new().fg(if on { t.focus } else { t.comment })),
                    Span::styled(said, Style::new().fg(if on { t.fg } else { t.muted })),
                ]),
                false,
            ));
        }
        out
    }

    /// As much of the theme list as `rows` holds, around `at`: each theme's name, its mode and a
    /// strip of its colours, the one the keys are on marked and the one kept ticked.
    fn theme_list(&self, at: usize, w: usize, rows: usize) -> Vec<(Line<'static>, bool)> {
        let t = self.theme;
        let all = themes::all();
        let kept = &self.settings_shown().theme;
        let top = at.saturating_sub(rows / 2).min(all.len().saturating_sub(rows));
        let name_w = w.saturating_sub(2 + 1 + 5 + 1 + SWATCHES);
        all.iter()
            .enumerate()
            .skip(top)
            .take(rows)
            .map(|(i, theme)| {
                let here = i == at;
                let mark = if here { "▸ " } else if theme.id == *kept { "✓ " } else { "  " };
                let name = fmt::fit(&theme.name, name_w);
                let pad = name_w.saturating_sub(fmt::width(&name));
                let mode = match theme.mode {
                    Mode::Dark => "dark ",
                    Mode::Light => "light",
                };
                let mut spans = vec![
                    Span::styled(mark, Style::new().fg(t.focus)),
                    Span::styled(name, Style::new().fg(if here { t.fg } else { t.text })),
                    Span::raw(" ".repeat(pad + 1)),
                    Span::styled(mode, Style::new().fg(t.comment)),
                    Span::raw(" "),
                ];
                let c = theme.colors;
                for colour in [c.background, c.accent, c.red, c.green, c.blue] {
                    spans.push(Span::styled("■", Style::new().fg(crate::theme::color(colour.0, t.truecolor))));
                }
                (Line::from(spans), here)
            })
            .collect()
    }
}
```

In `crates/jaw/src/view.rs`:

- Add to `View`, after `quick_look`: `/// The settings panel, while it's open.` and `settings_panel: Option<settings::Panel>,`, with `settings_panel: None,` in `View::new`.
- In `restyle`, replace its first line with `let id = self.previewed().map(str::to_string).unwrap_or_else(|| self.settings_shown().theme.clone());`.

- [ ] **Step 4: Give it the keys, its place and its hints**

In `crates/jaw/src/view/keys.rs`'s `on_key`, after the `if self.palette.is_some() { … return; }` block, add:

```rust
        if self.settings_panel.is_some() {
            self.on_settings_key(k);
            return;
        }
```

and after the Ctrl-K block:

```rust
        if ctrl && k.code == KeyCode::Char(',') && self.reach == super::Reach::Up && !self.linking() {
            self.open_settings(false);
            return;
        }
```

In `crates/jaw/src/view/draw.rs`'s `draw_main`, replace the branches after `if beside { … }` with:

```rust
        let settings = self.settings_panel.is_some();
        if rest.width < NARROW {
            if settings {
                self.draw_settings(f, rest);
            } else if self.picking || self.open.is_none() {
                self.draw_list(f, rest, true);
            } else {
                self.draw_conversation(f, rest);
            }
        } else if self.focused() && !settings {
            self.draw_conversation(f, rest);
        } else {
            let [list, conversation] = Layout::horizontal([Constraint::Length(LIST), Constraint::Min(20)]).areas(rest);
            if settings {
                self.draw_settings(f, list);
            } else {
                self.draw_list(f, list, false);
            }
            self.draw_conversation(f, conversation);
        }
```

In `hints`, after the `if self.palette.is_some() { … }` block:

```rust
        if self.settings_panel.is_some() {
            return self.settings_hints();
        }
```

In `draw_composer`, change `let typing = self.focus == Focus::Typing && self.overlay.is_none();` to `let typing = self.focus == Focus::Typing && self.overlay.is_none() && self.settings_panel.is_none();`, so the box gives up its focused border and its cursor while the panel has the keys. Do the same in `crates/jaw/src/view/composer.rs` wherever the spike's box decides it's typing (`rg "Focus::Typing" crates/jaw/src/view/composer.rs`).

In `draw_keys`, after the `pair(("ctrl-b", "the sidebar", true), Some(("ctrl-c", "quit", true))),` line, add:

```rust
            pair(("ctrl-,", "settings: theme, messages", true), None),
```

- [ ] **Step 5: Offer it in the palette**

In `crates/jaw/src/view/palette.rs`:

- Add to `enum Does`, after `Library`:

```rust
    /// Ctrl-,'s settings panel; on the theme list; your messages set as asked.
    Settings,
    Theme,
    Messages(jaw_proto::settings::Messages),
```

- In `actions`, before the `all.extend([ Action::new("New poll", …`, add:

```rust
        let shown = self.settings_shown();
        all.push(Action::new("Settings", "the theme, where your messages sit", Does::Settings).key("ctrl-,"));
        let theme = jaw_proto::themes::or_default(&shown.theme).name.clone();
        all.push(Action::new("Change the theme", format!("{theme} now; previewed as you move"), Does::Theme));
        all.push(match shown.messages {
            jaw_proto::settings::Messages::Bubbles => Action::new(
                "Your messages on the left, as IRC",
                "under \"You\", as everyone's are",
                Does::Messages(jaw_proto::settings::Messages::Irc),
            ),
            jaw_proto::settings::Messages::Irc => Action::new(
                "Your messages on the right",
                "a third of the way across",
                Does::Messages(jaw_proto::settings::Messages::Bubbles),
            ),
        });
```

- Where the palette carries out an action, beside `Does::Library => { … }`:

```rust
            Does::Settings => {
                close(self);
                self.open_settings(false);
            }
            Does::Theme => {
                close(self);
                self.open_settings(true);
            }
            Does::Messages(messages) => {
                close(self);
                self.ask_settings(jaw_proto::settings::SettingsSet { theme: None, messages: Some(messages) });
            }
```

`ask_settings`, `open_settings` and `settings_shown` are methods of `View`, reachable from the palette as the other actions' methods are; widen any that's private to `pub(super)`.

- [ ] **Step 6: Run the tests and re-pin what changed**

Run: `cargo test -p jaw`
Expected: the new tests pass. The keys sheet's new line changes the spike's `"?"` screen: if `with_every_switch_off_the_screen_is_as_it_was_byte_for_byte` reports it (or any other), re-pin that entry in `AS_IT_WAS` as in Task 5 step 6. If `a_change_of_theme_redraws_the_screen_within_a_keypress` fails under load, run it again alone (`cargo test -p jaw a_change_of_theme -- --test-threads=1`) and report its time; don't raise the 16 ms budget.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all
git add crates/jaw/src
git commit -m "The settings panel: Ctrl-, or Settings, Change the theme and Your messages on the left or right in the palette open it in the chat list's place, below 90 columns the whole screen, so the open conversation shows each change on real messages; Enter opens the list of Omarchy's 23, each with its mode and a strip of its colours, and moving through it previews each theme on the whole screen without asking jawd, Enter keeping it and Esc going back; your messages switch with Enter or the arrows, kept at once; Ctrl-, closes it from anywhere in it, keeping nothing previewed; the box gives up its border and cursor while the panel has the keys, the keys sheet lists Ctrl-,, and a change of theme redraws the screen within a keypress"
```

---

### Task 9: `jaw settings`

**Files:**
- Modify: `crates/jaw/src/main.rs`

**Interfaces:**
- Consumes: `connect()` in `main.rs`, `jaw_proto::settings::{Settings, SettingsKept, Messages}`, `jaw_proto::themes`.
- Produces: the subcommand `jaw settings [name=value …]`; `fn asked_settings(changes: &[String]) -> Result<serde_json::Value>`; `fn describe_settings(settings: &Settings) -> String`.

- [ ] **Step 1: Write the failing tests**

Add at the end of `crates/jaw/src/main.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_asked_as_name_value_pairs_for_jawd_to_check() {
        let asked = asked_settings(&["theme=gruvbox".into(), " messages = irc ".into()]).unwrap();
        assert_eq!(asked, serde_json::json!({"theme": "gruvbox", "messages": "irc"}));
        assert!(asked_settings(&["gruvbox".into()]).unwrap_err().to_string().contains("name=value"));
    }

    #[test]
    fn settings_are_said_with_the_values_each_takes() {
        let said = describe_settings(&jaw_proto::settings::Settings::default());
        assert!(said.starts_with("theme     tokyo-night (Tokyo Night)\n"), "{said}");
        assert!(said.contains("catppuccin, catppuccin-latte, ethereal"), "{said}");
        assert!(said.contains("messages  bubbles\n          one of bubbles, irc"), "{said}");
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p jaw --bin jaw settings_are`
Expected: FAIL to compile: no `asked_settings`.

- [ ] **Step 3: Write the command**

In `crates/jaw/src/main.rs`, add to `enum Command`, after `Link`:

```rust
    /// The terminal's settings, or changes them: jaw settings theme=gruvbox messages=irc
    Settings {
        /// Settings to change, each as name=value
        changes: Vec<String>,
    },
```

add `Some(Command::Settings { changes }) => settings(changes),` to `main`'s match, after the `Link` arm, and after `fn link`:

```rust
fn settings(changes: Vec<String>) -> Result<()> {
    let (mut connection, welcome) = connect()?;
    let now = if changes.is_empty() {
        welcome.settings.unwrap_or_default()
    } else {
        let kept = connection.call("settings.set", asked_settings(&changes)?)?;
        serde_json::from_value::<jaw_proto::settings::SettingsKept>(kept)?.settings
    };
    println!("{}", describe_settings(&now));
    Ok(())
}

/// `name=value` pairs as `settings.set`'s params; jawd checks the names and the values.
fn asked_settings(changes: &[String]) -> Result<serde_json::Value> {
    let mut asked = serde_json::Map::new();
    for change in changes {
        let Some((name, value)) = change.split_once('=') else {
            bail!("{change} isn't a name=value pair, such as theme=gruvbox");
        };
        asked.insert(name.trim().to_string(), serde_json::Value::from(value.trim()));
    }
    Ok(serde_json::Value::Object(asked))
}

/// The settings as `jaw settings` prints them, each with the values it takes.
fn describe_settings(settings: &jaw_proto::settings::Settings) -> String {
    let themes: Vec<&str> = jaw_proto::themes::all().iter().map(|t| t.id.as_str()).collect();
    let messages = jaw_proto::settings::Messages::ALL.map(|m| m.name());
    format!(
        "theme     {} ({})\n          one of {}\nmessages  {}\n          one of {}",
        settings.theme,
        jaw_proto::themes::or_default(&settings.theme).name,
        themes.join(", "),
        settings.messages.name(),
        messages.join(", "),
    )
}
```

- [ ] **Step 4: Run the tests, then try it on a demo jawd**

Run: `cargo test -p jaw --bin jaw settings_are`
Expected: 2 passed.

Then, on a folder of its own, with the demo engine (never the owner's folder):

```bash
export JAW_TRY=$(mktemp -d) && cargo build -p jawd -p jaw
JAW_HOME=$JAW_TRY target/debug/jawd --engine demo & echo $! > $JAW_TRY/pid; sleep 2
JAW_HOME=$JAW_TRY target/debug/jaw settings
JAW_HOME=$JAW_TRY target/debug/jaw settings theme=nord messages=irc
JAW_HOME=$JAW_TRY target/debug/jaw settings theme=solarized; echo "exit $?"
JAW_HOME=$JAW_TRY target/debug/jaw status | grep settings
kill $(cat $JAW_TRY/pid)
```

Expected: the defaults, then nord and irc, then jawd's "theme takes one of …" with `exit 1`, then `settings  jaw: messages irc, theme nord`. (Check `target/debug/jawd --help` for the engine flag's exact spelling first.) Stop only the PID started here.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/jaw/src/main.rs
git commit -m "jaw settings shows the terminal's settings, its own domain's since it says hello as jaw, with the values each takes, and jaw settings theme=nord messages=irc changes them through settings.set and prints them as they now are, a setting or value jawd refuses printing its reason and exiting with 1, so the owner's dot theme can keep jaw in step with Ghostty and Neovim"
```

---

### Task 10: The docs

**Files:**
- Modify: `protocol/README.md`, `docs/plans/2026-10-16-jaw-design.md`, `docs/plans/2026-10-26-settings-design.md`, `README.md`

- [ ] **Step 1: The protocol**

In `protocol/README.md`:

- Add `,"settings":{"theme":"tokyo-night","messages":"bubbles"}` to the end of the hello result example, inside `result`, and a bullet after `connection.state`: `- \`settings\`: the settings of the connection's own domain, the one its \`client\` names, with the values in effect, as "Settings" says; absent for a client jawd keeps no settings for.`
- Add a section before "## Focus and reading":

````markdown
## Settings

Each frontend's settings, kept by jawd in jaw.db in a domain of its own, named by the `client` its hello gives, so a terminal and a Mac app on one machine keep separate preferences. A connection reads its own domain's settings in hello's answer, changes them with `settings.set` and is pushed each change to them. The terminal's domain, `jaw`, is the only one jawd keeps so far; `jaw settings` and `jaw status` share it, since they say hello as `jaw` too.

```json
{"id":20,"method":"settings.set","params":{"theme":"gruvbox"}}
{"id":20,"result":{"settings":{"theme":"gruvbox","messages":"bubbles"}}}
{"method":"settings","params":{"theme":"gruvbox","messages":"bubbles"}}
{"id":21,"method":"themes"}
{"id":21,"result":{"themes":[{"id":"catppuccin","name":"Catppuccin","mode":"dark","colors":{"accent":"#89b4fa","background":"#1e1e2e","…":"…"}}]}}
```

- **In the terminal's domain,** **`theme`** is one of Omarchy's themes by its ID, as `themes` lists them; `tokyo-night` by default. **`messages`** is where your messages sit: `bubbles`, in a column from a third of the way across, by default, or `irc`, on the left under "You" as everyone's are.
- **`settings.set`** changes the connection's own domain; a connection whose client has no domain is error 3, with `data` `{"client": "<client>"}`. It takes one setting or more and checks every one before changing any: a setting jawd doesn't know is -32602 with `data` `{"param": "<setting>"}`, a value it doesn't take or of the wrong type is -32602 with `data` `{"param": "<setting>", "allowed": [...]}`, and no settings at all is -32602. It answers with `settings`, all of them as they now are, once the change is on disk, synced; the value jaw.db already holds for a setting is answered at once and writes nothing. While jaw.db can't be written for a reason that passes, such as a full disk, the answer waits as every write's does; a change that can't be kept at all is -32603, and nothing changes.
- **The `settings` push** goes to every connection of the same domain that has said hello, the one that asked included, whenever that domain's settings change, and carries all of them. A connection always ends on the latest: a change made while it's slow to read may be skipped, never the last. Connections of other domains hear nothing of it.
- **`themes`** answers with every theme, ordered by ID: its `id`, a `name` to show, its `mode` (`dark` or `light`) and `colors`, Omarchy's 25 colours by their keys as lowercase `#rrggbb`; where a theme leaves them out, `orange` is its `yellow` and `brown` its `orange` mixed half with black, as Omarchy resolves them. They're protocol/themes/, each theme's colors.toml as Omarchy wrote it (MIT, docs/licenses/omarchy.txt), which an interface in any language can read as well.
- **`status`** says each domain's settings in effect, by the domain's name, and any whose value in jaw.db this jawd doesn't take, which has its default instead.
````

- In the Methods table, add after the `status` row:

```markdown
| `settings.set` | any of `theme`, `messages` | `settings`, all of them, once kept. See Settings |
| `themes` | | `themes`: every theme with its colours. See Settings |
```

- In the Other pushes table, add: `| \`settings\` | its domain's settings, as hello's \`settings\` |`.
- In the `status` row of the Methods table, add `, and \`settings\`, each domain's settings in effect by its name, with \`settings_ignored\`, any jawd doesn't take` before the end of its result cell.

- [ ] **Step 2: The design**

In `docs/plans/2026-10-16-jaw-design.md`, add under "## jawd", after "### Lifecycle and notifications":

```markdown
### Settings

- **A domain of settings for each frontend,** named by the client its hello gives, kept by jawd as jaw's own in the `settings` table, so a terminal and a Mac app on one machine keep separate preferences. The terminal's are the theme, one of Omarchy's, and where your messages sit. Hello's answer carries a connection's own, `settings.set` changes them with a synced commit before it answers, and each of the domain's connections is pushed each change. docs/plans/2026-10-26-settings-design.md has the whole design.
- **Omarchy's themes, as data:** protocol/themes/ holds each theme's colors.toml as Omarchy wrote it, served with `themes`; each interface maps the palette onto what it draws, keeping every role readable.
```

Under "## The terminal interface", add a bullet after "The library":

```markdown
- **Settings:** Ctrl-, opens a panel in the chat list's place, the open conversation showing each change: the theme, each previewed on the whole screen as you move through the list, and your messages on the right or, as IRC clients put them, on the left under "You". `jaw settings` reads and changes them from the command line.
```

and in its "Keys" bullet, after "Ctrl-B shows the sidebar, and Tab moves between folders and roster.", add " Ctrl-, opens settings."

- [ ] **Step 3: The spec's correction, and the README**

In `docs/plans/2026-10-26-settings-design.md`, replace "If jaw.db can't be written, it's -32603 saying why, and nothing changes." with "While jaw.db can't be written for a reason that passes, such as a full disk, the answer waits as every write's does; a change that can't be kept at all is -32603 saying why, and nothing changes." Replace "- **A jaw.db that can't be written** publishes nothing, and the answer says why." with "- **A change that can't be kept** publishes nothing, and the answer says why; one waiting for a jaw.db that can't be written yet waits, as every write does." In its Testing section, replace "A jaw.db that can't be written publishes nothing and answers with why." with "A change that can't be kept publishes nothing and answers with why."

In `README.md`, add after the paragraph that starts "jaw is being built.":

```markdown
Its themes are [Omarchy's](https://github.com/omacom/omarchy), as Omarchy wrote them, MIT-licensed ([docs/licenses/omarchy.txt](docs/licenses/omarchy.txt)); Ctrl-, or `jaw settings` picks one.
```

- [ ] **Step 4: Commit**

```bash
git add protocol/README.md docs/plans/2026-10-16-jaw-design.md docs/plans/2026-10-26-settings-design.md README.md
git commit -m "The settings in the protocol, the design and the README: settings.set, the settings push, themes and the settings in hello's answer and status as they're served, Ctrl-, and jaw settings among the terminal's keys and commands, Omarchy credited with its licence, and the spec's account of a jaw.db that can't be written corrected to what the writer does, a change waiting while the trouble passes and one that can't be kept at all refused"
```

---

### Task 11: The gate, the audit and the measurements

**Files:**
- Create: `docs/audits/2026-10-26-settings.md` (written by the database expert)
- Modify: whatever the audit asks for

- [ ] **Step 1: Catch up with main**

Run: `git fetch origin && git rebase origin/main`
If `migrations.lock` conflicts because main gained migration 11, renumber ours to the next free number: move its text after theirs in `MIGRATIONS`, give its lock line that number (its hash doesn't change), update the comment above it, and continue the rebase.

- [ ] **Step 2: The gate**

Run: `make test && make lint && make check-generated`
Expected: everything passes. Fix anything that doesn't in the task it belongs to, with a commit of its own.

- [ ] **Step 3: The budgets**

Build release binaries on this branch and on `origin/main` (in a scratch worktree at `/tmp/jaw-settings-main`, removed afterwards), and run `jaw bench --first --jawd` on each, five times, reporting the medians: the first frame must stay within 50 ms. Run `jaw bench` (keypresses during a flood) on this branch with the panel closed: within 16 ms p99. Record the numbers for the audit and the final report.

- [ ] **Step 4: The audit**

Launch the `database-expert` subagent with the worktree path `/Users/pedrogomes/src/jaw-settings` and the range `origin/main..HEAD`, asking it to audit the settings branch against the data-integrity rules and write `docs/audits/2026-10-26-settings.md`. Fix everything it marks as data loss or inconsistency, each with a test that fails without the fix, then launch a fresh auditor for the next look. Repeat until the note ends `Verdict: approved`.

- [ ] **Step 5: The trailers**

Write the note's commit message to `/tmp/jaw-settings-audit-message.txt` first, in the project's voice, as the earlier audits' commits read (`git log --grep "database expert's audit" -3`): how many looks it took, what each found, what was done about it and what's left as follow-ups. Commit the note, then give every commit that touches the data layer its trailer:

```bash
git add docs/audits/2026-10-26-settings.md
git commit -F /tmp/jaw-settings-audit-message.txt
git rebase origin/main -x 'python3 scripts/check-data-audit.py HEAD~1 HEAD >/dev/null 2>&1 || git commit --amend --no-edit --trailer "Data-Audit: docs/audits/2026-10-26-settings.md"'
make check-audit
```

Expected: `make check-audit` passes.

- [ ] **Step 6: Report, and land only when told**

Report to the owner: what was built, the measurements, the audit's verdict and follow-ups, and the commits. Landing is a push to `main` from this worktree, which the pre-push hook checks; do it only once the owner says to.
