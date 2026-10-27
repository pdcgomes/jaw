//! The settings panel: Ctrl-, and the palette open it in the chat list's place, beside the
//! conversation and its panes; the theme list previews each theme on the whole screen and keeps
//! one; your messages switch at once; what jawd keeps, refuses or pushes shows in it; and while it
//! has the keys, nothing else has the focused border, the mouse or a paste.

use super::panes_tests::drawn;
use super::settings::{Panel, Row};
use super::spike_tests::{frame, press, view_with};
use super::tests::{NOW, came};
use super::*;
use crate::demo::{Clock, Demo, State};
use crate::run::{self, Event};
use jaw_proto::settings::Messages;
use jaw_proto::themes;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    Event as Input, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::style::Color;
use std::sync::mpsc::Receiver;

fn view(width: u16, height: u16) -> (View, Receiver<Event>, Demo) {
    view_with("", State::Online, width, height)
}

/// A key as the terminal sends it, with nothing it brings applied yet.
fn key(code: KeyCode, modifiers: KeyModifiers) -> Input {
    Input::Key(KeyEvent::new(code, modifiers))
}

/// Applies what jawd has answered and pushed since it was last asked, saying how many changes it
/// answered.
fn answered(v: &mut View, rx: &Receiver<Event>) -> usize {
    let updates = came(rx);
    let n = updates.iter().filter(|u| matches!(u, Update::SettingsKept(_) | Update::SettingsRefused(_))).count();
    updates.into_iter().for_each(|u| v.apply(u));
    n
}

