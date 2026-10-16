//! Frames as text at several sizes, keys driving the view, and anchoring as rows arrive.

use super::*;
use crate::data::{Chat, Hit, Kind, Media, Sender, Span, SpanKind};
use crate::demo::{Clock, Demo, State};
use crate::graphics::{Images, Protocol};
use crate::run::{self, Event};
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
    assert_eq!(at, 34 + 2 + 102 / 3, "yours start a third of the way across the conversation:\n{mine}");
    assert!(mine.ends_with("✓✓  23:12 │"), "{mine}");
    assert!(screen.contains("Yesterday") && screen.contains("Today"), "day separators:\n{screen}");
    assert!(screen.lines().last().unwrap().starts_with(" ⏎ send  alt-⏎ new line  ↑ messages"), "{screen}");
    assert!(screen.lines().all(|l| crate::fmt::width(l) <= 140));
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
fn logged_out_says_so_above_linking_again() {
    let (mut v, rx) = view_in(State::LoggedOut, 100, 36);
    let screen = frame(&mut v);
    assert!(screen.contains("jaw  logged out from the phone") && screen.contains("kept your messages"), "{screen}");
    assert!(!screen.contains("It changes in"), "{screen}");
    press(&mut v, &rx, "\n");
    let screen = frame(&mut v);
    assert!(screen.contains("logged out") && screen.contains("It changes in 20 s."), "{screen}");
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
    assert!(frame(&mut v).contains("folders come with M6"));
    press(&mut v, &rx, "<c-u>new poll\n");
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
    use ratatui::backend::TestBackend;
    use ratatui::buffer::CellDiffOption;
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
fn a_send_whose_answer_was_lost_waits_for_its_row_rather_than_saying_not_sent() {
    let (mut v, _rx) = view(140, 40);
    v.apply(Update::Unsure { chat: "ana".into(), text: "a caminho".into() });
    assert!(!frame(&mut v).contains("not sent"));
    let mut row = message("lost-1", 1, "9999999999990~000000001");
    row.chat = "ana".into();
    row.mine = true;
    row.status = Some(crate::data::Tick::Pending);
    row.spans = vec![Span { t: SpanKind::Text, s: "a caminho".into(), person: None, url: None }];
    arrive(&mut v, "ana", vec![row]);
    v.clock = Some(NOW + 60_000);
    v.tick();
    let screen = frame(&mut v);
    assert!(!screen.contains("not sent") && v.composer.is_empty(), "its row came, so it went:\n{screen}");
}

#[test]
fn a_lost_send_with_no_row_once_jawd_is_back_goes_back_to_the_box_as_not_sent() {
    let (mut v, _rx) = view(140, 40);
    v.apply(Update::Unreachable { why: "gone".into(), retry_in: 1_000 });
    v.apply(Update::Unsure { chat: "ana".into(), text: "a caminho".into() });
    v.clock = Some(NOW + 60_000);
    v.tick();
    assert!(v.composer.is_empty(), "while jawd is away, it waits");
    v.apply(Update::Hello { root: String::new(), linked: true, me: vec![], connection: v.connection.clone() });
    assert!(v.next_wake() <= NOW + 60_000 + 8_000);
    v.clock = Some(NOW + 70_000);
    v.tick();
    let screen = frame(&mut v);
    assert!(v.composer.text() == "a caminho" && screen.contains("didn't arrive"), "{screen}");
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
