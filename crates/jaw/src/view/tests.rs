//! Frames as text at several sizes, keys driving the view, and anchoring as rows arrive.

use super::*;
use crate::data::{Chat, Hit, Kind, Sender, Span, SpanKind};
use crate::demo::{Clock, Demo, State};
use crate::run::{self, Event};
use ratatui::crossterm::event::{
    Event as Input, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::sync::mpsc::Receiver;

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
        reactions: vec![],
        status: None,
        edited: false,
        deleted: false,
        summary: None,
        system: None,
    }
}

fn theirs(id: &str, sort: &str, at: i64, text: &str) -> Message {
    let mut m = message(id, 1, sort);
    m.at = at;
    m.from = Some(Sender { id: "person:20".into(), name: "Marta".into(), colour: 0 });
    m.spans = vec![Span { t: SpanKind::Text, s: text.into(), person: None }];
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
    press(&mut v, &rx, "r");
    assert!(frame(&mut v).contains("replying comes with M2"), "a key for later says when it comes");
    press(&mut v, &rx, "<up><up>\n");
    assert!(frame(&mut v).contains("playing voice messages comes with M5"));
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
        screen.contains("╭ keys ─") && screen.contains("reply · M2") && screen.contains("any key closes"),
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