/// The cell where `text` starts on the screen, a character to a cell.
fn find(buf: &Buffer, text: &str) -> (u16, u16) {
    let want: Vec<char> = text.chars().collect();
    let area = buf.area;
    let starts = |x: u16, y: u16| {
        let same = want.iter().zip(x..area.width).filter(|(c, x)| buf[(*x, y)].symbol().chars().eq([**c]));
        same.count() == want.len()
    };
    (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .find(|&(x, y)| starts(x, y))
        .unwrap_or_else(|| panic!("{text:?} isn't on screen"))
}

#[test]
fn ctrl_comma_opens_the_panel_in_the_chat_lists_place_and_closes_it_again() {
    let (mut v, rx, _) = view(140, 40);
    press(&mut v, &rx, "<c-,>");
    let screen = frame(&mut v);
    assert!(screen.contains("╭ settings ─") && !screen.contains("╭ chats · All ─"), "{screen}");
    assert!(screen.contains("Theme") && screen.contains("Tokyo Night ›"), "{screen}");
    assert!(screen.contains("◉ on the right") && screen.contains("○ on the left, as IRC"), "{screen}");
    assert!(screen.contains("╭ Ana Ribeiro"), "the conversation stays beside it:\n{screen}");
    assert!(screen.lines().last().unwrap().contains("esc closes"), "{screen}");
    press(&mut v, &rx, "<c-,>");
    assert!(frame(&mut v).contains("╭ chats · All ─"));
}

#[test]
fn moving_through_the_themes_previews_each_esc_goes_back_and_enter_keeps_one() {
    let (mut v, rx, demo) = view(140, 40);
    press(&mut v, &rx, "<c-,>\n");
    assert!(frame(&mut v).contains("› theme"), "Enter opens the theme list");
    press(&mut v, &rx, "<down>");
    assert_eq!(v.theme, Theme::named("vantablack", true), "the theme under the keys is previewed");
    assert_eq!(demo.settings().theme, "tokyo-night", "and nothing is asked of jawd");
    press(&mut v, &rx, "<esc>");
    assert_eq!(v.theme, Theme::with(true), "Esc goes back to the theme kept");
    press(&mut v, &rx, "\n<up>\n");
    assert_eq!(demo.settings().theme, "starship");
    assert_eq!(v.theme, Theme::named("starship", true));
    assert!(frame(&mut v).contains("Starship ›"), "back in the panel, with the theme kept");
}

#[test]
fn your_messages_switch_at_once_and_the_conversation_beside_shows_it() {
    let (mut v, rx, demo) = view(140, 40);
    press(&mut v, &rx, "<c-,><down><right>");
    assert_eq!(demo.settings().messages, Messages::Irc);
    let screen = frame(&mut v);
    assert!(screen.contains("◉ on the left, as IRC") && screen.contains("You   "), "{screen}");
}

#[test]
fn ctrl_comma_closes_it_from_the_theme_list_keeping_nothing_previewed() {
    let (mut v, rx, demo) = view(140, 40);
    press(&mut v, &rx, "<c-,>\n<down><down><c-,>");
    assert!(v.settings_panel.is_none());
    assert_eq!(v.theme, Theme::with(true));
    assert_eq!(demo.settings().theme, "tokyo-night");
}

#[test]
fn below_ninety_columns_the_panel_takes_the_screen() {
    let (mut v, rx, _) = view(80, 24);
    press(&mut v, &rx, "<c-,>");
    let screen = frame(&mut v);
    let top = screen.lines().nth(1).unwrap();
    assert!(top.starts_with("╭ settings ─") && top.ends_with('╮'), "{screen}");
}

#[test]
fn in_the_focused_mode_the_panel_unfolds_into_the_chat_lists_place_while_its_open() {
    let (mut v, rx, _) = view_with("focus", State::Online, 160, 45);
    press(&mut v, &rx, "<c-f><c-,>");
    let screen = frame(&mut v);
    let top = screen.lines().nth(1).unwrap();
    assert!(top.starts_with("╭ settings ─") && top.contains("╮╭ Ana Ribeiro"), "{screen}");
    assert!(!screen.contains("╭ folders") && !screen.contains("╭ chats"), "the rest stays folded:\n{screen}");
    press(&mut v, &rx, "<c-,>");
    let screen = frame(&mut v);
    assert!(screen.lines().nth(1).unwrap().starts_with("╭ Ana Ribeiro"), "closed, focused again:\n{screen}");
}

#[test]
fn with_two_panes_the_panel_takes_the_lists_place_and_the_panes_beside_it_show_the_preview() {
    let (mut v, rx, _) = view_with("panes", State::Online, 160, 45);
    press(&mut v, &rx, "<c-p>%<c-,>");
    let screen = frame(&mut v);
    let top = screen.lines().nth(1).unwrap();
    assert!(top.starts_with("╭ folders ─") && top.contains("╮╭ settings ─"), "{screen}");
    assert!(top.contains("╮╭ 1 Ana Ribeiro") && top.contains("╮╭ 2  Mãe ─"), "both panes beside it:\n{screen}");
    assert!(!screen.contains("╭ chats"), "{screen}");
    press(&mut v, &rx, "\n<home><down><down><down><down><down>");
    let gruvbox = Settings { theme: "gruvbox".into(), messages: Messages::Bubbles };
    let t = Theme::named(&gruvbox.theme, true);
    assert_eq!(v.theme, t, "Gruvbox, previewed");
    let now = drawn(&mut v);
    assert!(!now.content.iter().any(|c| c.bg == Theme::with(true).bg), "nothing on Tokyo Night's ground");
    let (ana, mae) = (v.panes_in_order()[0], v.pane_with_keys());
    let (parked, keys) = (v.pane_area(ana).unwrap(), v.pane_area(mae).unwrap());
    let corners = (now[find(&now, "╭ settings")].fg, now[(keys.x, keys.y)].fg);
    assert_eq!(corners, (t.focus, t.gutter), "the panel's border is the focused one, not Mãe's pane's");
    let (mut all_along, rx, _) = view_with("panes", State::Online, 160, 45);
    all_along.apply(Update::Settings(gruvbox));
    press(&mut all_along, &rx, "<c-p>%");
    let was = drawn(&mut all_along);
    let mut cells = (parked.x..parked.right()).flat_map(|x| (parked.y..parked.bottom()).map(move |y| (x, y)));
    let differ = cells.find(|&(x, y)| now[(x, y)] != was[(x, y)]);
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
        let buf = term.backend().buffer().clone();
        let column = find(&buf, if v.settings_panel.is_some() { "╭ settings" } else { "╭ chats" });
        let (conversation, b) = (find(&buf, "╭ Ana Ribeiro"), v.areas.composer);
        ((buf[column].fg, buf[conversation].fg, buf[(b.x - 2, b.y - 1)].fg), term.backend().cursor_visible())
    };
    assert_eq!(look(&mut v), ((t.gutter, t.gutter, t.focus), true), "typing, the box has them");
    press(&mut v, &rx, "<c-,>");
    assert_eq!(look(&mut v), ((t.focus, t.gutter, t.gutter), false), "the panel, and only the panel");
    press(&mut v, &rx, "<c-,><up>");
    assert_eq!(look(&mut v), ((t.gutter, t.focus, t.gutter), false), "a message selected, the conversation");
    press(&mut v, &rx, "<c-,>");
    assert_eq!(look(&mut v), ((t.focus, t.gutter, t.gutter), false), "the panel again");

    let (mut v, rx, _) = view_with("composer", State::Online, 160, 45);
    press(&mut v, &rx, "<c-,>");
    let mut term = Terminal::new(TestBackend::new(160, 45)).unwrap();
    term.draw(|f| v.draw(f)).unwrap();
    let corner = term.backend().buffer()[(0, v.areas.composer.y - 1)].clone();
    assert_eq!((corner.symbol(), corner.fg), ("╭", t.gutter), "the box along the bottom gives it up too");
    assert!(!term.backend().cursor_visible(), "and its cursor");
}

