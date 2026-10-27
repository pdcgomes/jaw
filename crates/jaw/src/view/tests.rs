//! Frames as text at several sizes, keys driving the view, and anchoring as rows arrive.

use super::*;
use crate::art;
use crate::data::{Chat, Hit, Kind, Media, Sender, Span, SpanKind};
use crate::demo::{Clock, Demo, State};
use crate::graphics::{Images, Protocol};
use crate::run::{self, Event};
use jaw_proto::settings::{Messages, Settings, SettingsSet};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::CellDiffOption;
use ratatui::crossterm::event::{
    Event as Input, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::sync::mpsc::Receiver;
use unicode_width::UnicodeWidthStr;

/// Friday 16 October 2026, 14:02 UTC.
pub const NOW: i64 = 1_792_159_320_000;
const UTC: FixedOffset = FixedOffset::east_opt(0).unwrap();

fn view_in(state: State, width: u16, height: u16) -> (View, Receiver<Event>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let demo = Demo::start(tx, Clock::Fixed(NOW), state, false);
    let mut v = View::new(Box::new(demo));
    v.clock = Some(NOW);
    v.tz = UTC;
    v.theme = Theme::with(true);
    v.no_color = false;
    v.resize(width, height);
    run::settle(&mut v, &rx);
    (v, rx)
}

fn view(width: u16, height: u16) -> (View, Receiver<Event>) {
    view_in(State::Online, width, height)
}

fn frame(v: &mut View) -> String {
    let (w, h) = v.size().unwrap();
    run::snapshot(v, w, h)
}

fn press(v: &mut View, rx: &Receiver<Event>, keys: &str) {
    run::press(v, rx, keys).unwrap();
}

/// The row of the screen a line containing `text` is on.
fn row_of(screen: &str, text: &str) -> Option<usize> {
    screen.lines().position(|l| l.contains(text))
}

fn message(id: &str, v: u64, sort: &str) -> Message {
    Message {
        id: id.into(),
        v,
        chat: "c".into(),
        sort: sort.into(),
        at: 0,
        from: None,
        mine: false,
        kind: Kind::Text,
        spans: vec![],
        reply: None,
        forwarded: 0,
        reactions: vec![],
        status: None,
        edited: false,
        deleted: false,
        summary: None,
        system: None,
        media: None,
        album: None,
        voice: None,
    }
}

fn theirs(id: &str, sort: &str, at: i64, text: &str) -> Message {
    let mut m = message(id, 1, sort);
    m.at = at;
    m.from = Some(Sender { id: "person:20".into(), name: "Marta".into(), colour: 0 });
    m.spans = vec![Span { t: SpanKind::Text, s: text.into(), person: None, url: None }];
    m
}

fn arrive(v: &mut View, chat: &str, upsert: Vec<Message>) {
    v.apply(Update::Messages { chat: chat.into(), update: jaw_client::Update { upsert, ..Default::default() } });
}

#[test]
fn at_140_columns_the_chat_list_sits_beside_the_conversation() {
    let (mut v, _rx) = view(140, 40);
    let screen = frame(&mut v);
    let first = screen.lines().next().unwrap();
    assert!(first.starts_with("╭  jaw  online ─") && first.ends_with("unread · 14:02 ╮"), "{screen}");
    assert!(screen.contains("╭ chats · All ─") && screen.contains("╭ Ana Ribeiro · typing… ─"), "{screen}");
    assert!(screen.contains("│ ● Ana Ribeiro ▴") && screen.contains("Família Gomes     11:52 @1 3"), "{screen}");
    assert!(screen.contains("Inês: ▣ photo") && screen.contains("draft: levo o bolo?"), "{screen}");
    assert!(screen.contains("Ana   vens jantar sexta? a tua mãe pergunta"), "{screen}");
    let mine = screen.lines().find(|l| l.contains("cheguei, boa noite")).unwrap();
    let at = mine.chars().position(|c| c == 'c').unwrap();
    assert_eq!(at, 26 + 34 + 2 + 76 / 3, "yours start a third of the way across the conversation:\n{mine}");
    assert!(mine.ends_with("✓✓  23:12 │"), "{mine}");
    assert!(screen.contains("Yesterday") && screen.contains("Today"), "day separators:\n{screen}");
    assert!(screen.lines().last().unwrap().starts_with(" ⏎ send  alt-⏎ new line  ↑ messages"), "{screen}");
    assert!(screen.lines().all(|l| crate::fmt::width(l) <= 140));
}

#[test]
fn in_the_irc_layout_yours_start_at_the_name_column_under_you() {
    let (mut v, rx) = view(140, 40);
    v.apply(Update::Settings(Settings { theme: "tokyo-night".into(), messages: Messages::Irc }));
    let screen = frame(&mut v);
    let line = |screen: &str, text: &str| screen.lines().find(|l| l.contains(text)).unwrap().to_string();
    let column = |line: &str, text: &str| line[..line.find(text).unwrap()].chars().count();
    let mine = line(&screen, "cheguei, boa noite");
    assert_eq!(column(&mine, "cheguei"), 26 + 34 + 2 + 6, "yours start at the name column:\n{mine}");
    assert!(mine.contains("│ You   cheguei"), "under You, as the sender changed:\n{screen}");
    assert!(mine.ends_with("✓✓  23:12 │"), "{mine}");
    assert!(line(&screen, "bom dia!").contains("│ You   bom dia!"), "on a new day:\n{screen}");
    assert!(line(&screen, "the one at the lake").contains("│ You   yes"), "{screen}");
    let next = line(&screen, "a Sofia tirou mais");
    let under = column(&next, "a Sofia") == 26 + 34 + 2 + 6 && !next.contains("You");
    assert!(under, "the next of yours sits under it:\n{screen}");

    let yours = |id: &str, sort: &str, at: i64, text: &str| Message {
        mine: true,
        from: None,
        status: Some(crate::data::Tick::Read),
        ..theirs(id, sort, at, text)
    };
    let soon = yours("soon", "9999999999990~000000001", NOW + MINUTE, "vou já");
    let later = yours("later", "9999999999990~000000002", NOW + 31 * MINUTE, "cheguei");
    arrive(&mut v, "ana", vec![soon, later]);
    run::settle(&mut v, &rx);
    let screen = frame(&mut v);
    assert!(line(&screen, "vou já").contains("│ You   vou já"), "{screen}");
    assert!(line(&screen, "14:33").contains("│ You   cheguei"), "after half an hour:\n{screen}");
    press(&mut v, &rx, "<up>");
    let detail = line(&frame(&mut v), "14:33:00 · read");
    assert_eq!(column(&detail, "Friday"), 26 + 34 + 2 + 6, "the selected one's detail too:\n{detail}");
}

#[test]
fn at_100_columns_the_list_keeps_its_width_and_the_conversation_takes_the_rest() {
    let (mut v, _rx) = view(100, 30);
    let screen = frame(&mut v);
    let second = screen.lines().nth(1).unwrap();
    assert_eq!(second.find("╮╭").map(|i| second[..i].chars().count()), Some(33), "{screen}");
    assert!(screen.contains("vens jantar sexta?"), "{screen}");
    assert!(screen.lines().all(|l| crate::fmt::width(l) <= 100));
}

#[test]
fn below_90_columns_the_list_takes_the_screen_until_a_chat_is_picked() {
    let (mut v, rx) = view(80, 24);
    let screen = frame(&mut v);
    assert!(screen.contains("chats · All") && !screen.contains("vens jantar"), "{screen}");
    assert!(screen.contains("⏎ open"), "{screen}");
    press(&mut v, &rx, "<down>\n");
    let screen = frame(&mut v);
    assert!(screen.contains("╭ Mãe ─") && screen.contains("ok, ligo-te logo à noite"), "{screen}");
    assert!(!screen.contains("chats · All") && screen.contains("esc chat list"), "{screen}");
    press(&mut v, &rx, "<esc>");
    assert!(frame(&mut v).contains("chats · All"), "Esc goes back to the list");
}

#[test]
fn up_from_an_empty_box_selects_the_newest_message_and_esc_steps_back() {
    let (mut v, rx) = view(100, 30);
    press(&mut v, &rx, "<up>");
    let newest = v.messages.as_ref().unwrap().newest().unwrap().id.clone();
    assert_eq!(v.focus, Focus::Message(newest));
    let screen = frame(&mut v);
    assert!(screen.contains("y copy") && screen.contains("esc back to typing"), "{screen}");
    assert!(screen.contains("Ana · Friday 16 October 2026, 14:02:00"), "everything on the selected one:\n{screen}");
    press(&mut v, &rx, "<up><up>");
    let screen = frame(&mut v);
    assert!(screen.contains("Ana · Friday 16 October 2026, 14:01"), "{screen}");
    press(&mut v, &rx, "<esc>");
    assert_eq!(v.focus, Focus::Typing);
    press(&mut v, &rx, "olá<up>");
    assert_eq!(v.focus, Focus::Typing, "with text in the box, Up stays in it");
}

#[test]
fn single_keys_act_on_the_selected_message_and_others_type() {
    let (mut v, rx) = view(100, 30);
    press(&mut v, &rx, "<up>y");
    assert_eq!(v.take_effects(), [Effect::Copy("vens jantar sexta? a tua mãe pergunta".into())]);
    press(&mut v, &rx, "s");
    assert!(frame(&mut v).contains("saving files comes with M3"), "a key for later says when it comes");
    press(&mut v, &rx, "<up>\n");
    assert!(frame(&mut v).contains("there's nothing to open in this one"));
    press(&mut v, &rx, "ok");
    assert_eq!((v.focus.clone(), v.composer.text()), (Focus::Typing, "ok"), "a key that acts on nothing types");
}

#[test]
fn enter_sends_and_alt_enter_adds_a_line() {
    let (mut v, rx) = view(100, 30);
    press(&mut v, &rx, "sim!<a-enter>e levo a sobremesa");
    assert_eq!(v.composer.text(), "sim!\ne levo a sobremesa");
    let screen = frame(&mut v);
    assert!(screen.contains("│ sim!") && screen.contains("│ e levo a sobremesa"), "the box grows:\n{screen}");
    press(&mut v, &rx, "\n");
    assert!(v.composer.is_empty());
    let screen = frame(&mut v);
    let sent = row_of(&screen, "e levo a sobremesa").unwrap();
    assert!(
        screen.lines().nth(sent - 1).unwrap().contains("sim!  ") && screen.lines().nth(sent - 1).unwrap().contains("◷"),
        "{screen}"
    );
    assert_eq!(v.scroll, Scroll::Bottom);
}

#[test]
fn alt_up_and_down_move_between_chats_and_the_draft_stays_with_its_chat() {
    let (mut v, rx) = view(100, 30);
    press(&mut v, &rx, "<a-down>levo o vinho<a-down>");
    assert_eq!(v.open.as_deref(), Some("escola"));
    assert!(v.composer.is_empty());
    let screen = frame(&mut v);
    assert!(screen.contains("draft: levo o vinho"), "{screen}");
    press(&mut v, &rx, "<a-up>");
    assert_eq!((v.open.as_deref(), v.composer.text()), (Some("mae"), "levo o vinho"));
}

#[test]
fn the_keys_show_on_question_mark_and_any_key_closes_them() {
    let (mut v, rx) = view(100, 30);
    press(&mut v, &rx, "?");
    let screen = frame(&mut v);
    assert!(
        screen.contains("╭ keys ─") && screen.contains("save · M3") && screen.contains("any key closes"),
        "{screen}"
    );
    press(&mut v, &rx, "?");
    assert!(v.overlay.is_none());
    assert_eq!(v.composer.text(), "?", "the key that closed them is typed, so a lone ? can be sent");
}

#[test]
fn at_80_by_24_the_keys_show_every_line_and_say_where_folders_are() {
    let rows = |screen: &str| {
        let top = row_of(screen, "╭ keys ─").unwrap_or_else(|| panic!("{screen}"));
        row_of(screen, " any key closes ─").unwrap_or_else(|| panic!("{screen}")) - top - 1
    };
    let (mut v, rx) = view(80, 24);
    press(&mut v, &rx, "?");
    let screen = frame(&mut v);
    let all = run::snapshot(&mut v, 120, 60);
    assert_eq!(rows(&screen), rows(&all), "every line, as a tall screen has them:\n{screen}\n{all}");
    for said in [
        "typing                                 a selected message",
        "/search    words in this chat          esc        back to typing",
        "alt-↑ ↓    the chat before, after      the mouse  click, scroll, open",
        "alt-⇧-↑ ↓  the folder before, after    ctrl-b     folders and people",
        "esc        the chat list, when narrow  ctrl-c     quit",
    ] {
        assert!(screen.contains(said), "{said}:\n{screen}");
    }
    assert!(!screen.contains("sidebar"), "{screen}");
}

#[test]
fn readline_keys_edit_the_box() {
    let (mut v, rx) = view(100, 30);
    press(&mut v, &rx, "vens jantar sexta<a-b>na <c-a><a-d><c-e>?");
    assert_eq!(v.composer.text(), " jantar na sexta?");
    press(&mut v, &rx, "<c-w><c-u>");
    assert!(v.composer.is_empty());
}

#[test]
fn history_arriving_above_moves_nothing_on_screen() {
    let (mut v, rx) = view(100, 24);
    press(&mut v, &rx, "<a-down><a-down><a-down><a-down>");
    assert_eq!(v.open.as_deref(), Some("climbing"));
    press(&mut v, &rx, "<pgup><pgup>");
    let before = frame(&mut v);
    assert!(matches!(v.scroll, Scroll::Anchor { .. }), "{:?}", v.scroll);
    let oldest = v.messages.as_ref().unwrap().oldest().unwrap().clone();
    let older: Vec<Message> = (0..30)
        .map(|i| {
            theirs(&format!("old{i}"), &format!("0000000000001~{i:09}"), oldest.at - 1_000 * (30 - i), "from long ago")
        })
        .collect();
    arrive(&mut v, "climbing", older);
    let after = frame(&mut v);
    assert_eq!(before, after, "the conversation is anchored on a message");
    assert!(!after.contains("from long ago"));
}

#[test]
fn new_messages_below_scroll_only_at_the_bottom_and_are_counted_otherwise() {
    let (mut v, rx) = view(100, 24);
    press(&mut v, &rx, "<a-down><a-down><a-down><a-down><up><esc>");
    let at_bottom = frame(&mut v);
    arrive(&mut v, "climbing", vec![theirs("new1", "9999999999990~000000001", NOW, "at the bottom it scrolls")]);
    let screen = frame(&mut v);
    assert!(screen.contains("at the bottom it scrolls") && screen != at_bottom, "{screen}");
    press(&mut v, &rx, "<pgup>");
    let reading = frame(&mut v);
    arrive(&mut v, "climbing", vec![theirs("new2", "9999999999990~000000002", NOW, "further up it waits")]);
    let screen = frame(&mut v);
    assert!(!screen.contains("further up it waits"), "{screen}");
    let marker = |s: &str| s.lines().find(|l| l.contains(" new ↓")).map(str::to_string);
    assert!(marker(&screen).is_some_and(|m| m.contains("─ 1 new ↓")), "{screen}");
    for (a, b) in reading.lines().zip(screen.lines()) {
        assert!(a == b || b.contains(" new ↓"), "nothing else moved:\n{a}\n{b}");
    }
    press(&mut v, &rx, "<pgdn><pgdn><pgdn><pgdn><pgdn><pgdn><pgdn><pgdn>");
    let screen = frame(&mut v);
    assert!(
        screen.contains("further up it waits") && marker(&screen).is_none(),
        "the marker goes once they're reached:\n{screen}"
    );
}

#[test]
fn a_chat_opened_with_unread_messages_starts_at_the_first_of_them() {
    let (mut v, rx) = view(100, 20);
    press(&mut v, &rx, "<a-down><a-down><a-down><a-down>");
    let screen = frame(&mut v);
    assert!(screen.contains(" new ↓"), "the rest are counted below:\n{screen}");
    let Scroll::Anchor { id, top, .. } = &v.scroll else { panic!("{:?}", v.scroll) };
    let first = v.messages.as_ref().unwrap().rows().rev().filter(|m| !m.mine).nth(13).unwrap();
    assert_eq!((id, *top), (&first.id, 1), "the first unread, a line from the top");
}

#[test]
fn the_selected_chat_holds_its_row_while_the_list_reorders() {
    let (mut v, rx) = view(100, 14);
    press(&mut v, &rx, "<a-down><a-down><a-down><a-down>");
    let before = frame(&mut v);
    let row = row_of(&before, "Lisbon Climbing").unwrap();
    let mut tiago: Chat = v.chats.get("tiago").unwrap().clone();
    tiago.v += 1;
    tiago.sort = format!("0~{:013}", NOW + 1);
    v.apply(Update::Chats(jaw_client::Update { upsert: vec![tiago.clone()], total: Some(13), ..Default::default() }));
    assert_eq!(row_of(&frame(&mut v), "Lisbon Climbing"), Some(row), "held while moving through the list");
    v.clock = Some(NOW + HOLD + 1);
    v.tick();
    let after = frame(&mut v);
    assert_eq!(row_of(&after, "Lisbon Climbing"), Some(row), "and kept on its row once it reorders:\n{after}");
    assert_eq!(v.chats.rows().nth(2).map(|c| c.id.as_str()), Some("tiago"));
}

#[test]
fn the_mouse_opens_chats_selects_messages_and_scrolls() {
    let (mut v, rx) = view(100, 30);
    let screen = frame(&mut v);
    let row = row_of(&screen, "Mãe ▴").unwrap() as u16;
    let click = |column, row| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    v.on_input(Input::Mouse(click(5, row)));
    run::settle(&mut v, &rx);
    assert_eq!(v.open.as_deref(), Some("mae"));
    let screen = frame(&mut v);
    let row = row_of(&screen, "liga-me quando puderes").unwrap() as u16;
    v.on_input(Input::Mouse(click(50, row)));
    assert!(matches!(&v.focus, Focus::Message(_)));
    v.on_input(Input::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 50,
        row,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(matches!(v.scroll, Scroll::Anchor { .. }));
    v.on_input(Input::Paste("colado\ntexto".into()));
    assert_eq!((v.focus.clone(), v.composer.text()), (Focus::Typing, "colado\ntexto"), "a paste goes into the box");
}

#[test]
fn an_empty_conversation_has_the_mimic_which_chomps_when_the_first_message_comes() {
    let (mut v, rx) = view(100, 32);
    let open = |v: &mut View| v.open_chat("mariana");
    open(&mut v);
    run::settle(&mut v, &rx);
    let screen = frame(&mut v);
    assert!(screen.contains("No messages with Mariana yet. What you type goes to them."), "{screen}");
    assert!(screen.contains('▀'), "the Mimic:\n{screen}");
    press(&mut v, &rx, "olá!\n");
    assert!(v.chomp_until.is_some(), "it chomps");
    assert!(frame(&mut v).contains("olá!"));
}

#[test]
fn linking_shows_the_qr_code_beside_the_mimic_or_a_code_for_the_phone() {
    let (mut v, rx) = view_in(State::Linking, 100, 30);
    let screen = frame(&mut v);
    assert!(screen.contains("jaw  not linked") && screen.contains("link jaw to WhatsApp"), "{screen}");
    assert!(screen.contains("█████") && screen.contains("It changes in 20 s."), "{screen}");
    assert!(screen.contains("p link with a phone number instead"), "{screen}");
    press(&mut v, &rx, "p351912000000");
    assert!(frame(&mut v).contains("+351912000000"));
    press(&mut v, &rx, "\n");
    let screen = frame(&mut v);
    assert!(screen.contains("link with a phone number") && screen.contains("K4PM-7XQ2"), "{screen}");
    press(&mut v, &rx, "<esc>");
    assert!(frame(&mut v).contains("It changes in 20 s."), "Esc goes back to the QR code");
}

#[test]
fn a_light_themes_qr_code_is_dark_on_light_but_light_on_dark_without_colour() {
    let (mut v, _rx) = view_in(State::Linking, 100, 30);
    v.theme = Theme::named("catppuccin-latte", true);
    let Some(LinkStep::Qr { code, .. }) = v.link.clone() else { panic!("a QR code to link with") };
    for (no_color, light) in [(false, true), (true, false)] {
        v.no_color = no_color;
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        term.draw(|f| v.draw(f)).unwrap();
        let buf = term.backend().buffer();
        let at = |rows: &[String]| {
            let (w, h) = (rows[0].chars().count() as u16, rows.len() as u16);
            let row = |x: u16, y: u16| (x..x + w).map(|x| buf[(x, y)].symbol()).collect::<String>();
            (0..=30 - h)
                .flat_map(|y| (0..=100 - w).map(move |x| (x, y)))
                .find(|&(x, y)| rows.iter().zip(y..).all(|(r, y)| row(x, y) == *r))
        };
        let way = if light { "dark on light" } else { "light on dark" };
        let (x, y) = at(&art::qr(&code, light)).unwrap_or_else(|| panic!("no_color {no_color}: not {way}"));
        assert!(at(&art::qr(&code, !light)).is_none(), "no_color {no_color}: only {way}");
        if !no_color {
            let cell = &buf[(x, y)];
            assert_eq!((cell.fg, cell.bg), (v.theme.fg, v.theme.bg), "in the text colour on the ground");
        }
    }
}

#[test]
fn a_jawd_that_said_hello_before_its_engine_said_it_was_linked_shows_the_chats_once_it_connects() {
    let (mut v, rx) = view_in(State::Linking, 100, 30);
    assert!(frame(&mut v).contains("link jaw to WhatsApp"));
    v.apply(Update::Connection(Connection::new(ConnectionState::Connecting)));
    assert!(frame(&mut v).contains("link jaw to WhatsApp"), "connecting says nothing of whose account it is");
    v.apply(Update::Connection(Connection::new(ConnectionState::Online)));
    run::settle(&mut v, &rx);
    let screen = frame(&mut v);
    assert!(!screen.contains("link jaw to WhatsApp") && screen.contains("jaw  online"), "{screen}");
    assert!(screen.contains("chats · All"), "{screen}");
}

#[test]
fn logged_out_says_so_above_linking_again() {
    let (mut v, rx) = view_in(State::LoggedOut, 100, 36);
    let screen = frame(&mut v);
    assert!(screen.contains("jaw  logged out from the phone") && screen.contains("kept your messages"), "{screen}");
    assert!(screen.contains("Your phone logged jaw out."), "{screen}");
    assert!(!screen.contains("It changes in"), "{screen}");
    press(&mut v, &rx, "\n");
    let screen = frame(&mut v);
    assert!(screen.contains("logged out") && screen.contains("It changes in 20 s."), "{screen}");
}

fn logged_out(reason: &str) -> Connection {
    Connection { reason: Some(reason.into()), ..Connection::new(ConnectionState::LoggedOut) }
}

#[test]
fn logged_out_says_why_when_jawd_says() {
    for (reason, header, said) in [
        (
            "this device isn't linked any more",
            "logged out: this device isn't linked any more",
            "Logged out: this device isn't linked any more.",
        ),
        ("logged out from jaw", "logged out from jaw", "Logged out from jaw."),
        ("logged out from another device", "logged out from another device", "Logged out from another device."),
    ] {
        let (mut v, _rx) = view(100, 36);
        v.apply(Update::Connection(logged_out(reason)));
        let screen = frame(&mut v);
        assert!(screen.lines().next().unwrap().contains(&format!("jaw  {header} ")), "{screen}");
        assert!(screen.contains(said) && screen.contains("kept your messages"), "{screen}");
        assert!(!screen.contains("Your phone") && !screen.contains("from the phone"), "{screen}");
    }
}

#[test]
fn once_logged_out_jaw_stays_there_whatever_the_connection_says_until_it_is_linked_again() {
    let (mut v, rx) = view(100, 36);
    v.apply(Update::Connection(logged_out("logged out from another device")));
    for state in [ConnectionState::Offline, ConnectionState::Connecting] {
        v.apply(Update::Connection(Connection::new(state)));
        let screen = frame(&mut v);
        assert!(screen.lines().next().unwrap().contains("jaw  logged out from another device"), "{state:?}:\n{screen}");
        assert!(screen.contains("⏎ link again") && !screen.contains("chats · All"), "{state:?}:\n{screen}");
    }
    let hello = |connection| Update::Hello {
        root: String::new(),
        linked: false,
        me: vec![],
        connection,
        settings: Settings::default(),
    };
    v.apply(hello(Connection::new(ConnectionState::Offline)));
    let screen = frame(&mut v);
    assert!(screen.contains("⏎ link again") && !screen.contains("It changes in"), "linking waits for Enter:\n{screen}");
    v.apply(hello(logged_out("logged out from jaw")));
    let screen = frame(&mut v);
    assert!(screen.lines().next().unwrap().contains("jaw  logged out from jaw"), "a hello says why too:\n{screen}");
    assert!(screen.contains("⏎ link again") && !screen.contains("It changes in"), "{screen}");
    v.apply(Update::Connection(Connection::new(ConnectionState::Online)));
    run::settle(&mut v, &rx);
    let screen = frame(&mut v);
    assert!(screen.lines().next().unwrap().contains("jaw  online") && screen.contains("chats · All"), "{screen}");
}

#[test]
fn logged_out_and_linked_again_from_the_qr_code_goes_back_to_the_chats() {
    let (mut v, rx) = view(100, 36);
    v.apply(Update::Connection(logged_out("logged out from another device")));
    v.apply(Update::Connection(Connection::new(ConnectionState::Offline)));
    press(&mut v, &rx, "\n");
    assert!(frame(&mut v).contains("It changes in"), "Enter links again");
    v.apply(Update::Link(LinkStep::Done { phone: "+351912000000".into() }));
    run::settle(&mut v, &rx);
    let screen = frame(&mut v);
    assert!(screen.contains("chats · All") && !screen.contains("logged out"), "{screen}");
}

#[test]
fn the_header_says_how_the_connection_is() {
    for (state, said) in [
        (State::CatchingUp, "online · catching up 1,204 of 5,000 ━"),
        (State::PhoneOffline, "phone offline: photos and older history wait for it"),
        (State::Offline, "offline: what you send waits until it's back"),
        (State::Reconnecting, "reconnecting: the engine restarted"),
        (State::UpdateNeeded, "update needed: WhatsApp wants a newer jaw · reading still works"),
        (State::Banned, "temporarily banned until 18 October, 14:02"),
        (State::Starting, "starting jawd…"),
        (State::Unreachable, "can't reach jawd · trying again in 4 s"),
    ] {
        let (mut v, _rx) = view_in(state, 140, 30);
        let screen = frame(&mut v);
        assert!(screen.lines().next().unwrap().contains(said), "{state:?}:\n{screen}");
    }
}

#[test]
fn starting_jawd_and_jawd_out_of_reach_say_so() {
    let (mut v, _rx) = view_in(State::Starting, 100, 24);
    assert!(frame(&mut v).contains("Starting jawd, which keeps your messages and talks to WhatsApp."));
    let (mut v, _rx) = view_in(State::Unreachable, 100, 30);
    let screen = frame(&mut v);
    assert!(screen.contains("jawd isn't answering.") && screen.contains('▀'), "the Mimic, asleep:\n{screen}");
    assert_eq!(v.next_wake() - NOW, 1_000, "its countdown redraws each second");
}

#[test]
fn a_new_account_has_the_mimic_where_the_chats_will_be() {
    let (mut v, _rx) = view_in(State::New, 100, 30);
    let screen = frame(&mut v);
    assert!(screen.contains("No chats yet.") && screen.contains("They come as your phone sends them."), "{screen}");
}

/// What the made-up account has sent since it was last asked, without applying it.
fn came(rx: &Receiver<Event>) -> Vec<Update> {
    rx.try_iter().filter_map(|e| if let Event::Update(u) = e { Some(u) } else { None }).collect()
}

/// A view on the made-up account that has had jawd's hello and nothing else.
fn greeted(width: u16, height: u16) -> (View, Receiver<Event>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut v = View::new(Box::new(Demo::start(tx, Clock::Fixed(NOW), State::Online, false)));
    v.clock = Some(NOW);
    v.tz = UTC;
    v.resize(width, height);
    let hello = came(&rx);
    assert!(matches!(hello[..], [Update::Hello { .. }]), "{hello:?}");
    v.apply(hello[0].clone());
    (v, rx)
}

/// Between jawd's hello and the chat list's first rows the panels wait empty rather than say there
/// are no chats, and the first frame isn't done until the open chat's messages are in.
#[test]
fn until_the_chat_list_comes_nothing_says_there_are_no_chats() {
    let (mut v, rx) = greeted(140, 30);
    let screen = frame(&mut v);
    assert!(!screen.contains("No chats") && !screen.contains('▀') && !v.showing_messages(), "{screen}");
    run::settle(&mut v, &rx);
    let screen = frame(&mut v);
    assert!(v.showing_messages() && screen.contains("Ana Ribeiro") && !screen.contains("No chats"), "{screen}");
}

/// jawd's hello and each window's rows where it was asked to be, as a chat opens, are what the
/// screen waits for, which the loop draws as they come; the pushes after them wait for a frame.
#[test]
fn jawds_hello_and_a_windows_rows_where_it_was_asked_to_be_are_awaited_and_pushes_are_not() {
    let (mut v, rx) = greeted(140, 30);
    assert!(v.awaited, "jawd's hello");
    let windows = came(&rx);
    assert!(windows.iter().any(|u| matches!(u, Update::Chats(_))) && windows.iter().all(Update::awaited));
    for u in windows {
        v.apply(u);
    }
    let first = came(&rx);
    assert!(matches!(first[..], [Update::Messages { .. }]) && first[0].awaited(), "{first:?}");
    v.apply(first[0].clone());
    v.on_input(Input::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT)));
    let open = v.open.clone().unwrap();
    let opened = came(&rx);
    let rows = opened.iter().find(|u| matches!(u, Update::Messages { chat, .. } if *chat == open));
    assert!(rows.is_some_and(Update::awaited), "the next chat's messages: {opened:?}");
    let pushed = jaw_client::Update { upsert: vec![theirs("m-new", "9", NOW, "chego já")], ..Default::default() };
    assert!(!Update::Messages { chat: open, update: pushed }.awaited());
    assert!(!Update::Chats(jaw_client::Update::default()).awaited());
}

