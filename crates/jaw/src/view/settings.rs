//! The settings panel: the theme, and where your messages sit, in the chat list's place so the open
//! conversation shows each change on real messages. Moving through the themes previews each on
//! the whole screen, and Enter keeps one. Each change is asked of jawd, which keeps the terminal's
//! settings for every jaw, and the screen follows what jawd says.

use super::View;
use crate::fmt;
use jaw_proto::settings::{Messages, SettingsSet};
use jaw_proto::themes::{self, Colors, Mode, Rgb};
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

/// The panel, while it's open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Panel {
    /// The row the keys are on.
    pub row: Row,
    /// While the theme list is open, the theme the keys are on, by its place in `themes::all()`,
    /// previewed on the whole screen.
    pub choosing: Option<usize>,
}

/// The strip of a theme's colours each row of the list shows, a cell to a colour.
const SWATCHES: usize = 5;

/// The colours in a theme's strip: its ground, its accent, red, green and blue.
fn strip(c: &Colors) -> [Rgb; SWATCHES] {
    [c.background, c.accent, c.red, c.green, c.blue]
}

impl View {
    /// Opens the panel, on the theme list when `choosing`. It takes the place of the library and
    /// the quick look, which close, and the keys from the sidebar, which would be drawn over it.
    pub(super) fn open_settings(&mut self, choosing: bool) {
        self.close_library();
        self.close_quick_look();
        self.sidebar.keys = None;
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

    /// Keys while the panel has them; Ctrl-, closes it from anywhere in it.
    pub(super) fn on_settings_key(&mut self, k: KeyEvent) {
        let Some(mut panel) = self.settings_panel.clone() else { return };
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char(',') {
            return self.close_settings();
        }
        let code = match k.code {
            KeyCode::Char('k') if !ctrl => KeyCode::Up,
            KeyCode::Char('j') if !ctrl => KeyCode::Down,
            code => code,
        };
        let last = themes::all().len() - 1;
        match (panel.choosing, code) {
            (Some(at), KeyCode::Up) => panel.choosing = Some(at.saturating_sub(1)),
            (Some(at), KeyCode::Down) => panel.choosing = Some((at + 1).min(last)),
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
            (None, KeyCode::Up) => panel.row = Row::Theme,
            (None, KeyCode::Down) => panel.row = Row::Messages,
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

    /// The footer's keys while the panel is open.
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
                    Span::styled(
                        if on { "    ◉ " } else { "    ○ " },
                        Style::new().fg(if on { t.focus } else { t.comment }),
                    ),
                    Span::styled(said, Style::new().fg(if on { t.fg } else { t.muted })),
                ]),
                false,
            ));
        }
        out
    }

    /// As much of the theme list as `rows` holds, around `at`: each theme's name, its mode and a
    /// strip of its colours, the one the keys are on marked in front and the one kept ticked after
    /// its name, so one row can show both.
    fn theme_list(&self, at: usize, w: usize, rows: usize) -> Vec<(Line<'static>, bool)> {
        let t = self.theme;
        let all = themes::all();
        let kept = &themes::or_default(&self.settings_shown().theme).id;
        let top = at.saturating_sub(rows / 2).min(all.len().saturating_sub(rows));
        // A row: the mark, the name with its tick, then the mode and the strip, each after a space.
        let name_w = w.saturating_sub(2 + 1 + 5 + 1 + SWATCHES);
        all.iter()
            .enumerate()
            .skip(top)
            .take(rows)
            .map(|(i, theme)| {
                let here = i == at;
                let tick = if theme.id == *kept { " ✓" } else { "" };
                let name = fmt::fit(&theme.name, name_w.saturating_sub(fmt::width(tick)));
                let pad = name_w.saturating_sub(fmt::width(&name) + fmt::width(tick));
                let mode = match theme.mode {
                    Mode::Dark => "dark ",
                    Mode::Light => "light",
                };
                let mut spans = vec![
                    Span::styled(if here { "▸ " } else { "  " }, Style::new().fg(t.focus)),
                    Span::styled(name, Style::new().fg(if here { t.fg } else { t.text })),
                    Span::styled(tick, Style::new().fg(t.focus)),
                    Span::raw(" ".repeat(pad + 1)),
                    Span::styled(mode, Style::new().fg(t.comment)),
                    Span::raw(" "),
                ];
                let swatch = |c: Rgb| Span::styled("■", Style::new().fg(crate::theme::color(c.0, t.truecolor)));
                spans.extend(strip(&theme.colors).map(swatch));
                (Line::from(spans), here)
            })
            .collect()
    }
}