#[test]
fn the_panel_takes_the_keys_from_a_sidebar_that_would_be_drawn_over_it() {
    let (mut v, rx, _) = view(100, 30);
    press(&mut v, &rx, "<c-b>");
    assert!(frame(&mut v).lines().nth(1).unwrap().starts_with("╭ folders ─"), "over the chat list");
    press(&mut v, &rx, "<c-,>");
    let screen = frame(&mut v);
    assert!(screen.lines().nth(1).unwrap().starts_with("╭ settings ─") && !screen.contains("╭ folders"), "{screen}");
    press(&mut v, &rx, "<c-,>");
    assert!(frame(&mut v).lines().nth(1).unwrap().starts_with("╭ chats · All ─"), "and keeps them once it closes");
}

#[test]
fn a_theme_jaw_doesnt_know_shows_in_the_panel_as_tokyo_night_which_it_is_drawn_in() {
    let (mut v, rx, _) = view(140, 40);
    v.apply(Update::Settings(Settings { theme: "solarized".into(), messages: Messages::Bubbles }));
    press(&mut v, &rx, "<c-,>");
    assert!(frame(&mut v).contains("Tokyo Night ›"));
    press(&mut v, &rx, "\n<down>");
    let screen = frame(&mut v);
    assert!(screen.contains("Tokyo Night ✓") && screen.contains("▸ Vantablack"), "{screen}");
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
    for (update, said) in [(Update::Connection(logged_out), "⏎ link again"), (unlinked, "p link with a phone number")]
    {
        let (mut v, rx, _) = view(140, 40);
        press(&mut v, &rx, "<c-,>\n<down>");
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
    press(&mut v, &rx, "<c-k>theme");
    assert!(frame(&mut v).contains("Change the theme"));
    press(&mut v, &rx, "<esc><c-k>irc");
    assert!(frame(&mut v).contains("Turn the IRC layout on"));
    press(&mut v, &rx, "<esc><c-k>settings");
    assert!(frame(&mut v).contains("ctrl-,"), "the palette teaches the key");
}

#[test]
fn a_push_while_the_panel_is_open_changes_the_theme_it_shows_as_kept() {
    let (mut v, rx, _) = view(140, 40);
    press(&mut v, &rx, "<c-,>");
    v.apply(Update::Settings(Settings { theme: "nord".into(), messages: Messages::Bubbles }));
    assert!(frame(&mut v).contains("Nord ›"));
    assert_eq!(v.theme, Theme::named("nord", true));
}

#[test]
fn a_push_while_a_theme_is_previewed_leaves_the_preview_on_screen_and_esc_returns_to_the_new_one() {
    let (mut v, rx, _) = view(140, 40);
    press(&mut v, &rx, "<c-,>\n<down>");
    v.apply(Update::Settings(Settings { theme: "nord".into(), messages: Messages::Bubbles }));
    assert_eq!(v.theme, Theme::named("vantablack", true), "the preview goes on");
    let screen = frame(&mut v);
    assert!(screen.contains("Nord ✓") && screen.contains("▸ Vantablack"), "the one kept is the one pushed:\n{screen}");
    press(&mut v, &rx, "<esc>");
    assert_eq!(v.theme, Theme::named("nord", true), "Esc goes back to the theme pushed");
    assert!(frame(&mut v).contains("Nord ›"));
}

#[test]
fn a_change_jawd_doesnt_keep_says_why_and_the_panel_goes_back_to_what_jawd_has() {
    let (mut v, rx, _) = view(140, 40);
    // jawd's answer to each change is taken off the channel unread, so the refusal comes instead.
    let answers = |rx: &Receiver<Event>| came(rx).iter().filter(|u| matches!(u, Update::SettingsKept(_))).count();
    press(&mut v, &rx, "<c-,>\n<down>");
    v.on_input(key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(v.theme, Theme::named("vantablack", true), "shown while jawd keeps it");
    assert_eq!(answers(&rx), 1, "Enter asks jawd once");
    v.apply(Update::SettingsRefused("jaw.db can't be written".into()));
    let screen = frame(&mut v);
    assert!(screen.lines().last().unwrap().contains("not kept: jaw.db can't be written"), "{screen}");
    assert!(screen.contains("╭ settings ─") && screen.contains("Tokyo Night ›"), "{screen}");
    assert_eq!(v.theme, Theme::with(true), "back to what jawd has");

    press(&mut v, &rx, "<down>");
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
fn the_palette_opens_the_panel_and_its_theme_list_where_ctrl_comma_cant_reach() {
    let (mut v, rx, _) = view(140, 40);
    press(&mut v, &rx, "<c-k>settings\n");
    assert_eq!(v.settings_panel, Some(Panel { row: Row::Theme, choosing: None }), "Settings opens the panel");
    assert!(frame(&mut v).contains("╭ settings ─"));
    press(&mut v, &rx, "<esc><c-k>theme\n");
    let kept = themes::all().iter().position(|t| t.id == "tokyo-night");
    let choosing = v.settings_panel.as_ref().and_then(|p| p.choosing);
    assert_eq!(choosing, kept, "Change the theme opens the list with the theme kept under the keys");
    assert!(frame(&mut v).contains("▸ Tokyo Night ✓"));
}

#[test]
fn the_palette_turns_the_irc_layout_on_and_off_at_once_asking_jawd_once() {
    let (mut v, rx, demo) = view(140, 40);
    for (action, messages) in
        [("Turn the IRC layout on", Messages::Irc), ("Turn the IRC layout off", Messages::Bubbles)]
    {
        press(&mut v, &rx, "<c-k>irc");
        let screen = frame(&mut v);
        assert!(screen.contains(action), "\"irc\" finds it:\n{screen}");
        v.on_input(key(KeyCode::Enter, KeyModifiers::NONE));
        let irc = frame(&mut v).contains("You   ");
        assert_eq!((v.settings_shown().messages, irc), (messages, messages == Messages::Irc), "{action}: at once");
        assert_eq!(answered(&mut v, &rx), 1, "{action}: asked once");
        assert_eq!(demo.settings().messages, messages, "{action}: kept");
    }
}

#[test]
fn esc_closes_the_panel() {
    let (mut v, rx, _) = view(140, 40);
    press(&mut v, &rx, "<c-,><esc>");
    assert!(v.settings_panel.is_none() && frame(&mut v).contains("╭ chats · All ─"));
}

#[test]
fn ctrl_comma_does_nothing_before_jawd_answers_or_on_the_linking_screen() {
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut v = View::new(Box::new(Demo::start(tx, Clock::Fixed(NOW), State::Online, false)));
    v.on_input(key(KeyCode::Char(','), KeyModifiers::CONTROL));
    assert!(v.settings_panel.is_none(), "before jawd answers");
    let (mut v, rx, _) = view_with("", State::Linking, 100, 30);
    press(&mut v, &rx, "<c-,>");
    assert!(v.settings_panel.is_none() && frame(&mut v).contains("link jaw to WhatsApp"), "on the linking screen");
}

#[test]
fn the_theme_list_marks_the_row_with_the_keys_in_front_and_the_theme_kept_after_its_name() {
    let (mut v, rx, _) = view(140, 40);
    press(&mut v, &rx, "<c-,>\n");
    let screen = frame(&mut v);
    assert!(screen.contains("│ ▸ Tokyo Night ✓    dark  ■■■■■ │"), "opening, the theme kept has both:\n{screen}");
    assert!(screen.contains("│   Catppuccin Latte light ■■■■■ │"), "a long name fills its column:\n{screen}");
    press(&mut v, &rx, "<down>");
    let screen = frame(&mut v);
    assert!(screen.contains("│   Tokyo Night ✓    dark  ■■■■■ │"), "{screen}");
    assert!(screen.contains("│ ▸ Vantablack       dark  ■■■■■ │"), "{screen}");
    v.apply(Update::Settings(Settings { theme: "catppuccin-latte".into(), messages: Messages::Bubbles }));
    press(&mut v, &rx, "<esc>\n");
    let screen = frame(&mut v);
    assert!(screen.contains("│ ▸ Catppuccin La… ✓ light ■■■■■ │"), "kept, it makes room for its tick:\n{screen}");
}

#[test]
fn enter_switches_your_messages_too_asking_jawd_once() {
    let (mut v, rx, demo) = view(140, 40);
    press(&mut v, &rx, "<c-,><down>");
    for (on, messages) in [("◉ on the left, as IRC", Messages::Irc), ("◉ on the right", Messages::Bubbles)] {
        v.on_input(key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(frame(&mut v).contains(on), "{on}, at once");
        assert_eq!(answered(&mut v, &rx), 1, "{on}: asked once");
        assert_eq!(demo.settings().messages, messages);
    }
}

#[test]
fn j_and_k_move_between_the_settings_and_through_the_themes() {
    let (mut v, rx, _) = view(140, 40);
    let row = |v: &View| v.settings_panel.as_ref().map(|p| p.row);
    press(&mut v, &rx, "<c-,>j");
    assert_eq!(row(&v), Some(Row::Messages));
    press(&mut v, &rx, "k");
    assert_eq!(row(&v), Some(Row::Theme));
    press(&mut v, &rx, "\nj");
    assert_eq!(v.theme, Theme::named("vantablack", true), "j previews the next theme");
    press(&mut v, &rx, "k");
    assert_eq!(v.theme, Theme::with(true), "k the one before");
}

#[test]
fn ctrl_k_and_ctrl_j_move_nothing_in_the_panel_or_its_list() {
    let (mut v, rx, _) = view(140, 40);
    let row = |v: &View| v.settings_panel.as_ref().map(|p| p.row);
    press(&mut v, &rx, "<c-,><c-j>");
    assert_eq!(row(&v), Some(Row::Theme));
    press(&mut v, &rx, "<down><c-k>");
    assert_eq!(row(&v), Some(Row::Messages));
    press(&mut v, &rx, "<up>\n<c-k><c-j>");
    let kept = themes::all().iter().position(|t| t.id == "tokyo-night");
    assert_eq!(v.settings_panel.as_ref().and_then(|p| p.choosing), kept, "the list stays on the theme kept");
    assert_eq!(v.theme, Theme::with(true));
}

#[test]
fn each_theme_in_the_list_shows_its_mode_and_a_strip_of_its_colours_and_the_footer_its_keys() {
    let (mut v, rx, _) = view(140, 40);
    press(&mut v, &rx, "<c-,>\n");
    let screen = frame(&mut v);
    assert!(screen.contains("│   Gruvbox          dark  ■■■■■ │"), "{screen}");
    assert!(screen.contains("│   Flexoki Light    light ■■■■■ │"), "{screen}");
    assert!(screen.lines().last().unwrap().contains("↑↓ preview  ⏎ keeps it  esc goes back"), "{screen}");
    let buf = drawn(&mut v);
    let (_, y) = find(&buf, "Gruvbox");
    let strip: Vec<Color> =
        (0..buf.area.width).map(|x| &buf[(x, y)]).filter(|c| c.symbol() == "■").map(|c| c.fg).collect();
    let c = themes::get("gruvbox").unwrap().colors;
    let colours = [c.background, c.accent, c.red, c.green, c.blue].map(|c| crate::theme::color(c.0, true));
    assert_eq!(strip, colours, "its ground, its accent, red, green and blue");
}

#[test]
fn ctrl_comma_after_a_push_during_a_preview_keeps_nothing_previewed_and_shows_the_theme_pushed() {
    let (mut v, rx, _) = view(140, 40);
    press(&mut v, &rx, "<c-,>\n<down>");
    v.apply(Update::Settings(Settings { theme: "nord".into(), messages: Messages::Bubbles }));
    press(&mut v, &rx, "<c-,>");
    assert!(v.settings_panel.is_none());
    assert_eq!(v.theme, Theme::named("nord", true));
}

#[test]
fn with_jawd_away_and_no_chats_the_open_panel_stays_on_screen_through_a_reconnect() {
    let (mut v, rx, _) = view_with("", State::New, 140, 40);
    press(&mut v, &rx, "<c-,>");
    let away = || Update::Unreachable { why: "jawd stopped".into(), retry_in: 1_000 };
    v.apply(away());
    let screen = frame(&mut v);
    assert!(screen.contains("╭ settings ─") && !screen.contains("jawd isn't answering"), "{screen}");
    assert!(screen.lines().last().unwrap().contains("esc closes"), "its keys, which it has:\n{screen}");
    v.apply(Update::Hello {
        root: String::new(),
        linked: true,
        me: vec![],
        connection: Connection::new(ConnectionState::Online),
        settings: Settings::default(),
    });
    assert!(v.settings_panel.is_some(), "back, it's still open");
    v.apply(away());
    press(&mut v, &rx, "<esc>");
    assert!(frame(&mut v).contains("jawd isn't answering."), "closed, the screen says jawd is away");
}

#[test]
fn a_change_of_theme_redraws_the_screen_within_a_keypress() {
    let (mut v, _rx, _) = view(160, 45);
    frame(&mut v);
    let started = Instant::now();
    v.apply(Update::Settings(Settings { theme: "gruvbox".into(), messages: Messages::Irc }));
    frame(&mut v);
    let took = started.elapsed();
    assert!(took < Duration::from_millis(16), "{took:?}");
}

#[test]
fn a_key_in_the_theme_list_previews_the_next_theme_within_a_keypress() {
    let (mut v, rx, _) = view(160, 45);
    press(&mut v, &rx, "<c-,>\n");
    let started = Instant::now();
    v.on_input(key(KeyCode::Down, KeyModifiers::NONE));
    frame(&mut v);
    let took = started.elapsed();
    assert_eq!(v.theme, Theme::named("vantablack", true));
    assert!(took < Duration::from_millis(16), "{took:?}");
}