#[test]
fn nothing_is_drawn_before_jawd_answers_and_the_first_frame_is_in_the_theme_hello_brings() {
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut v = View::new(Box::new(Demo::start(tx, Clock::Fixed(NOW), State::Online, false)));
    v.tick();
    assert!(!v.dirty, "nothing to draw before jawd says hello");
    let wait = FIRST_FRAME_WAIT.as_millis() as i64;
    assert!(v.next_wake() - v.now() <= wait, "but the loop wakes to draw once a quarter of a second has passed");
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
    quiet.clock = Some(NOW);
    quiet.born -= FIRST_FRAME_WAIT;
    quiet.tick();
    assert!(
        quiet.dirty,
        "a jawd that says nothing for a quarter of a second still gets a frame, whatever the clock says"
    );
}

#[test]
fn a_change_shows_while_jawd_keeps_it_and_one_it_doesnt_keep_goes_back_and_says_why() {
    let (mut v, _rx) = view(140, 40);
    v.ask_settings(SettingsSet { theme: Some("nord".into()), messages: None });
    assert_eq!(v.theme, Theme::named("nord", true), "shown at once");
    v.apply(Update::SettingsRefused("jaw.db can't be written".into()));
    assert_eq!(v.theme, Theme::with(true), "back to what jawd has");
    assert!(frame(&mut v).lines().last().unwrap().contains("not kept: jaw.db can't be written"));
    v.apply(Update::Settings(Settings { theme: "kanagawa".into(), messages: Messages::Bubbles }));
    assert_eq!(v.theme, Theme::named("kanagawa", true), "another interface's change shows here too");
}

