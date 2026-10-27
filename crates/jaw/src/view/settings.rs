//! The settings panel: the theme, and where your messages sit, in the chat list's place so the open
//! conversation shows each change on real messages. Moving through the themes previews each on
//! the whole screen, and Enter keeps one. Each change is asked of jawd, which keeps the terminal's
//! settings for every jaw, and the screen follows what jawd says.

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

/// The panel, while it's open.
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
    /// strip of its colours, the one the keys are on marked and the one kept ticked.
    fn theme_list(&self, at: usize, w: usize, rows: usize) -> Vec<(Line<'static>, bool)> {
        let t = self.theme;
        let all = themes::all();
        let kept = &themes::or_default(&self.settings_shown().theme).id;
        let top = at.saturating_sub(rows / 2).min(all.len().saturating_sub(rows));
        let name_w = w.saturating_sub(2 + 1 + 5 + 1 + SWATCHES);
        all.iter()
            .enumerate()
            .skip(top)
            .take(rows)
            .map(|(i, theme)| {
                let here = i == at;
                let mark = if here {
                    "▸ "
                } else if theme.id == *kept {
                    "✓ "
                } else {
                    "  "
                };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Update;
    use crate::demo::{Clock, Demo, State};
    use crate::experiments::Experiments;
    use crate::run::{self, Event};
    use crate::theme::Theme;
    use crate::view::{Focus, Scroll};
    use jaw_proto::settings::Settings;
    use jaw_proto::{Connection, ConnectionState};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::crossterm::event::{Event as Input, MouseButton, MouseEvent, MouseEventKind};
    use std::sync::mpsc::Receiver;
    use std::time::{Duration, Instant};

    /// A view on the made-up account with the switches `on` names, and the account, to ask it what
    /// it keeps.
    fn view_with(on: &str, width: u16, height: u16) -> (View, Receiver<Event>, Demo) {
        let (tx, rx) = std::sync::mpsc::channel();
        let demo = Demo::start(tx, Clock::Fixed(crate::view::tests::NOW), State::Online, false);
        let mut v = View::new(Box::new(demo.clone()));
        v.experiments = Experiments::parse(on);
        v.clock = Some(crate::view::tests::NOW);
        v.tz = chrono::FixedOffset::east_opt(0).unwrap();
        v.theme = Theme::with(true);
        v.no_color = false;
        v.resize(width, height);
        run::settle(&mut v, &rx);
        (v, rx, demo)
    }

    fn view(width: u16, height: u16) -> (View, Receiver<Event>, Demo) {
        view_with("", width, height)
    }

    fn frame(v: &mut View) -> String {
        let (w, h) = v.size().unwrap();
        run::snapshot(v, w, h)
    }

    /// A frame's cells, each with its colours.
    fn drawn(v: &mut View) -> Buffer {
        let (w, h) = v.size().unwrap();
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| v.draw(f)).unwrap();
        term.backend().buffer().clone()
    }

    /// A key as the terminal sends it, with nothing it brings applied yet.
    fn key(code: KeyCode, modifiers: KeyModifiers) -> Input {
        Input::Key(KeyEvent::new(code, modifiers))
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
    fn in_the_focused_mode_the_panel_unfolds_into_the_chat_lists_place_while_its_open() {
        let (mut v, rx, _) = view_with("focus", 160, 45);
        run::press(&mut v, &rx, "<c-f><c-,>").unwrap();
        let screen = frame(&mut v);
        let top = screen.lines().nth(1).unwrap();
        assert!(top.starts_with("╭ settings ─") && top.contains("╮╭ Ana Ribeiro"), "{screen}");
        assert!(!screen.contains("╭ folders") && !screen.contains("╭ chats"), "the rest stays folded:\n{screen}");
        run::press(&mut v, &rx, "<c-,>").unwrap();
        let screen = frame(&mut v);
        assert!(screen.lines().nth(1).unwrap().starts_with("╭ Ana Ribeiro"), "closed, focused again:\n{screen}");
    }

    #[test]
    fn with_two_panes_the_panel_takes_the_lists_place_and_the_panes_beside_it_show_the_preview() {
        let (mut v, rx, _) = view_with("panes", 160, 45);
        run::press(&mut v, &rx, "<c-p>%<c-,>").unwrap();
        let screen = frame(&mut v);
        let top = screen.lines().nth(1).unwrap();
        assert!(top.starts_with("╭ folders ─") && top.contains("╮╭ settings ─"), "{screen}");
        assert!(top.contains("╮╭ 1 Ana Ribeiro") && top.contains("╮╭ 2  Mãe ─"), "both panes beside it:\n{screen}");
        assert!(!screen.contains("╭ chats"), "{screen}");
        run::press(&mut v, &rx, "\n<home><down><down><down><down><down>").unwrap();
        let gruvbox = Settings { theme: "gruvbox".into(), messages: Messages::Bubbles };
        let t = Theme::named(&gruvbox.theme, true);
        assert_eq!(v.theme, t, "Gruvbox, previewed");
        let now = drawn(&mut v);
        assert!(!now.content.iter().any(|c| c.bg == Theme::with(true).bg), "nothing on Tokyo Night's ground");
        let corners = (now[(26, 1)].fg, now[(110, 1)].fg);
        assert_eq!(corners, (t.focus, t.gutter), "the panel's border is the focused one, not Mãe's pane's");
        let (mut all_along, rx, _) = view_with("panes", 160, 45);
        all_along.apply(Update::Settings(gruvbox));
        run::press(&mut all_along, &rx, "<c-p>%").unwrap();
        let was = drawn(&mut all_along);
        let mut parked = (60..110u16).flat_map(|x| (1..44u16).map(move |y| (x, y)));
        let differ = parked.find(|&(x, y)| now[(x, y)] != was[(x, y)]);
        assert_eq!(differ, None, "Ana's pane, without the keys, as if Gruvbox had been kept all along");
        assert!(frame(&mut v).contains("vens jantar sexta?"), "with her messages in it");
    }

    #[test]
    fn while_the_panel_has_the_keys_only_its_border_is_the_focused_one_and_the_box_has_no_cursor() {
        let (mut v, rx, _) = view(140, 40);
        let t = Theme::with(true);
        // The borders of the chat list's column, the conversation and the box, and the cursor.
        let look = |v: &mut View| {
            let mut term = Terminal::new(TestBackend::new(140, 40)).unwrap();
            term.draw(|f| v.draw(f)).unwrap();
            let (buf, b) = (term.backend().buffer().clone(), v.areas.composer);
            let borders = (buf[(26, 1)].fg, buf[(60, 1)].fg, buf[(b.x - 2, b.y - 1)].fg);
            (borders, term.backend().cursor_visible())
        };
        assert_eq!(look(&mut v), ((t.gutter, t.gutter, t.focus), true), "typing, the box has them");
        run::press(&mut v, &rx, "<c-,>").unwrap();
        assert_eq!(look(&mut v), ((t.focus, t.gutter, t.gutter), false), "the panel, and only the panel");
        run::press(&mut v, &rx, "<c-,><up>").unwrap();
        assert_eq!(look(&mut v), ((t.gutter, t.focus, t.gutter), false), "a message selected, the conversation");
        run::press(&mut v, &rx, "<c-,>").unwrap();
        assert_eq!(look(&mut v), ((t.focus, t.gutter, t.gutter), false), "the panel again");

        let (mut v, rx, _) = view_with("composer", 160, 45);
        run::press(&mut v, &rx, "<c-,>").unwrap();
        let mut term = Terminal::new(TestBackend::new(160, 45)).unwrap();
        term.draw(|f| v.draw(f)).unwrap();
        let corner = term.backend().buffer()[(0, v.areas.composer.y - 1)].clone();
        assert_eq!((corner.symbol(), corner.fg), ("╭", t.gutter), "the box along the bottom gives it up too");
        assert!(!term.backend().cursor_visible(), "and its cursor");
    }

    #[test]
    fn the_panel_takes_the_keys_from_a_sidebar_that_would_be_drawn_over_it() {
        let (mut v, rx, _) = view(100, 30);
        run::press(&mut v, &rx, "<c-b>").unwrap();
        assert!(frame(&mut v).lines().nth(1).unwrap().starts_with("╭ folders ─"), "over the chat list");
        run::press(&mut v, &rx, "<c-,>").unwrap();
        let screen = frame(&mut v);
        assert!(
            screen.lines().nth(1).unwrap().starts_with("╭ settings ─") && !screen.contains("╭ folders"),
            "{screen}"
        );
        run::press(&mut v, &rx, "<c-,>").unwrap();
        assert!(frame(&mut v).lines().nth(1).unwrap().starts_with("╭ chats · All ─"), "and keeps them once it closes");
    }

    #[test]
    fn a_theme_jaw_doesnt_know_shows_in_the_panel_as_tokyo_night_which_it_is_drawn_in() {
        let (mut v, rx, _) = view(140, 40);
        v.apply(Update::Settings(Settings { theme: "solarized".into(), messages: Messages::Bubbles }));
        run::press(&mut v, &rx, "<c-,>").unwrap();
        assert!(frame(&mut v).contains("Tokyo Night ›"));
        run::press(&mut v, &rx, "\n<down>").unwrap();
        let screen = frame(&mut v);
        assert!(screen.contains("✓ Tokyo Night") && screen.contains("▸ Vantablack"), "{screen}");
    }

    #[test]
    fn the_panel_closes_with_the_screen_it_was_on_once_the_account_is_no_longer_linked() {
        let logged_out = Connection::new(ConnectionState::LoggedOut);
        let unlinked = Update::Hello {
            root: String::new(),
            linked: false,
            me: vec![],
            connection: Connection::new(ConnectionState::Offline),
            settings: Settings::default(),
        };
        for (update, said) in
            [(Update::Connection(logged_out), "⏎ link again"), (unlinked, "p link with a phone number")]
        {
            let (mut v, rx, _) = view(140, 40);
            run::press(&mut v, &rx, "<c-,>\n<down>").unwrap();
            v.apply(update);
            assert!(v.settings_panel.is_none(), "{said}");
            assert_eq!(v.theme, Theme::with(true), "{said}: nothing previewed");
            let screen = frame(&mut v);
            assert!(screen.lines().last().unwrap().contains(said), "the linking screen has the keys:\n{screen}");
        }
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
    fn a_push_while_a_theme_is_previewed_leaves_the_preview_on_screen_and_esc_returns_to_the_new_one() {
        let (mut v, rx, _) = view(140, 40);
        run::press(&mut v, &rx, "<c-,>\n<down>").unwrap();
        v.apply(Update::Settings(Settings { theme: "nord".into(), messages: Messages::Bubbles }));
        assert_eq!(v.theme, Theme::named("vantablack", true), "the preview goes on");
        let screen = frame(&mut v);
        assert!(
            screen.contains("✓ Nord") && screen.contains("▸ Vantablack"),
            "the one kept is the one pushed:\n{screen}"
        );
        run::press(&mut v, &rx, "<esc>").unwrap();
        assert_eq!(v.theme, Theme::named("nord", true), "Esc goes back to the theme pushed");
        assert!(frame(&mut v).contains("Nord ›"));
    }

    #[test]
    fn a_change_jawd_doesnt_keep_says_why_and_the_panel_goes_back_to_what_jawd_has() {
        let (mut v, rx, _) = view(140, 40);
        // What jawd answers each change, put aside so the refusal can come in its place.
        let answers = |rx: &Receiver<Event>| {
            let came: Vec<Event> = rx.try_iter().collect();
            came.iter().filter(|e| matches!(e, Event::Update(Update::SettingsKept(_)))).count()
        };
        run::press(&mut v, &rx, "<c-,>\n<down>").unwrap();
        v.on_input(key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(v.theme, Theme::named("vantablack", true), "shown while jawd keeps it");
        assert_eq!(answers(&rx), 1, "Enter asks jawd once");
        v.apply(Update::SettingsRefused("jaw.db can't be written".into()));
        let screen = frame(&mut v);
        assert!(screen.lines().last().unwrap().contains("not kept: jaw.db can't be written"), "{screen}");
        assert!(screen.contains("╭ settings ─") && screen.contains("Tokyo Night ›"), "{screen}");
        assert_eq!(v.theme, Theme::with(true), "back to what jawd has");

        run::press(&mut v, &rx, "<down>").unwrap();
        v.on_input(key(KeyCode::Left, KeyModifiers::NONE));
        assert!(frame(&mut v).contains("◉ on the left, as IRC"), "shown while jawd keeps it");
        assert_eq!(answers(&rx), 1, "← asks jawd once");
        v.apply(Update::SettingsRefused("jaw.db can't be written".into()));
        let screen = frame(&mut v);
        assert!(screen.contains("◉ on the right") && !screen.contains("You   "), "back to what jawd has:\n{screen}");
    }

    #[test]
    fn while_the_panel_has_the_keys_a_paste_a_click_and_the_wheel_do_nothing() {
        let (mut v, rx, _) = view(140, 40);
        frame(&mut v);
        let (list, (row, _)) = (v.areas.list, v.areas.chat_rows[1].clone());
        v.on_input(key(KeyCode::Char(','), KeyModifiers::CONTROL));
        let mouse = |kind, column, row| Input::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE });
        let click = MouseEventKind::Down(MouseButton::Left);
        v.on_input(mouse(click, list.x + 2, row));
        v.on_input(mouse(MouseEventKind::ScrollDown, list.x + 2, row));
        assert_eq!((v.open.as_deref(), v.list_top), (Some("ana"), 0), "the chat list under the panel takes nothing");
        frame(&mut v);
        let (messages, (top, _, _)) = (v.areas.messages, v.areas.message_rows[0].clone());
        v.on_input(mouse(click, messages.x + 4, top));
        v.on_input(mouse(MouseEventKind::ScrollUp, messages.x + 4, top));
        v.on_input(Input::Paste("vens jantar?".into()));
        assert_eq!((&v.focus, &v.scroll), (&Focus::Typing, &Scroll::Bottom), "nor does the conversation beside it");
        assert!(v.composer.is_empty(), "and the box, without its cursor, takes no paste");
        run::settle(&mut v, &rx);
        assert!(v.settings_panel.is_some() && frame(&mut v).contains("╭ settings ─"));
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

    #[test]
    fn a_key_in_the_theme_list_previews_the_next_theme_within_a_keypress() {
        let (mut v, rx, _) = view(160, 45);
        run::press(&mut v, &rx, "<c-,>\n").unwrap();
        let started = Instant::now();
        v.on_input(key(KeyCode::Down, KeyModifiers::NONE));
        frame(&mut v);
        let took = started.elapsed();
        assert_eq!(v.theme, Theme::named("vantablack", true));
        assert!(took < Duration::from_millis(16), "{took:?}");
    }
}