/// Changes asked one after another: the last asked stays shown while the answers and pushes for
/// those before it come, and a change another interface makes meanwhile is kept without showing
/// over it; a refusal goes back to what jawd has at once, even with another change on its way.
#[test]
fn with_changes_on_their_way_the_last_asked_stays_shown_until_jawd_has_answered_them_all() {
    let (mut v, rx) = view(140, 40);
    let kept = |theme: &str| Settings { theme: theme.into(), messages: Messages::Bubbles };
    v.ask_settings(SettingsSet { theme: Some("nord".into()), messages: None });
    v.ask_settings(SettingsSet { theme: Some("kanagawa".into()), messages: None });
    let answers = came(&rx);
    let (nord, kanagawa) = (kept("nord"), kept("kanagawa"));
    let both = [
        Update::SettingsKept(nord.clone()),
        Update::Settings(nord),
        Update::SettingsKept(kanagawa.clone()),
        Update::Settings(kanagawa.clone()),
    ];
    assert_eq!(answers, both);
    v.apply(Update::Settings(kept("gruvbox")));
    assert_eq!(v.theme, Theme::named("kanagawa", true), "another interface's change doesn't show over it");
    for u in answers {
        let what = format!("{u:?}");
        v.apply(u);
        assert_eq!(v.theme, Theme::named("kanagawa", true), "the last asked, after {what}");
    }
    assert_eq!((v.settings_asked.as_ref(), &v.settings), (None, &kanagawa), "all answered: what jawd has");

    v.ask_settings(SettingsSet { theme: Some("solarized".into()), messages: None });
    v.ask_settings(SettingsSet { theme: Some("nord".into()), messages: None });
    let mut answers = came(&rx).into_iter();
    let refused = answers.next().unwrap();
    assert!(matches!(refused, Update::SettingsRefused(_)), "{refused:?}");
    v.apply(refused);
    assert_eq!(v.settings_shown(), &kanagawa, "refused, back to what jawd has at once");
    answers.for_each(|u| v.apply(u));
    assert_eq!(v.settings_shown(), &kept("nord"));
}

/// A key or a resize before jawd's hello is taken at once, but the loop draws nothing until hello
/// comes, so the first frame is already in the theme hello brings.
#[test]
fn input_before_jawds_hello_is_taken_but_drawn_only_once_hello_comes_in_its_theme() {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut v = View::new(Box::new(Demo::start(tx.clone(), Clock::Fixed(NOW), State::Online, false)));
    v.clock = Some(NOW);
    v.tz = UTC;
    v.no_color = false;
    // However slowly hello comes, the first frame still waits for it.
    v.born += Duration::from_secs(10);
    let Some(Update::Hello { root, linked, me, connection, .. }) = came(&rx).pop() else { panic!("jawd's hello") };
    let gruvbox = Settings { theme: "gruvbox".into(), messages: Messages::Bubbles };
    let hello = Update::Hello { root, linked, me, connection, settings: gruvbox };
    let start = Instant::now();
    tx.send(Event::Input(Input::Resize(100, 30), start)).unwrap();
    tx.send(Event::Input(Input::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)), start)).unwrap();
    let saying = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        tx.send(Event::Update(hello)).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        tx.send(Event::Stop).unwrap();
    });
    let mut terminal = ratatui::Terminal::new(TestBackend::new(100, 30)).unwrap();
    let mut probe = run::Probe { from: Some(start + Duration::from_millis(100)), ..run::Probe::default() };
    run::drive(&mut v, &rx, &mut terminal, &mut probe, &mut |_| {}).unwrap();
    saying.join().unwrap();
    assert_eq!(probe.frames - probe.frames_from, 0, "nothing drawn before hello");
    assert!(probe.frames > 0 && v.overlay == Some(Overlay::Keys), "the key was taken, and drawn once hello came");
    let ground = terminal.backend().buffer()[(0, 0)].bg;
    assert_eq!(ground, Theme::named("gruvbox", v.theme.truecolor).bg, "in the theme hello brought");
}

#[test]
fn the_made_up_account_keeps_the_settings_asked_as_jawd_does_and_refuses_a_theme_it_doesnt_know() {
    let (tx, rx) = std::sync::mpsc::channel();
    let demo = Demo::start(tx, Clock::Fixed(NOW), State::Online, false);
    let mut v = View::new(Box::new(demo.clone()));
    v.clock = Some(NOW);
    v.theme = Theme::with(true);
    run::settle(&mut v, &rx);
    let kept = Settings { theme: "gruvbox".into(), messages: Messages::Irc };
    v.ask_settings(SettingsSet { theme: Some("gruvbox".into()), messages: Some(Messages::Irc) });
    let answered = [Update::SettingsKept(kept.clone()), Update::Settings(kept.clone())];
    assert_eq!(came(&rx), answered, "answered with all of them, then pushed");
    assert_eq!(demo.settings(), kept);
    v.ask_settings(SettingsSet { theme: Some("gruvbox".into()), messages: None });
    assert_eq!(came(&rx), [Update::SettingsKept(kept.clone())], "a value it has already changes nothing to push");
    v.ask_settings(SettingsSet { theme: Some("solarized".into()), messages: None });
    let refused = came(&rx);
    assert!(matches!(&refused[..], [Update::SettingsRefused(why)] if why.contains("solarized")), "{refused:?}");
    assert_eq!(demo.settings(), kept, "and keeps what it had");
}

#[test]
fn without_colour_the_selection_is_reversed() {
    let (mut v, _rx) = view(100, 30);
    v.no_color = true;
    let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    term.draw(|f| v.draw(f)).unwrap();
    let buf = term.backend().buffer();
    assert!(buf.content.iter().all(|c| c.fg == ratatui::style::Color::Reset && c.bg == ratatui::style::Color::Reset));
    assert!(buf.content.iter().any(|c| c.modifier.contains(ratatui::style::Modifier::REVERSED)));
}

#[test]
fn other_kinds_show_their_one_line_summary() {
    let (mut v, rx) = view(100, 40);
    press(&mut v, &rx, "<a-down><a-down><a-down><pgup><pgup>");
    let screen = frame(&mut v);
    for said in ["▶ video 0:42 · 18.4 MB", "⌖ Miradouro da Senhora do Monte", "◉ Dr.ª Marta Lopes", "▣ 2 photos"]
    {
        assert!(screen.contains(said), "{said}:\n{screen}");
    }
    let unknown = message("u", 1, "1");
    assert_eq!(Message { kind: Kind::Unknown, ..unknown }.kind, Kind::Unknown);
}

#[test]
fn a_keypress_redraws_and_nothing_else_wakes_but_the_clock() {
    let (mut v, rx) = view(100, 30);
    frame(&mut v);
    v.dirty = false;
    v.tick();
    assert!(!v.dirty, "nothing due, nothing to draw");
    assert_eq!(v.next_wake(), NOW + 60_000, "the clock's next minute");
    v.on_input(Input::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)));
    assert!(v.dirty);
    assert!(v.next_wake() <= NOW + 1_500, "the draft is saved once typing pauses");
    drop(rx);
}

fn palette_titles(v: &View) -> Vec<String> {
    v.palette_items()
        .into_iter()
        .filter_map(|i| match i {
            palette::Item::Heading(label, _) => Some(label.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn ctrl_k_opens_the_palette_over_the_screen_and_esc_closes_it_keeping_the_box() {
    let (mut v, rx) = view(140, 34);
    press(&mut v, &rx, "levo o vinho<c-k>");
    let screen = frame(&mut v);
    assert!(
        screen.contains("╭ palette ─") && screen.contains("a chat, a person, something to do, or words"),
        "{screen}"
    );
    assert!(screen.contains("recent") && screen.contains("Start a chat") && screen.contains("esc closes"), "{screen}");
    press(&mut v, &rx, "<esc>");
    assert!(v.palette.is_none());
    assert_eq!(v.composer.text(), "levo o vinho", "the message box keeps its text");
}

#[test]
fn only_the_answer_to_the_latest_find_is_kept() {
    let (mut v, rx) = view(140, 34);
    press(&mut v, &rx, "<c-k>a");
    let first = v.palette.as_ref().unwrap().asked;
    press(&mut v, &rx, "r");
    let latest = v.palette.as_ref().unwrap().asked;
    assert!(latest > first);
    let shown = v.palette.as_ref().unwrap().results.clone();
    v.apply(Update::Found { id: first, results: vec![], counts: Default::default() });
    assert_eq!(v.palette.as_ref().unwrap().results, shown, "an earlier answer that comes late changes nothing");
    v.apply(Update::FindFailed { id: first, cancelled: true, why: "replaced".into() });
    v.apply(Update::FindFailed { id: latest, cancelled: true, why: "replaced".into() });
    assert!(v.palette.as_ref().unwrap().failed.is_none(), "a cancelled one says nothing");
    v.apply(Update::FindFailed { id: latest, cancelled: false, why: "jaw.db is busy".into() });
    assert!(frame(&mut v).contains("jawd couldn't search: jaw.db is busy"));
}

#[test]
fn results_are_grouped_by_kind_names_first_and_a_verb_puts_actions_first() {
    let (mut v, rx) = view(140, 34);
    press(&mut v, &rx, "<c-k>ar");
    assert_eq!(palette_titles(&v), ["chats", "people", "folders", "actions"]);
    let screen = frame(&mut v);
    assert!(
        screen.contains("● Ana Ribeiro")
            && screen.contains("○ Artur Reis")
            && screen.contains("no chat yet · ⏎ starts one"),
        "{screen}"
    );
    assert!(
        screen.contains(" all   chats 4   people 4   folders 1   actions "),
        "the strip counts each kind:\n{screen}"
    );
    press(&mut v, &rx, "<c-u>mute");
    assert_eq!(palette_titles(&v)[0], "actions", "typing what you want to do puts the actions first");
    press(&mut v, &rx, "<c-u>j");
    assert!(!palette_titles(&v).contains(&"messages".to_string()), "one letter finds names and actions only");
    press(&mut v, &rx, "antar");
    let screen = frame(&mut v);
    assert!(palette_titles(&v).contains(&"messages".to_string()), "{screen}");
    assert!(screen.contains("Ana in Ana Ribeiro") || screen.contains("Ana vens jantar"), "{screen}");
    assert!(screen.contains("messages · ") && screen.contains("tab shows them all"), "{screen}");
}

#[test]
fn tab_narrows_to_one_kind_and_backspace_widens_again() {
    let (mut v, rx) = view(140, 34);
    press(&mut v, &rx, "<c-k>jantar\t");
    assert_eq!(v.palette.as_ref().unwrap().narrow, palette::Narrow::Kind(crate::data::FoundKind::Chat));
    assert_eq!(palette_titles(&v), ["chats"]);
    press(&mut v, &rx, "\t");
    assert_eq!(v.palette.as_ref().unwrap().narrow, palette::Narrow::Kind(crate::data::FoundKind::Message));
    let found = v.palette_items().iter().filter(|i| matches!(i, palette::Item::Found(_))).count();
    assert!(found > 4, "narrowed, all of them show: {found}");
    press(&mut v, &rx, "\t");
    assert_eq!(v.palette.as_ref().unwrap().narrow, palette::Narrow::All, "and round to all again");
    press(&mut v, &rx, "<s-tab><c-u><bs>");
    assert_eq!(v.palette.as_ref().unwrap().narrow, palette::Narrow::All);
}

#[test]
fn enter_opens_a_chat_and_says_it_was_opened() {
    let (mut v, rx) = view(140, 34);
    press(&mut v, &rx, "<c-k>sofia\n");
    assert!(v.palette.is_none());
    assert_eq!(v.open.as_deref(), Some("sofia"));
    press(&mut v, &rx, "<c-k>");
    let first = v.palette_items().into_iter().find_map(|i| match i {
        palette::Item::Found(n) => Some(n),
        _ => None,
    });
    let r = &v.palette.as_ref().unwrap().results[first.unwrap()];
    assert!(matches!(&r.hit, Hit::Chat(c) if c.id == "sofia"), "the chats opened most come first: {r:?}");
}

#[test]
fn enter_on_a_person_starts_a_chat_with_them() {
    let (mut v, rx) = view(140, 34);
    press(&mut v, &rx, "<c-k>artur\n");
    let open = v.open.clone().unwrap();
    assert_eq!(v.chats.get(&open).map(|c| c.name.as_str()), Some("Artur Reis"));
    assert!(frame(&mut v).contains("No messages with Artur yet."));
    press(&mut v, &rx, "<c-k>artur");
    assert_eq!(palette_titles(&v), ["chats"], "now found as a chat");
}

#[test]
fn enter_on_a_message_opens_its_chat_on_it_selected() {
    let (mut v, rx) = view(140, 34);
    press(&mut v, &rx, "<c-k>miradouro\n");
    assert_eq!(v.open.as_deref(), Some("familia"));
    let Focus::Message(id) = v.focus.clone() else { panic!("{:?}", v.focus) };
    let m = v.messages.as_ref().unwrap().get(&id).unwrap();
    assert!(m.summary.as_deref().unwrap_or_default().contains("Miradouro"));
    let screen = frame(&mut v);
    assert!(screen.contains("Miradouro da Senhora do Monte") && screen.contains("esc back to typing"), "{screen}");
}

#[test]
fn ctrl_g_goes_to_a_message_and_says_what_it_does_on_anything_else() {
    let (mut v, rx) = view(140, 34);
    press(&mut v, &rx, "<c-k>ar<c-g>");
    assert!(v.palette.is_some());
    assert!(frame(&mut v).contains("ctrl-g goes to a message"));
    press(&mut v, &rx, "<c-u>casadapraia<c-g>");
    assert_eq!(v.open.as_deref(), Some("familia"));
    assert!(matches!(v.focus, Focus::Message(_)));
}

#[test]
fn a_folder_and_what_comes_later_say_when_they_come() {
    let (mut v, rx) = view(140, 34);
    press(&mut v, &rx, "<c-k>archived\n");
    assert!(frame(&mut v).contains("chats · Archived"), "a folder found shows its chats");
    press(&mut v, &rx, "<c-k>new poll\n");
    assert!(frame(&mut v).contains("polls come with M2"));
    assert!(v.palette.is_some(), "it stays open");
}

#[test]
fn actions_pin_mute_archive_and_go_to_unread() {
    let (mut v, rx) = view(140, 34);
    press(&mut v, &rx, "<c-k>pin this\n");
    assert!(!v.chats.get("ana").unwrap().pinned, "Ana was pinned, so it offers to unpin her");
    press(&mut v, &rx, "<c-k>pin this\n");
    assert!(v.chats.get("ana").unwrap().pinned, "and then to pin her again");
    press(&mut v, &rx, "<a-down>");
    let open = v.open.clone().unwrap();
    press(&mut v, &rx, "<c-k>mute this\n");
    assert!(frame(&mut v).contains("For 8 hours") && v.palette.is_some(), "mute asks for how long");
    press(&mut v, &rx, "<down><down>\n");
    assert_eq!(v.chats.get(&open).and_then(|c| c.muted_until), Some(i64::MAX));
    press(&mut v, &rx, "<c-k>archive this\n");
    assert!(v.chats.get(&open).is_none(), "archived, it leaves the list");
    assert_eq!(v.open.as_deref(), Some(open.as_str()), "and stays open");
    let name = v.chat_known(&open).unwrap().name.clone();
    press(&mut v, &rx, &format!("<c-k>{name}"));
    assert!(frame(&mut v).contains("archived"), "the palette still finds it");
    press(&mut v, &rx, "<c-u>go to unread\n");
    assert!(v.open.as_ref().and_then(|id| v.chats.get(id)).is_some_and(|c| c.unread > 0 || c.marked_unread));
}

#[test]
fn search_in_a_chat_finds_only_its_messages_and_backspace_widens_it() {
    let (mut v, rx) = view(140, 34);
    press(&mut v, &rx, "<a-down><a-down><a-down>/search vinho\n");
    assert_eq!(v.open.as_deref(), Some("familia"));
    let screen = frame(&mut v);
    assert!(screen.contains(" in Família Gomes ") && screen.contains("vinho"), "{screen}");
    let p = v.palette.as_ref().unwrap();
    assert!(p.results.iter().all(|r| matches!(&r.hit, Hit::Message(m) if m.chat == "familia")), "{:?}", p.results);
    press(&mut v, &rx, "<c-u><bs>");
    assert!(v.palette.as_ref().unwrap().scope.is_none(), "Backspace on nothing widens to everywhere");
    press(&mut v, &rx, "vinho");
    assert!(
        v.palette.as_ref().unwrap().results.iter().any(|r| matches!(&r.hit, Hit::Message(m) if m.chat != "familia"))
    );
}

#[test]
fn the_palette_fits_at_140_100_and_80_columns() {
    for (w, h) in [(140, 34), (100, 30), (80, 24)] {
        let (mut v, rx) = view(w, h);
        press(&mut v, &rx, "<c-k>ar");
        let screen = frame(&mut v);
        assert!(
            screen.contains("╭ palette ─") && screen.contains("Ana Ribeiro") && screen.contains("Artur Reis"),
            "{w}:\n{screen}"
        );
        assert!(screen.lines().all(|l| crate::fmt::width(l) <= w as usize), "{w}:\n{screen}");
    }
}

fn pictured(protocol: Protocol, width: u16, height: u16) -> (View, Receiver<Event>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let demo = Demo::start(tx, Clock::Fixed(NOW), State::Online, false);
    let mut v = View::new(Box::new(demo));
    v.images = Images::here(protocol, (8, 16));
    v.clock = Some(NOW);
    v.tz = UTC;
    v.theme = Theme::with(true);
    v.no_color = false;
    v.resize(width, height);
    run::settle(&mut v, &rx);
    (v, rx)
}

/// The runs of half-blocks on a line: where each starts and how wide it is.
fn blocks_on(line: &str) -> Vec<(usize, usize)> {
    let mut runs = vec![];
    let mut start = None;
    for (i, c) in line.chars().chain([' ']).enumerate() {
        match (matches!(c, '▀' | '▄'), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                runs.push((s, i - s));
                start = None;
            }
            _ => {}
        }
    }
    runs
}

/// The rows of half-blocks right under the line containing `text`, and their runs.
fn picture_under(screen: &str, text: &str) -> Vec<Vec<(usize, usize)>> {
    let lines: Vec<&str> = screen.lines().collect();
    let Some(at) = row_of(screen, text) else { panic!("{text} isn't on screen:\n{screen}") };
    lines[at + 1..].iter().map(|l| blocks_on(l)).take_while(|runs| !runs.is_empty()).collect()
}

#[test]
fn a_photo_is_drawn_under_its_line_keeping_its_shape_at_140_100_and_80_columns() {
    for (width, keys) in [(140, ""), (100, ""), (80, "\n")] {
        let (mut v, rx) = pictured(Protocol::Blocks, width, 40);
        press(&mut v, &rx, keys);
        let screen = frame(&mut v);
        let rows = picture_under(&screen, "▣ photo · 4032 × 3024");
        assert_eq!(rows.len(), 15, "a 4:3 photo forty columns wide is fifteen rows at {width}:\n{screen}");
        let label = screen.lines().find(|l| l.contains("▣ photo · 4032")).unwrap();
        let col = label.chars().position(|c| c == '▣').unwrap();
        assert!(rows.iter().all(|r| *r == vec![(col, 40)]), "under its line, forty wide, at {width}:\n{screen}");
        let after = row_of(&screen, "▣ photo · 4032").unwrap() + 16;
        assert!(screen.lines().nth(after).unwrap().contains("❤ 2"), "its reactions come after it:\n{screen}");
    }
}

#[test]
fn a_photo_narrows_to_a_narrow_conversation() {
    let (mut v, rx) = pictured(Protocol::Blocks, 44, 40);
    press(&mut v, &rx, "\n");
    let screen = frame(&mut v);
    let rows = picture_under(&screen, "▣ photo");
    let (_, wide) = rows[0][0];
    assert!(wide < 40 && !rows.is_empty(), "{screen}");
    assert_eq!(rows.len() as u16, crate::graphics::fit(4032, 3024, wide as u16, 16, (8, 16)).1, "{screen}");
    for line in screen.lines().filter(|l| !blocks_on(l).is_empty()) {
        assert!(line.ends_with('│'), "the picture stays inside the box:\n{screen}");
    }
}

#[test]
fn an_album_is_tiles_two_across_a_sticker_is_smaller_and_a_poster_comes_under_its_play_line() {
    let (mut v, rx) = pictured(Protocol::Blocks, 140, 50);
    press(&mut v, &rx, "<a-down><a-down><a-down><up><pgup><pgup>");
    let screen = frame(&mut v);
    let album = picture_under(&screen, "▣ 2 photos");
    assert_eq!(album.len(), 8, "a tile is half a photo's most rows:\n{screen}");
    assert!(album.iter().all(|r| r.len() == 2 && r[0].1 == 20 && r[1].1 == 20 && r[1].0 == r[0].0 + 21), "{screen}");
    let caption = row_of(&screen, "▣ 2 photos").unwrap() + 9;
    assert!(screen.lines().nth(caption).unwrap().contains("a vista do quarto"), "the caption is under the tiles");

    let sticker = picture_under(&screen, "◇ sticker");
    assert!(!sticker.is_empty() && sticker.len() <= 8, "{screen}");
    assert!(sticker.iter().flatten().all(|(_, w)| *w <= 18), "{screen}");

    let poster = picture_under(&screen, "▶ video 0:42 · 18.4 MB");
    assert_eq!(poster.first().map(|r| r[0].1), Some(40), "{screen}");
}

#[test]
fn a_message_whose_previews_cant_be_read_is_its_summary_line() {
    let (mut v, rx) = pictured(Protocol::Blocks, 100, 30);
    let mut m = theirs("gone", "9999999999990~000000001", NOW, "");
    m.kind = Kind::Image;
    m.spans = vec![];
    m.summary = Some("▣ photo · long gone".into());
    m.media = Some(Box::new(Media {
        w: 800,
        h: 600,
        previews: crate::data::Previews {
            small: Some("nowhere-256.jpg".into()),
            large: Some("nowhere-768.jpg".into()),
        },
        ..Media::default()
    }));
    arrive(&mut v, "ana", vec![m, theirs("next", "9999999999990~000000002", NOW, "and then this")]);
    run::settle(&mut v, &rx);
    let screen = frame(&mut v);
    let at = row_of(&screen, "▣ photo · long gone").unwrap();
    assert_eq!(row_of(&screen, "and then this"), Some(at + 1), "no room is kept for it:\n{screen}");
}

#[test]
fn without_colour_photos_are_their_summary_lines() {
    let (mut v, rx) = pictured(Protocol::Blocks, 140, 40);
    v.no_color = true;
    v.laid.clear();
    run::settle(&mut v, &rx);
    let screen = frame(&mut v);
    assert!(!screen.contains('▀'), "{screen}");
    assert!(screen.contains("▣ photo · 4032 × 3024"));
}

#[test]
fn scrolling_leaves_nothing_of_an_escape_drawn_picture_behind() {
    let (mut v, rx) = pictured(Protocol::Iterm, 140, 50);
    press(&mut v, &rx, "<a-down><a-down><a-down><up><pgup><pgup>");
    let mut term = ratatui::Terminal::new(TestBackend::new(140, 50)).unwrap();
    let mut draw = |v: &mut View| {
        let buf = term.draw(|f| v.draw(f)).unwrap().buffer.clone();
        (buf, String::from_utf8_lossy(&v.take_writes()).to_string(), term.backend().buffer().clone())
    };
    let (buf, writes, _) = draw(&mut v);
    assert!(writes.is_empty() && buf.content.iter().all(|c| c.diff_option == CellDiffOption::None));
    std::thread::sleep(crate::graphics::STILL + std::time::Duration::from_millis(20));
    let (buf, writes, _) = draw(&mut v);
    assert_eq!(writes.matches("\x1b]1337;File=").count(), 4, "the tiles, the sticker and the poster, once still");
    assert!(buf.content.iter().any(|c| c.diff_option == CellDiffOption::Skip));
    let (_, writes, _) = draw(&mut v);
    assert!(writes.is_empty(), "standing still, they aren't drawn again");

    v.on_input(Input::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)));
    let (buf, writes, shown) = draw(&mut v);
    assert!(writes.is_empty(), "moving, they're half-blocks");
    for y in 0..buf.area.height {
        let mut covered = 0;
        for x in 0..buf.area.width {
            if covered > 0 {
                covered -= 1;
                continue;
            }
            let cell = &buf[(x, y)];
            covered = cell.symbol().width().saturating_sub(1);
            assert_eq!(cell, &shown[(x, y)], "the frame's cell at {x}, {y} reached the terminal");
        }
    }
}

#[test]
fn in_kitty_a_photo_is_placeholders_under_its_line_and_is_sent_once() {
    let (mut v, rx) = pictured(Protocol::Kitty, 140, 40);
    let sent = String::from_utf8_lossy(&v.take_writes()).to_string();
    assert_eq!(sent.matches("\x1b_Ga=T,U=1,").count(), 1, "the lake is sent once");
    let screen = frame(&mut v);
    let lines: Vec<&str> = screen.lines().collect();
    let at = row_of(&screen, "▣ photo · 4032 × 3024").unwrap();
    let rows: Vec<usize> = (at + 1..lines.len()).take_while(|r| lines[*r].contains('\u{10EEEE}')).collect();
    assert_eq!(rows.len(), 15);
    assert_eq!(lines[rows[0]].matches('\u{10EEEE}').count(), 40);
    press(&mut v, &rx, "<up><up>");
    frame(&mut v);
    assert!(!String::from_utf8_lossy(&v.take_writes()).contains("a=T"), "moving it sends nothing");
}

#[test]
fn a_picture_that_has_stood_still_wakes_the_loop_to_be_drawn_with_its_escape() {
    let (mut v, _rx) = pictured(Protocol::Iterm, 140, 40);
    frame(&mut v);
    v.dirty = false;
    let wake = v.next_wake() - v.now();
    assert!(wake <= crate::graphics::STILL.as_millis() as i64, "it asks to be woken in {wake} ms");
    std::thread::sleep(crate::graphics::STILL + std::time::Duration::from_millis(20));
    v.tick();
    assert!(v.dirty, "and once woken there's a frame to draw");
    frame(&mut v);
    assert!(String::from_utf8_lossy(&v.take_writes()).contains("\x1b]1337;File="));
}

/// A theme that changes draws each picture iTerm2 or sixel drew again on its new ground: first as
/// half-blocks, which reach the terminal in the new colours in the cells its escape had, then,
/// once it has stood still again, with its escape over them, so a transparent pixel shows the new
/// ground rather than what the terminal kept under it.
#[test]
fn a_theme_that_changes_draws_the_pictures_escapes_drew_again_on_its_ground() {
    let (mut v, rx) = pictured(Protocol::Iterm, 140, 50);
    press(&mut v, &rx, "<a-down><a-down><a-down><up><pgup><pgup>");
    let mut term = ratatui::Terminal::new(TestBackend::new(140, 50)).unwrap();
    let mut draw = |v: &mut View| {
        let buf = term.draw(|f| v.draw(f)).unwrap().buffer.clone();
        (buf, String::from_utf8_lossy(&v.take_writes()).to_string(), term.backend().buffer().clone())
    };
    let escapes = |writes: &str| writes.matches("\x1b]1337;File=").count();
    draw(&mut v);
    std::thread::sleep(crate::graphics::STILL + Duration::from_millis(20));
    let (buf, _, _) = draw(&mut v);
    let w = buf.area.width;
    let under: Vec<(u16, u16)> = (0..buf.content.len())
        .filter(|i| buf.content[*i].diff_option == CellDiffOption::Skip)
        .map(|i| (i as u16 % w, i as u16 / w))
        .collect();
    assert!(!under.is_empty(), "the pictures stand still, drawn with their escapes");
    v.apply(Update::Settings(Settings { theme: "gruvbox".into(), messages: Messages::Bubbles }));
    let (buf, writes, shown) = draw(&mut v);
    assert_eq!(escapes(&writes), 0, "first, half-blocks on the new ground");
    let reached =
        |&(x, y): &(u16, u16)| buf[(x, y)].diff_option == CellDiffOption::None && shown[(x, y)] == buf[(x, y)];
    assert!(under.iter().all(reached), "every cell the escapes had reached the terminal");
    let ground = Theme::named("gruvbox", true).bg;
    assert!(
        under.iter().any(|&(x, y)| shown[(x, y)].bg == ground),
        "the sticker's transparent pixels, on Gruvbox's ground"
    );
    std::thread::sleep(crate::graphics::STILL + Duration::from_millis(20));
    let (_, writes, _) = draw(&mut v);
    assert_eq!(escapes(&writes), 4, "then their escapes again, over it");
}

/// The open chat's voice message lasting `seconds`.
fn voice_message(v: &View, seconds: f64) -> Message {
    let w = v.messages.as_ref().unwrap();
    let mut at = w.oldest().cloned();
    while let Some(m) = at {
        if m.voice.as_ref().is_some_and(|x| x.seconds == seconds) {
            return m;
        }
        at = w.after(&m.sort).cloned();
    }
    panic!("no voice message of {seconds} s")
}

fn playback(m: &Message, state: crate::data::PlayState, at: f64) -> Update {
    Update::Playback(crate::data::Playback {
        message: Some(m.id.clone()),
        state,
        at,
        seconds: m.voice.as_ref().unwrap().seconds,
        speed: 1.0,
        error: None,
    })
}

fn line_with<'a>(screen: &'a str, text: &str) -> &'a str {
    screen.lines().find(|l| l.contains(text)).unwrap_or_else(|| panic!("{text} isn't on screen:\n{screen}"))
}

/// The waveform's cells on a line.
fn bars(line: &str) -> usize {
    line.chars().filter(|c| ('▁'..='█').contains(c)).count()
}

#[test]
fn a_voice_message_is_its_waveform_and_length_with_a_dot_until_played_at_140_100_and_80_columns() {
    for (width, keys) in [(140, ""), (100, ""), (80, "\n")] {
        let (mut v, rx) = view(width, 40);
        press(&mut v, &rx, keys);
        let screen = frame(&mut v);
        let line = line_with(&screen, "0:14");
        assert!(line.contains("Ana   ▶ "), "at {width}:\n{screen}");
        assert_eq!(bars(line), crate::layout::WAVE, "two of the 64 points to a cell at {width}:\n{screen}");
        assert!(line.contains("0:14 ●"), "not played yet, at {width}:\n{screen}");
    }
}

#[test]
fn a_narrow_conversation_narrows_the_waveform_and_your_own_has_no_dot() {
    let (mut v, rx) = view(52, 40);
    press(&mut v, &rx, "\n");
    let screen = frame(&mut v);
    let line = line_with(&screen, "0:14");
    assert!(bars(line) < crate::layout::WAVE && bars(line) >= 4, "{screen}");
    assert!(line.ends_with('│') && line.contains("0:14 ●"), "it all fits on its line:\n{screen}");
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<a-down><a-down><a-down><a-down><a-down><a-down>");
    assert_eq!(v.open.as_deref(), Some("jantar"));
    let screen = frame(&mut v);
    let mine = line_with(&screen, "0:06");
    assert!(!mine.contains('●') && mine.contains("✓✓"), "{screen}");
}

#[test]
fn enter_plays_a_voice_message_its_playhead_moves_with_jawds_pushes_and_enter_pauses_it() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<up><up><up>");
    let m = voice_message(&v, 14.0);
    assert_eq!(v.focus, Focus::Message(m.id.clone()));
    press(&mut v, &rx, "\n");
    let screen = frame(&mut v);
    let line = line_with(&screen, "0:14");
    assert!(
        line.contains("⏸ ") && line.contains("0:00 / 0:14") && !line.contains('●'),
        "played, the dot goes:\n{screen}"
    );
    assert!(screen.contains("⏎ pause") && screen.contains("←→ seek"), "{screen}");
    v.apply(playback(&m, crate::data::PlayState::Playing, 7.0));
    let at = |v: &View| v.play_of(&m.id).map(|p| p.at);
    assert_eq!(at(&v), Some(70));
    assert!(frame(&mut v).contains("0:07 / 0:14"));
    press(&mut v, &rx, "\n");
    let screen = frame(&mut v);
    assert!(line_with(&screen, "/ 0:14").contains("▶ "), "paused, it keeps its place:\n{screen}");
    press(&mut v, &rx, "\n");
    assert!(frame(&mut v).contains("⏸ "), "and carries on from there");
}

#[test]
fn left_and_right_seek_the_one_playing_and_x_steps_through_the_speeds() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<up><up><up>\n<right><right>");
    assert!(frame(&mut v).contains("0:10 / 0:14"));
    press(&mut v, &rx, "<left>");
    assert!(frame(&mut v).contains("0:05 / 0:14"));
    press(&mut v, &rx, "x");
    assert!(frame(&mut v).contains("0:05 / 0:14 · 1.5×"));
    press(&mut v, &rx, "x");
    assert!(frame(&mut v).contains("0:05 / 0:14 · 2×"));
    press(&mut v, &rx, "x");
    let screen = frame(&mut v);
    assert!(!line_with(&screen, "0:05 / 0:14").contains('×'), "back to 1×:\n{screen}");
}

#[test]
fn x_on_a_voice_message_not_playing_says_the_speed_the_next_will_play_at() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<up><up><up>x");
    assert!(frame(&mut v).contains("voice messages play at 1.5×"));
    press(&mut v, &rx, "\n");
    assert!(frame(&mut v).contains("0:00 / 0:14 · 1.5×"));
}

#[test]
fn when_one_ends_the_next_one_not_yet_played_plays_and_one_plays_at_a_time() {
    let (mut v, rx) = view(140, 50);
    press(&mut v, &rx, "<a-down><a-down><a-down>");
    let (first, next) = (voice_message(&v, 31.0), voice_message(&v, 9.0));
    v.toggle_voice(&first);
    run::settle(&mut v, &rx);
    v.apply(playback(&first, crate::data::PlayState::Ended, 31.0));
    assert_eq!(v.playing.as_ref().and_then(|p| p.message.clone()), Some(next.id.clone()));
    v.toggle_voice(&first);
    v.apply(playback(&next, crate::data::PlayState::Playing, 2.0));
    assert_eq!(
        v.playing.as_ref().and_then(|p| p.message.clone()),
        Some(first.id.clone()),
        "a late push undoes nothing"
    );
    v.apply(playback(&next, crate::data::PlayState::Stopped, 2.0));
    assert!(v.play_of(&first.id).is_some(), "the one stopped was the one before");
}

#[test]
fn ctrl_r_records_with_its_levels_and_time_and_enter_sends_it() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<c-r>");
    let screen = frame(&mut v);
    assert!(screen.contains("● recording") && screen.contains("⏎ sends  ctrl-r stops to listen  esc throws it away"));
    for (i, level) in [10u8, 60, 95, 40].iter().enumerate() {
        let r = crate::data::Recorded {
            state: crate::data::RecordState::Recording,
            seconds: 3.0 + i as f64 * 0.1,
            level: Some(*level),
            waveform: vec![],
            end: None,
            reason: None,
            error: None,
        };
        v.apply(Update::Recorded(r));
    }
    let screen = frame(&mut v);
    assert!(line_with(&screen, "● recording").contains("0:03"), "{screen}");
    assert!(screen.contains("▂▅█▄"), "the levels as they came:\n{screen}");
    press(&mut v, &rx, "abc<a-down>");
    assert_eq!((v.composer.text(), v.open.as_deref()), ("", Some("ana")), "keys wait while it records");
    press(&mut v, &rx, "\n");
    let screen = frame(&mut v);
    assert!(!screen.contains("● recording") && screen.contains("⏎ send"), "{screen}");
    let sent = v.messages.as_ref().unwrap().newest().unwrap().clone();
    assert!(sent.mine && sent.kind == Kind::Voice && sent.voice.is_some(), "{sent:?}");
}

#[test]
fn ctrl_r_again_stops_it_to_listen_and_esc_throws_it_away() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<c-r><c-r>");
    let screen = frame(&mut v);
    assert!(screen.contains("● recorded") && screen.contains("ctrl-r pauses"), "listening to it:\n{screen}");
    assert!(v.listening().is_some());
    press(&mut v, &rx, "<c-r>");
    assert!(frame(&mut v).contains("ctrl-r listens"));
    let before = v.messages.as_ref().unwrap().newest().unwrap().id.clone();
    press(&mut v, &rx, "<esc>");
    let screen = frame(&mut v);
    assert!(!screen.contains("● record") && v.listening().is_none(), "{screen}");
    assert_eq!(v.messages.as_ref().unwrap().newest().unwrap().id, before, "nothing was sent");
}

#[test]
fn jawd_turning_voice_down_brings_the_box_back_and_says_why() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<c-r>");
    v.apply(Update::VoiceRefused("voice comes with the next step".into()));
    let screen = frame(&mut v);
    assert!(!screen.contains("● recording") && screen.contains("voice comes with the next step"), "{screen}");
}

fn newest(v: &View) -> Message {
    v.messages.as_ref().unwrap().newest().unwrap().clone()
}

/// Your messages in the open chat's window that say `text`.
pub fn mine_saying(v: &View, text: &str) -> Vec<Message> {
    let Some(w) = v.messages.as_ref() else { return vec![] };
    w.rows().filter(|m| m.mine && m.spans.iter().map(|s| s.s.as_str()).collect::<String>() == text).cloned().collect()
}

/// The open chat's message `id`, as its window holds it.
pub fn shown(v: &View, id: &str) -> Option<Message> {
    v.messages.as_ref()?.get(id).cloned()
}

fn find_message(v: &View, text: &str) -> Message {
    let w = v.messages.as_ref().unwrap();
    let mut at = w.oldest().cloned();
    while let Some(m) = at {
        if m.spans.iter().map(|s| s.s.as_str()).collect::<String>().contains(text) {
            return m;
        }
        at = w.after(&m.sort).cloned();
    }
    panic!("no message saying {text}")
}

/// Ana's chat at each width, open: at 80 the list has the screen until Enter.
fn anas(width: u16) -> (View, Receiver<Event>) {
    let (mut v, rx) = view(width, 40);
    if width < 90 {
        press(&mut v, &rx, "\n");
    }
    (v, rx)
}

#[test]
fn r_replies_with_the_message_in_the_boxs_border_and_the_row_jawd_sends_quotes_it() {
    for width in [140, 100, 80] {
        let (mut v, rx) = anas(width);
        let asked = newest(&v);
        press(&mut v, &rx, "<up>rsim, levo");
        let screen = frame(&mut v);
        assert!(screen.contains("↩ Ana vens jantar sexta?") && screen.contains("esc drops the reply"), "{screen}");
        assert_eq!(v.focus, Focus::Typing);
        press(&mut v, &rx, "\n");
        let sent = newest(&v);
        let quote = sent.reply.as_ref().expect("it quotes what it answers");
        assert_eq!((quote.id.as_deref(), quote.who().as_str()), (Some(asked.id.as_str()), "Ana"));
        let screen = frame(&mut v);
        assert!(screen.contains("┃ Ana vens jantar sexta?") && !screen.contains("↩ Ana"), "at {width}:\n{screen}");
    }
}

#[test]
fn esc_drops_the_reply_and_keeps_what_was_typed() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<up>rsim<esc>");
    assert_eq!((v.composer.text(), v.replying.is_none()), ("sim", true));
}

#[test]
fn e_edits_your_own_message_in_the_box_and_jawds_row_says_edited() {
    for width in [140, 100, 80] {
        let (mut v, rx) = anas(width);
        press(&mut v, &rx, "um rascunho<up>");
        assert!(v.focus == Focus::Typing, "Up with text in the box moves in the box");
        press(&mut v, &rx, "<c-u><up><up>e");
        let screen = frame(&mut v);
        assert!(screen.contains("✎ editing") && v.composer.text() == "haha, ela vai adorar", "{screen}");
        press(&mut v, &rx, "!!\n");
        let edited = find_message(&v, "haha, ela vai adorar!!");
        assert!(edited.edited);
        let screen = frame(&mut v);
        assert!(line_with(&screen, "adorar!!").contains("edited"), "at {width}:\n{screen}");
    }
}

#[test]
fn editing_gives_the_draft_back_after_and_isnt_saved_as_one() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "olá<up>");
    press(&mut v, &rx, "<esc>");
    let haha = find_message(&v, "haha, ela vai adorar");
    v.edit(&haha);
    assert_eq!(v.composer.text(), "haha, ela vai adorar");
    press(&mut v, &rx, "<esc>");
    assert_eq!(v.composer.text(), "olá", "Esc stops editing");
    v.edit(&haha);
    press(&mut v, &rx, " :)\n");
    assert_eq!(v.composer.text(), "olá", "saved, the draft is back");
}

#[test]
fn someone_elses_message_and_one_too_old_cant_be_edited_and_say_why() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<up>e");
    assert!(frame(&mut v).contains("only your own messages can be edited"));
    let old = find_message(&v, "cheguei, boa noite");
    v.edit(&old);
    assert!(frame(&mut v).contains("15 minutes"));
    assert!(v.editing.is_none());
}

#[test]
fn d_deletes_for_everyone_leaving_a_line_or_for_you_taking_it_away() {
    for width in [140, 100, 80] {
        let (mut v, rx) = anas(width);
        press(&mut v, &rx, "<up><up>d");
        let screen = frame(&mut v);
        assert!(
            screen.contains("For me") && screen.contains("For everyone") && !screen.contains("finding"),
            "{screen}"
        );
        press(&mut v, &rx, "<down>\n");
        let screen = frame(&mut v);
        assert!(screen.contains("⊘ You deleted this message") && !screen.contains("ela vai adorar"), "{screen}");
        let gone = find_message(&v, "a Sofia tirou mais");
        v.focus = Focus::Message(gone.id.clone());
        press(&mut v, &rx, "d\n");
        assert!(!frame(&mut v).contains("a Sofia tirou mais"), "for you, it goes, at {width}");
    }
}

#[test]
fn someone_elses_message_is_deleted_only_for_you() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<up>d<down>\n");
    let screen = frame(&mut v);
    assert!(screen.contains("only your own") && screen.contains("vens jantar sexta?"), "{screen}");
}

#[test]
fn one_to_six_react_at_once_again_takes_it_away_and_another_replaces_it() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<up>2");
    let mine = |v: &View| newest(v).reactions.iter().filter(|r| r.mine).map(|r| r.emoji.clone()).collect::<Vec<_>>();
    assert_eq!(mine(&v), ["❤️"]);
    assert!(line_with(&frame(&mut v), "❤️").contains("1"));
    press(&mut v, &rx, "1");
    assert_eq!(mine(&v), ["👍"], "one reaction of yours a message");
    press(&mut v, &rx, "1");
    assert!(mine(&v).is_empty() && newest(&v).reactions.is_empty());
}

#[test]
fn plus_opens_the_picker_with_the_quick_six_and_any_emoji_by_name() {
    for width in [140, 100, 80] {
        let (mut v, rx) = anas(width);
        press(&mut v, &rx, "<up>+");
        let screen = frame(&mut v);
        assert!(
            screen.contains(" react ") && screen.contains("thumbs up") && screen.contains("folded hands"),
            "{screen}"
        );
        press(&mut v, &rx, "party");
        let screen = frame(&mut v);
        assert!(screen.contains("partying face") && !screen.contains("thumbs up"), "{screen}");
        press(&mut v, &rx, "\n");
        assert_eq!(newest(&v).reactions.iter().find(|r| r.mine).map(|r| r.emoji.as_str()), Some("🥳"));
    }
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<up>+3");
    assert_eq!(newest(&v).reactions.iter().find(|r| r.mine).map(|r| r.emoji.as_str()), Some("😂"));
}

#[test]
fn f_forwards_to_the_chats_picked_with_tab() {
    for width in [140, 100, 80] {
        let (mut v, rx) = anas(width);
        let asked = newest(&v);
        press(&mut v, &rx, "<up>f");
        let screen = frame(&mut v);
        assert!(screen.contains(" forward ") && screen.contains("tab picks a chat"), "{screen}");
        press(&mut v, &rx, "<down><tab><down><tab>");
        let screen = frame(&mut v);
        assert!(screen.contains("✓ Mãe") && screen.contains("✓ Escola do Bairro"), "{screen}");
        press(&mut v, &rx, "\n");
        assert!(frame(&mut v).contains("forwarded to 2 chats"));
        press(&mut v, &rx, "<esc>");
        v.open_chat("mae");
        run::settle(&mut v, &rx);
        let sent = newest(&v);
        assert!(sent.mine && sent.forwarded == 1 && sent.spans == asked.spans, "{sent:?}");
        assert!(frame(&mut v).contains("↪ forwarded"), "at {width}");
    }
}

#[test]
fn enter_while_forwarding_with_none_picked_forwards_to_the_selected_chat() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<up>f<down>\n");
    assert!(frame(&mut v).contains("forwarded"));
    v.open_chat("mae");
    run::settle(&mut v, &rx);
    assert_eq!(newest(&v).forwarded, 1);
}

#[test]
fn i_shows_who_has_it_and_who_has_read_it_and_any_key_closes_it() {
    for width in [140, 100, 80] {
        let (mut v, rx) = anas(width);
        press(&mut v, &rx, "<up><up>i");
        let screen = frame(&mut v);
        assert!(screen.contains("╭ info ") && screen.contains("delivered") && screen.contains("read"), "{screen}");
        assert!(line_with(&screen, "│ Ana ").contains(':'), "Ana's times:\n{screen}");
        press(&mut v, &rx, "x");
        assert!(!frame(&mut v).contains("╭ info "));
    }
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<up>i");
    assert!(frame(&mut v).contains("Receipts are only for your own messages."));
}

#[test]
fn at_in_a_group_completes_its_members_and_the_message_carries_a_real_mention() {
    for width in [140, 100, 80] {
        let (mut v, rx) = view(width, 40);
        let keys = if width < 90 { "<down><down><down>\n" } else { "<a-down><a-down><a-down>" };
        press(&mut v, &rx, keys);
        assert_eq!(v.open.as_deref(), Some("familia"));
        press(&mut v, &rx, "@In");
        let screen = frame(&mut v);
        assert!(screen.contains(" members ") && screen.contains("Inês Duarte") && screen.contains("··· "), "{screen}");
        press(&mut v, &rx, "<down><tab>vens?\n");
        let sent = newest(&v);
        let mention = sent.spans.iter().find(|s| s.t == SpanKind::Mention).expect("a mention");
        assert_eq!((mention.s.as_str(), mention.person.as_deref()), ("@Inês Duarte", Some("person:8")), "at {width}");
    }
}

#[test]
fn esc_puts_the_names_away_and_a_direct_chat_offers_none() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<a-down><a-down><a-down>@In<esc>");
    assert!(!frame(&mut v).contains(" members ") && v.composer.text() == "@In");
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "@An");
    assert!(!frame(&mut v).contains(" members "));
}

#[test]
fn enter_on_a_message_with_a_link_opens_it() {
    let (mut v, rx) = view(140, 50);
    press(&mut v, &rx, "<a-down><a-down><a-down>");
    let link = find_message(&v, "casadapraia");
    v.focus = Focus::Message(link.id.clone());
    press(&mut v, &rx, "\n");
    assert_eq!(v.take_effects(), [Effect::Open("https://casadapraia.pt/ericeira".into())]);
}

#[test]
fn the_palette_has_the_selected_messages_actions() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<up><up><c-k>edit");
    assert!(frame(&mut v).contains("Edit this message"));
    press(&mut v, &rx, "\n");
    assert!(v.editing.is_some());
    press(&mut v, &rx, "<esc><up><c-k>reply\n");
    assert!(v.replying.is_some(), "Up selects the newest, whose actions the palette offers");
}

#[test]
fn the_messages_keys_are_lit_and_edit_only_where_it_can_be() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<up>");
    let hints = v.hints();
    assert!(hints.contains(&("r", "reply", true)) && hints.contains(&("e", "edit", false)));
    press(&mut v, &rx, "<up>");
    assert!(v.hints().contains(&("e", "edit", true)));
}

#[test]
fn the_picker_finds_emoji_as_it_is_typed_and_the_quick_six_come_back_when_it_is_cleared() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<up>+heart");
    let screen = frame(&mut v);
    assert!(
        screen.contains("red heart") && screen.contains("broken heart") && !screen.contains("thumbs up"),
        "{screen}"
    );
    press(&mut v, &rx, "<c-u>");
    let screen = frame(&mut v);
    assert!(screen.contains("thumbs up") && screen.contains("folded hands"), "{screen}");
}

#[test]
fn a_message_keeps_one_key_from_its_first_letter_through_leaving_its_chat_to_its_send() {
    let (mut v, rx) = view(140, 40);
    let open = v.open.clone().unwrap();
    press(&mut v, &rx, "a cam");
    let key = v.keys[&open].clone();
    assert!(key.starts_with("jaw-") && key.len() == 36, "{key}");
    press(&mut v, &rx, "<a-down><a-up>");
    assert_eq!((v.open.as_deref(), v.composer.text()), (Some(open.as_str()), "a cam"), "its draft came back");
    press(&mut v, &rx, "inho");
    assert_eq!(v.keys[&open], key, "the draft is the same message");
    press(&mut v, &rx, "\n");
    assert!(!v.keys.contains_key(&open), "the key went with it");
    assert!(v.sent.iter().any(|(k, _)| *k == key), "and jawd took it under that key: {:?}", v.sent);
    press(&mut v, &rx, "outra");
    assert_ne!(v.keys[&open], key, "the next message is another");
}

#[test]
fn a_send_whose_answer_was_lost_waits_for_jawd_to_answer_for_it_however_long_that_takes() {
    let (mut v, _rx) = view(140, 40);
    let open = v.open.clone().unwrap();
    v.apply(Update::Unsure { chat: open.clone(), key: "jaw-1".into() });
    let screen = frame(&mut v);
    assert!(screen.contains("asking jawd again once it's back") && v.composer.is_empty(), "{screen}");
    v.clock = Some(NOW + 3_600_000);
    v.tick();
    let screen = frame(&mut v);
    assert!(!screen.contains("not sent") && v.composer.is_empty(), "nothing goes back by itself:\n{screen}");
    v.apply(Update::Sent { key: "jaw-1".into(), message: "3EB0AA".into() });
    assert!(!frame(&mut v).contains("asking jawd again"));
    v.apply(Update::Unsent { chat: open, key: "jaw-1".into(), text: "a caminho".into(), why: "gone".into() });
    let screen = frame(&mut v);
    assert!(v.composer.is_empty() && !screen.contains("not sent"), "jawd has it, so it isn't given back:\n{screen}");
}

#[test]
fn an_action_whose_answer_was_lost_says_so_on_the_box_until_jawd_answers_for_it() {
    let (mut v, _rx) = view(140, 40);
    let open = v.open.clone().unwrap();
    v.apply(Update::Unsure { chat: open.clone(), key: "jaw-react".into() });
    v.apply(Update::Unsure { chat: open, key: "jaw-forward".into() });
    assert!(frame(&mut v).contains("asking jawd again once it's back"));
    v.apply(Update::Acted { key: "jaw-react".into(), said: None });
    assert!(frame(&mut v).contains("asking jawd again once it's back"), "the forward is still unsure");
    v.apply(Update::Acted { key: "jaw-forward".into(), said: Some("forwarded".into()) });
    let screen = frame(&mut v);
    assert!(!screen.contains("asking jawd again") && screen.contains("forwarded"), "{screen}");
}

#[test]
fn a_message_jawd_turned_down_goes_back_with_its_key_ahead_of_what_was_typed_since_or_to_its_chats_draft() {
    let (mut v, rx) = view(140, 40);
    let open = v.open.clone().unwrap();
    v.apply(Update::Unsent { chat: open.clone(), key: "jaw-a".into(), text: "a caminho".into(), why: "full".into() });
    assert_eq!((v.composer.text(), v.keys[&open].as_str()), ("a caminho", "jaw-a"));
    assert!(frame(&mut v).contains("not sent: full"));
    press(&mut v, &rx, "<c-u>já vou");
    let typed = v.keys[&open].clone();
    v.apply(Update::Unsent { chat: open.clone(), key: "jaw-b".into(), text: "chego tarde".into(), why: "full".into() });
    assert_eq!(v.composer.text(), "chego tarde\njá vou", "nothing typed is lost either");
    assert_eq!(v.keys[&open], typed);

    press(&mut v, &rx, "<a-down>");
    let other = v.open.clone().unwrap();
    press(&mut v, &rx, "<a-up>");
    v.apply(Update::Unsent { chat: other.clone(), key: "jaw-c".into(), text: "amanhã".into(), why: "full".into() });
    run::settle(&mut v, &rx);
    let screen = frame(&mut v);
    assert!(screen.contains("back in its box: full") && screen.contains("draft: amanhã"), "{screen}");
    assert_eq!(v.keys[&other], "jaw-c");
    press(&mut v, &rx, "<a-down>");
    assert_eq!(v.composer.text(), "amanhã");
}

#[test]
fn a_failed_message_says_enter_tries_again_and_enter_sends_it_again() {
    for width in [140, 100, 80] {
        let (mut v, rx) = view(width, 40);
        let keys = if width < 90 {
            "<down><down><down><down><down><down>\n"
        } else {
            "<a-down><a-down><a-down><a-down><a-down><a-down>"
        };
        press(&mut v, &rx, keys);
        assert_eq!(v.open.as_deref(), Some("jantar"));
        let failed = find_message(&v, "esqueci-me do pão");
        let screen = frame(&mut v);
        assert!(screen.contains("! not sent · ⏎ tries again"), "at {width}:\n{screen}");
        v.focus = Focus::Message(failed.id.clone());
        press(&mut v, &rx, "\n");
        let again = find_message(&v, "esqueci-me do pão");
        assert_eq!(again.status, Some(crate::data::Tick::Pending), "at {width}");
        assert!(!frame(&mut v).contains("! not sent"));
    }
}

#[test]
fn the_palette_sends_a_failed_message_again() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<a-down><a-down><a-down><a-down><a-down><a-down>");
    let failed = find_message(&v, "esqueci-me do pão");
    v.focus = Focus::Message(failed.id.clone());
    press(&mut v, &rx, "<c-k>send it");
    assert!(frame(&mut v).contains("Send it again"));
    press(&mut v, &rx, "\n");
    assert_eq!(find_message(&v, "esqueci-me do pão").status, Some(crate::data::Tick::Pending));
}

#[test]
fn a_number_typed_in_the_palette_starts_a_chat_with_it() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<c-k>+351 913 777 777");
    let screen = frame(&mut v);
    assert!(screen.contains("Start a chat with +351 913 777 777"), "{screen}");
    press(&mut v, &rx, "\n");
    assert_eq!(v.open.as_deref(), Some("351913777777@s.whatsapp.net"));
    press(&mut v, &rx, "<c-k>12 34");
    assert!(!frame(&mut v).contains("Start a chat with"), "too few digits to be a number");
}

#[test]
fn from_120_columns_the_sidebar_has_the_folders_and_the_roster_beside_the_list() {
    for width in [140, 120] {
        let (mut v, _rx) = view(width, 44);
        let screen = frame(&mut v);
        let first = screen.lines().nth(1).unwrap();
        assert!(first.starts_with("╭ folders ─") && first.contains("╭ chats · All"), "at {width}:\n{screen}");
        for said in ["│   All", "│ ▸ Family", "│ ◇ Waiting on me", "roster · All", "● João"] {
            assert!(screen.contains(said), "{said} at {width}:\n{screen}");
        }
        assert!(line_with(&screen, "▸ Family").contains("@1"), "a mention carried up to the folder:\n{screen}");
    }
}

#[test]
fn below_120_columns_the_sidebar_folds_and_ctrl_b_draws_it_over_the_list() {
    for width in [100, 80] {
        let (mut v, rx) = view(width, 40);
        assert!(!frame(&mut v).contains("╭ folders"), "folded at {width}");
        press(&mut v, &rx, "<c-b>");
        let screen = frame(&mut v);
        assert!(
            screen.contains("╭ folders") && v.hints().contains(&("⏎", "shows its chats", true)),
            "at {width}:\n{screen}"
        );
        press(&mut v, &rx, "<esc>");
        assert!(!frame(&mut v).contains("╭ folders"));
    }
}

#[test]
fn ctrl_b_gives_a_wide_sidebar_the_keys_and_folds_it_and_brings_it_back() {
    let (mut v, rx) = view(140, 40);
    press(&mut v, &rx, "<c-b>");
    assert!(frame(&mut v).contains("ctrl-b folds it"));
    press(&mut v, &rx, "<c-b>");
    let screen = frame(&mut v);
    assert!(!screen.contains("╭ folders") && screen.lines().nth(1).unwrap().starts_with("╭ chats"), "{screen}");
    press(&mut v, &rx, "<c-b>");
    assert!(frame(&mut v).contains("╭ folders"));
}

#[test]
fn picking_a_folder_makes_the_chat_list_its_window_and_the_roster_its_people() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-b><down><down><down><down><down><down>\n");
    assert_eq!(v.folder, "folder:4");
    let screen = frame(&mut v);
    assert!(screen.contains("chats · Work") && screen.contains("roster · Work"), "{screen}");
    assert!(
        screen.contains("Mariana Lopes") && screen.contains("Tiago Fonseca") && !screen.contains("Escola do Bairro"),
        "{screen}"
    );
    assert!(screen.contains("Ana Ribeiro · typing"), "the open chat stays open:\n{screen}");
    press(&mut v, &rx, "<up><up><up><up><up><up>\n");
    assert!(frame(&mut v).contains("chats · All"));
}

#[test]
fn a_folder_opens_to_show_the_folders_in_it_and_counts_each_chat_once() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-b><down><down><down><down><down><right>");
    let screen = frame(&mut v);
    assert!(
        screen.contains("▾ Family") && screen.contains("│     Parents") && screen.contains("│     Cousins"),
        "{screen}"
    );
    let family = v.folders.iter().find(|f| f.name == "Family").unwrap();
    assert_eq!(family.count, 5, "Família, Avó, Mãe, Sofia, and Inês with no chat, each once");
    press(&mut v, &rx, "<left>");
    assert!(!frame(&mut v).contains("Parents"));
}

#[test]
fn the_roster_has_who_is_around_first_then_who_was_seen_last_and_enter_opens_their_chat() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-b><down><down><down><down><down>\n");
    assert_eq!(v.folder, "folder:1");
    let screen = frame(&mut v);
    let at = |name: &str| row_of(&screen, &format!("│ ○ {name}")).or(row_of(&screen, &format!("│ ● {name}")));
    assert!(
        at("Mãe").unwrap() < at("Inês Gomes").unwrap() && at("Inês Gomes").unwrap() < at("Avó Lurdes").unwrap(),
        "{screen}"
    );
    assert!(
        line_with(&screen, "○ Mãe").contains("5 min ago") && line_with(&screen, "○ Avó Lurdes").contains("yesterday")
    );
    press(&mut v, &rx, "<tab>\n");
    assert_eq!(v.open.as_deref(), Some("mae"), "Mãe is the first, and her chat opens");
}

#[test]
fn presence_changing_moves_people_in_the_roster() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-b><down><down><down><down><down>\n");
    let mut row = v.roster.iter().find(|r| r.name == "Inês Gomes").cloned().unwrap();
    row.presence = crate::data::Presence { state: crate::data::PresenceState::Online, at: None, known: true };
    row.sort = "0~inês gomes".into();
    v.apply(Update::Roster { folder: "folder:1".into(), upsert: vec![row], remove: vec![], whole: false });
    let screen = frame(&mut v);
    assert!(row_of(&screen, "● Inês Gomes").unwrap() < row_of(&screen, "○ Mãe").unwrap(), "{screen}");
}

const CLICK: MouseEventKind = MouseEventKind::Down(MouseButton::Left);

/// The mouse at a cell, everything it brings applied.
fn mouse(v: &mut View, rx: &Receiver<Event>, kind: MouseEventKind, (column, row): (u16, u16)) {
    v.on_input(Input::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE }));
    run::settle(v, rx);
}

/// The cell `text` starts on, the first time the screen has it: its column and its row.
fn cell_of(screen: &str, text: &str) -> (u16, u16) {
    let row = row_of(screen, text).unwrap_or_else(|| panic!("no {text:?} on the screen:\n{screen}"));
    let line = screen.lines().nth(row).unwrap_or_default();
    (line[..line.find(text).unwrap_or_default()].width() as u16, row as u16)
}

/// The sidebar's rows its band is on, the selection's colour behind them, as text.
fn banded(v: &mut View) -> Vec<String> {
    let (w, h) = v.size().unwrap();
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| v.draw(f)).unwrap();
    let (buf, selection) = (term.backend().buffer(), v.theme.selection);
    (0..h)
        .filter(|&y| buf[(1, y)].bg == selection)
        .map(|y| (1..sidebar::SIDEBAR - 1).map(|x| buf[(x, y)].symbol()).collect::<String>().trim().to_string())
        .collect()
}

#[test]
fn alt_shift_up_and_down_go_to_the_folder_before_and_after_from_typing_and_stay_at_the_ends() {
    let (mut v, rx) = view(100, 30);
    press(&mut v, &rx, "olá<a-s-down>");
    let screen = frame(&mut v);
    assert!(screen.contains("╭ chats · Unread") && screen.contains("Escola do Bairro"), "{screen}");
    assert!(
        !screen.contains("Pedro & Ana") && !screen.contains("╭ folders"),
        "its chats, the sidebar folded:\n{screen}"
    );
    assert_eq!(
        (v.open.as_deref(), v.focus.clone(), v.composer.text()),
        (Some("ana"), Focus::Typing, "olá"),
        "the chat and the box as they were"
    );
    press(&mut v, &rx, "<a-s-up>");
    assert!(frame(&mut v).contains("╭ chats · All"));
    press(&mut v, &rx, "<a-s-up>");
    assert_eq!(v.folder, "all", "All is the first, and it stays there");
    let mut walked = vec![];
    for _ in 0..14 {
        press(&mut v, &rx, "<a-s-down>");
        walked.push(v.folder.clone());
    }
    assert_eq!(
        walked,
        [
            "unread",
            "groups",
            "communities",
            "archived",
            "folder:1",
            "folder:2",
            "folder:3",
            "folder:4",
            "folder:5",
            "folder:6",
            "smart:1",
            "smart:2",
            "smart:3",
            "smart:3"
        ],
        "the tree's order with every folder open, staying at the last"
    );
    assert!(frame(&mut v).contains("╭ chats · Muted, unread"));
}

#[test]
fn alt_shift_down_to_a_nested_folder_opens_its_parents_and_the_sidebars_band_follows() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<a-s-down><a-s-down><a-s-down><a-s-down><a-s-down>");
    assert!(banded(&mut v).iter().any(|b| b.starts_with("▸ Family")), "{:?}", banded(&mut v));
    press(&mut v, &rx, "<a-s-down>");
    let screen = frame(&mut v);
    assert!(
        screen.contains("▾ Family") && screen.contains("│     Parents") && screen.contains("chats · Parents"),
        "{screen}"
    );
    assert_eq!(banded(&mut v), ["Parents"]);
    press(&mut v, &rx, "<c-b><a-s-down>");
    assert_eq!((v.folder.as_str(), v.sidebar.keys), ("folder:3", Some(sidebar::Part::Folders)));
    assert_eq!(banded(&mut v), ["Cousins"], "with the keys in the sidebar too");
    press(&mut v, &rx, "<down>\n");
    assert_eq!(v.folder, "folder:4", "the sidebar's keys go on from there");
}

#[test]
fn alt_up_and_down_still_step_chats_and_alt_left_and_right_still_move_by_word() {
    let (mut v, rx) = view(100, 30);
    press(&mut v, &rx, "<a-down>");
    assert_eq!((v.open.as_deref(), v.folder.as_str()), (Some("mae"), "all"));
    press(&mut v, &rx, "<a-up>vens jantar sexta<a-left>na <a-right>?");
    assert_eq!((v.open.as_deref(), v.folder.as_str()), (Some("ana"), "all"));
    assert_eq!(v.composer.text(), "vens jantar na sexta?");
}

#[test]
fn a_click_in_the_sidebar_shows_a_folders_chats_opens_it_on_its_mark_and_opens_someones_chat() {
    let (mut v, rx) = view(140, 44);
    let screen = frame(&mut v);
    mouse(&mut v, &rx, CLICK, cell_of(&screen, "Work"));
    assert_eq!(v.folder, "folder:4");
    assert_eq!((v.sidebar.keys, v.focus.clone()), (None, Focus::Typing), "the keys stay in the box");
    let screen = frame(&mut v);
    assert!(screen.contains("╭ chats · Work") && screen.contains("roster · Work"), "{screen}");
    assert_eq!(banded(&mut v), ["Work"]);

    let mark = cell_of(&screen, "▸ Family");
    mouse(&mut v, &rx, CLICK, mark);
    let screen = frame(&mut v);
    assert!(screen.contains("▾ Family") && screen.contains("│     Parents"), "{screen}");
    assert_eq!(v.folder, "folder:4", "the mark opens it instead");
    mouse(&mut v, &rx, CLICK, mark);
    assert!(!frame(&mut v).contains("Parents"), "and closes it");
    mouse(&mut v, &rx, CLICK, (mark.0 + 2, mark.1));
    assert_eq!(v.folder, "folder:1", "its name shows its chats");

    let screen = frame(&mut v);
    mouse(&mut v, &rx, CLICK, cell_of(&screen, "○ Mãe"));
    assert_eq!((v.open.as_deref(), v.sidebar.keys), (Some("mae"), None));
    let screen = frame(&mut v);
    mouse(&mut v, &rx, CLICK, cell_of(&screen, "Inês Gomes"));
    assert_eq!(v.starting.as_deref(), Some("person:4"), "someone without a chat: one begins");
    assert!(frame(&mut v).contains("a chat with Inês Gomes begins"));
}

#[test]
fn the_wheel_over_the_sidebar_moves_nothing_as_nothing_in_it_scrolls() {
    let (mut v, rx) = view(140, 44);
    let before = frame(&mut v);
    for text in ["▸ Family", "● João"] {
        mouse(&mut v, &rx, MouseEventKind::ScrollDown, cell_of(&before, text));
    }
    assert_eq!(frame(&mut v), before);

    let (mut v, rx) = view(100, 30);
    press(&mut v, &rx, "<c-b>");
    let before = frame(&mut v);
    for _ in 0..3 {
        mouse(&mut v, &rx, MouseEventKind::ScrollDown, cell_of(&before, "▸ Family"));
    }
    assert_eq!(v.list_top, 0, "the chat list under it stays where it was");
    assert_eq!(frame(&mut v), before);
    press(&mut v, &rx, "<esc>");
    mouse(&mut v, &rx, MouseEventKind::ScrollDown, cell_of(&before, "▸ Family"));
    assert_eq!(v.list_top, 1, "where the list scrolls once the sidebar's gone");
}

#[test]
fn below_120_columns_a_click_in_the_sidebar_over_the_list_acts_as_enter_and_closes_it() {
    for width in [100, 80] {
        let (mut v, rx) = view(width, 40);
        press(&mut v, &rx, "<c-b>");
        let screen = frame(&mut v);
        mouse(&mut v, &rx, CLICK, cell_of(&screen, "▸ Family"));
        let screen = frame(&mut v);
        assert!(
            screen.contains("▾ Family") && screen.contains("Parents") && v.sidebar.keys.is_some(),
            "at {width}, a mark leaves it open:\n{screen}"
        );
        mouse(&mut v, &rx, CLICK, cell_of(&screen, "Parents"));
        assert_eq!((v.folder.as_str(), v.sidebar.keys), ("folder:2", None), "at {width}");
        let screen = frame(&mut v);
        assert!(!screen.contains("╭ folders") && screen.contains("╭ chats · Parents"), "at {width}:\n{screen}");
        assert_eq!(v.open.as_deref(), Some("ana"), "at {width}, nothing under the sidebar was clicked");

        press(&mut v, &rx, "<c-b>");
        let screen = frame(&mut v);
        mouse(&mut v, &rx, CLICK, cell_of(&screen, "○ Mãe"));
        assert_eq!((v.open.as_deref(), v.sidebar.keys), (Some("mae"), None), "at {width}");
        assert!(!frame(&mut v).contains("╭ folders"), "at {width}");
    }
}

#[test]
fn a_click_while_the_sidebar_has_the_keys_moves_its_band_and_its_keys_go_on_from_there() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-b>");
    let screen = frame(&mut v);
    mouse(&mut v, &rx, CLICK, cell_of(&screen, "Work"));
    assert_eq!((v.folder.as_str(), v.sidebar.keys), ("folder:4", Some(sidebar::Part::Folders)));
    assert_eq!(banded(&mut v), ["Work"]);
    mouse(&mut v, &rx, CLICK, cell_of(&screen, "▸ Family"));
    let band = banded(&mut v);
    assert!(band.len() == 1 && band[0].starts_with("▾ Family") && v.folder == "folder:4", "{band:?}");
    press(&mut v, &rx, "<down>\n");
    assert_eq!(v.folder, "folder:2", "down from Family is Parents");
}

#[test]
fn below_120_columns_the_folder_key_leaves_the_sidebar_over_the_list_open_and_its_band_follows() {
    let (mut v, rx) = view(100, 30);
    press(&mut v, &rx, "<c-b><a-s-down><a-s-down>");
    assert_eq!((v.folder.as_str(), v.sidebar.keys), ("groups", Some(sidebar::Part::Folders)));
    assert!(frame(&mut v).contains("╭ folders"));
    let band = banded(&mut v);
    assert!(band.len() == 1 && band[0].starts_with("Groups"), "{band:?}");
}

#[test]
fn alt_down_in_a_folder_without_the_open_chat_opens_its_first_chat() {
    for (width, height, keys) in [(100, 30, "<a-s-down>"), (140, 44, "<c-b><down>\n<esc>")] {
        let (mut v, rx) = view(width, height);
        press(&mut v, &rx, keys);
        let listed = v.list_order();
        assert!(v.folder == "unread" && !listed.iter().any(|c| c == "ana"), "after {keys}: {listed:?}");
        press(&mut v, &rx, "<a-down>");
        assert_eq!(v.open.as_ref(), listed.first(), "after {keys}");
    }
    let (mut v, rx) = view(80, 24);
    press(&mut v, &rx, "<a-s-down><a-s-down>");
    let listed = v.list_order();
    assert!(v.folder == "groups" && !listed.iter().any(|c| c == "ana"), "{listed:?}");
    press(&mut v, &rx, "<down>");
    assert_eq!(v.selected.as_ref(), listed.first(), "and Down in the narrow list");
}

#[test]
fn while_jawd_is_out_of_reach_a_click_in_the_sidebar_does_nothing_as_keys_do() {
    let (mut v, _rx) = view(140, 44);
    v.apply(Update::Unreachable { why: "it stopped".into(), retry_in: 4_000 });
    let screen = frame(&mut v);
    assert!(screen.contains("can't reach jawd") && screen.contains("╭ chats · All"), "{screen}");
    for text in ["Work", "▸ Family", "● João"] {
        let (column, row) = cell_of(&screen, text);
        v.on_input(Input::Mouse(MouseEvent { kind: CLICK, column, row, modifiers: KeyModifiers::NONE }));
    }
    assert_eq!((v.folder.as_str(), v.open.as_deref(), v.starting.as_deref()), ("all", Some("ana"), None));
    assert!(v.sidebar.open.is_empty());
    let screen = frame(&mut v);
    assert!(screen.contains("╭ chats · All") && !screen.contains("jawd isn't answering"), "{screen}");
}

#[test]
fn with_neither_the_list_nor_the_sidebar_drawn_the_folder_key_names_the_folder_in_the_footer() {
    let (mut v, rx) = view(80, 24);
    press(&mut v, &rx, "<down>\n<a-s-down>");
    let screen = frame(&mut v);
    assert!(screen.lines().last().unwrap_or_default().contains("Unread: esc shows its chats"), "{screen}");
    press(&mut v, &rx, "<esc>");
    assert!(frame(&mut v).contains("╭ chats · Unread"));
    for (width, keys) in [(80, "<a-s-down>"), (100, "<a-s-down>"), (80, "<down>\n<c-b><a-s-down>")] {
        let (mut v, rx) = view(width, 24);
        press(&mut v, &rx, keys);
        assert!(v.folder == "unread" && v.notice.is_none(), "at {width} after {keys}: {:?}", v.notice);
    }
}

#[test]
fn below_90_columns_the_folder_keys_notice_names_esc_only_where_esc_shows_the_chat_list_next() {
    for (state, keys, said) in [
        ("typing", "\n", "Unread: esc shows its chats"),
        ("a message selected", "\n<up>", "Unread"),
        ("an edit open", "\n<up><up>e", "Unread"),
        ("the names showing", "<down><down><down>\n@In", "Unread"),
        ("a reply open", "\n<up>r", "Unread"),
    ] {
        let (mut v, rx) = view(80, 40);
        press(&mut v, &rx, keys);
        press(&mut v, &rx, "<a-s-down>");
        let notice = v.notice.clone().map(|(n, _)| n);
        assert_eq!(notice.as_deref(), Some(said), "{state}:\n{}", frame(&mut v));
        press(&mut v, &rx, "<esc>");
        assert_eq!(v.picking, said.contains("esc"), "{state}: Esc shows the list next only where it's named");
    }
}

#[test]
fn m_files_the_open_chat_into_folders_ticked_with_space() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<up>m");
    let screen = frame(&mut v);
    assert!(
        screen.contains(" file ") && screen.contains("[ ] Family") && screen.contains("[x] Ana's 40th"),
        "{screen}"
    );
    press(&mut v, &rx, " <down><down> \n");
    let ana = v.chats.get("ana").unwrap();
    assert!(ana.folders.contains(&"folder:1".to_string()) && ana.folders.contains(&"folder:3".to_string()), "{ana:?}");
    assert!(frame(&mut v).contains("filed in Family, Cousins"));
}

#[test]
fn n_while_filing_makes_a_new_folder_and_ticks_it() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<up>mnTrabalho\n");
    assert!(frame(&mut v).contains("[x] Trabalho"));
    press(&mut v, &rx, "\n");
    let made = v.folders.iter().find(|f| f.name == "Trabalho").unwrap().id.clone();
    assert!(v.chats.get("ana").unwrap().folders.contains(&made));
}

#[test]
fn folders_are_made_renamed_nested_and_deleted_from_the_palette() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-k>new folder\nTeatro\n");
    let teatro = v.folders.iter().find(|f| f.name == "Teatro").expect("made").id.clone();
    v.pick_folder(&teatro);
    run::settle(&mut v, &rx);
    press(&mut v, &rx, "<c-k>rename teatro\n<c-u>Teatro e cinema\n");
    assert!(v.folders.iter().any(|f| f.name == "Teatro e cinema"));
    press(&mut v, &rx, "<c-k>move teatro\n");
    let screen = frame(&mut v);
    assert!(screen.contains(" move ") && screen.contains("The top") && !screen.contains("    Teatro"), "{screen}");
    press(&mut v, &rx, "<down>\n");
    let row = v.folders.iter().find(|f| f.id == teatro).unwrap();
    assert_eq!(row.parent.as_deref(), Some("folder:1"), "nested in Family");
    press(&mut v, &rx, "<c-k>delete the folder\n");
    assert!(!v.folders.iter().any(|f| f.id == teatro));
    assert_eq!(v.folder, "all", "its list goes back to All");
}

#[test]
fn a_community_is_one_row_that_opens_to_its_groups_announcements_first() {
    let (mut v, rx) = view(80, 60);
    let screen = frame(&mut v);
    assert!(screen.contains("▸ Bairro de Alvalade") && !screen.contains("Festa de Natal"), "{screen}");
    for _ in 0..15 {
        press(&mut v, &rx, "<down>");
    }
    assert_eq!(v.selected.as_deref(), Some("bairro"));
    press(&mut v, &rx, "\n");
    let screen = frame(&mut v);
    let (announce, festa) = (row_of(&screen, "Avisos do Bairro").unwrap(), row_of(&screen, "Festa de Natal").unwrap());
    assert!(screen.contains("▾ Bairro de Alvalade") && announce < festa, "{screen}");
    assert!(line_with(&screen, "Festa de Natal").contains("│     Festa"), "indented under it:\n{screen}");
}

fn library_rows(v: &View) -> Vec<crate::data::LibraryRow> {
    v.library.as_ref().map(|l| l.rows.clone()).unwrap_or_default()
}

fn selected_item(v: &View) -> crate::data::LibraryRow {
    let l = v.library.as_ref().unwrap();
    l.rows.iter().find(|r| Some(&r.id) == l.at.as_ref()).cloned().unwrap()
}

#[test]
fn ctrl_l_opens_the_open_chats_library_in_its_pane_then_all_chats_over_the_screen() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-l>");
    let screen = frame(&mut v);
    assert!(screen.contains("╭ library · Ana Ribeiro") && screen.contains("╭ chats · All"), "{screen}");
    assert!(screen.contains("esc back to the conversation"));
    press(&mut v, &rx, "<c-l>");
    let screen = frame(&mut v);
    assert!(screen.contains("╭ library · all chats") && !screen.contains("╭ chats"), "the whole screen:\n{screen}");
    assert!(
        screen.contains("photos 79") && screen.contains("photos and videos · 80") && screen.contains("links · 26"),
        "{screen}"
    );
    press(&mut v, &rx, "<esc>");
    assert!(v.library.is_none() && frame(&mut v).contains("╭ Ana Ribeiro"));
}

#[test]
fn across_all_chats_each_kind_shows_at_140_100_and_80_columns() {
    for width in [140, 100, 80] {
        let (mut v, rx) = view(width, 44);
        press(&mut v, &rx, "<c-l><c-l>");
        if width < 90 {
            assert!(
                v.library.as_ref().is_some_and(|l| l.chat.is_none()),
                "no chat open at {width}, so all chats at once"
            );
        }
        let per_row = ((width - 4 + 2) / 18) as usize;
        let screen = frame(&mut v);
        let captions = line_with(&screen, "Ana · ");
        assert_eq!(captions.matches(" · ").count(), per_row, "{per_row} previews a row at {width}:\n{screen}");
        press(&mut v, &rx, "<tab>");
        assert!(frame(&mut v).contains("October 2026 ─"), "photos under their months at {width}");
        press(&mut v, &rx, "<tab><tab>");
        assert!(frame(&mut v).contains("0:14"), "voice at {width}");
        press(&mut v, &rx, "<tab>");
        let screen = frame(&mut v);
        assert!(
            screen.contains("▣ orcamento-obras-cozinha.pdf") && screen.contains("PDF · 2.4 MB · 3 pages"),
            "files at {width}:\n{screen}"
        );
        press(&mut v, &rx, "<tab>");
        let screen = frame(&mut v);
        assert!(
            screen.contains("↗ Climbing in Sintra: the complete guide") && screen.contains("thecrag.com"),
            "links at {width}:\n{screen}"
        );
        assert!(
            screen.contains("Every crag around Sintra"),
            "the selected link opens out to its description at {width}"
        );
    }
}

#[test]
fn arrows_move_across_the_grid_and_down_to_the_nearest_column() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-l><c-l><tab>");
    let first = selected_item(&v).id;
    press(&mut v, &rx, "<right><right>");
    let rows = library_rows(&v);
    assert_eq!(selected_item(&v).id, rows[2].id);
    press(&mut v, &rx, "<down>");
    assert_eq!(selected_item(&v).id, rows[9].id, "seven a row: the one under it");
    press(&mut v, &rx, "<up><left><left>");
    assert_eq!(selected_item(&v).id, first);
}

#[test]
fn filters_are_typed_with_completion_and_shown_as_chips_backspace_takes_away() {
    let (mut v, rx) = view(100, 44);
    press(&mut v, &rx, "<c-l><c-l>from:Le");
    let screen = frame(&mut v);
    assert!(screen.contains("┆ Leonor") && screen.contains("tab takes it"), "{screen}");
    press(&mut v, &rx, "<tab>");
    let screen = frame(&mut v);
    assert!(
        screen.contains(" from: Leonor ")
            && library_rows(&v).iter().all(|r| r.from.as_ref().is_some_and(|f| f.name == "Leonor")),
        "{screen}"
    );
    press(&mut v, &rx, "in:Lisb<tab>since:a mon<tab>");
    let screen = frame(&mut v);
    assert!(screen.contains(" in: Lisbon Climbing ") && screen.contains(" since: a month ago "), "{screen}");
    let since = NOW - 31 * crate::fmt::DAY;
    assert!(library_rows(&v).iter().all(|r| r.at >= since && r.chat == "climbing"));
    press(&mut v, &rx, "<backspace>");
    assert!(!frame(&mut v).contains("since:"), "Backspace on an empty search takes the last chip away");
}

#[test]
fn words_search_captions_file_names_and_link_titles() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-l><c-l>sintra");
    let rows = library_rows(&v);
    assert!(rows.iter().any(|r| r.kind == crate::data::LibraryKind::Photo));
    assert!(rows.iter().any(|r| r.kind == crate::data::LibraryKind::File));
    assert!(rows.iter().any(|r| r.kind == crate::data::LibraryKind::Link));
    let photo = rows.iter().find(|r| r.kind == crate::data::LibraryKind::Photo).unwrap();
    assert!(photo.caption.iter().any(|m| m.t == crate::data::MarkKind::Mark && m.s == "Sintra"), "{photo:?}");
}

#[test]
fn enter_opens_a_link_in_the_browser_a_file_in_its_app_and_plays_a_voice_message() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-l><c-l><tab><tab><tab><tab><tab>");
    press(&mut v, &rx, "\n");
    assert_eq!(v.take_effects(), [Effect::Open("https://thecrag.com/en/climbing/portugal/sintra".into())]);
    press(&mut v, &rx, "<s-tab>\n");
    let opened = v.take_effects();
    assert!(matches!(&opened[..], [Effect::Open(p)] if p.ends_with(".pdf") || p.ends_with(".gpx")), "{opened:?}");
    press(&mut v, &rx, "<s-tab>\n");
    assert!(v.playing.is_some(), "a voice message plays in place");
    assert!(frame(&mut v).contains("⏸ "));
}

#[test]
fn enter_on_a_photo_shows_it_full_size_and_arrows_go_to_the_ones_beside_it() {
    let (mut v, rx) = pictured(Protocol::Blocks, 140, 44);
    press(&mut v, &rx, "<c-l><c-l><tab>\n");
    let screen = frame(&mut v);
    let first = selected_item(&v).id;
    assert!(screen.contains("in Ana Ribeiro ·") && screen.matches('▀').count() > 1_000, "full size:\n{screen}");
    press(&mut v, &rx, "<right>");
    assert_ne!(selected_item(&v).id, first);
    press(&mut v, &rx, "x");
    assert!(!v.library.as_ref().unwrap().viewing, "any other key goes back to the grid");
}

#[test]
fn ctrl_g_goes_to_the_items_message_in_its_chat() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-l><c-l><tab><tab><tab><tab>");
    let item = selected_item(&v);
    press(&mut v, &rx, "<c-g>");
    assert!(v.library.is_none());
    assert_eq!(v.open.as_deref(), Some(item.chat.as_str()));
    assert_eq!(v.focus, Focus::Message(item.message.clone()), "selected, there");
}

#[test]
fn moving_near_the_end_of_a_kind_with_more_asks_for_the_next_page() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-l><c-l><tab>");
    let asked = v.library.as_ref().unwrap().asked;
    let mut rows = library_rows(&v);
    rows.truncate(14);
    v.apply(Update::Library {
        asked,
        window: crate::data::LibraryWindow { upsert: rows, whole: true, older: Some(true), ..Default::default() },
    });
    press(&mut v, &rx, "<down>");
    let l = v.library.as_ref().unwrap();
    assert!(l.paging.is_none() || l.rows.len() > 14, "the next page was asked for and came: {} rows", l.rows.len());
    assert!(l.rows.len() > 14);
}

#[test]
fn what_arrives_while_the_library_is_open_comes_into_it() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-l><c-l><tab>");
    let before = library_rows(&v).len();
    let asked = v.library.as_ref().unwrap().asked;
    let mut new = library_rows(&v)[0].clone();
    new.id = "new-photo".into();
    new.sort = "9999999999999~new".into();
    v.apply(Update::Library { asked, window: crate::data::LibraryWindow { upsert: vec![new], ..Default::default() } });
    let rows = library_rows(&v);
    assert_eq!((rows.len(), rows[0].id.as_str()), (before + 1, "new-photo"), "newest first");
    drop(rx);
}

#[test]
fn the_palette_opens_the_library() {
    let (mut v, rx) = view(140, 44);
    press(&mut v, &rx, "<c-k>library\n");
    assert!(v.library.as_ref().is_some_and(|l| l.chat.is_none()));
}
