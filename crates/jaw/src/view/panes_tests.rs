//! Split panes: with the switch on and one pane the screen is as it was but for its key; Ctrl-P
//! splits, gives a pane the keys, swaps, resizes and closes; each pane keeps its chat's window and
//! its box; closing lets its window go; and the status line marks the pane with the keys.

use super::panes::{KEEP_AFTER, MOST};
use super::*;
use crate::demo::{Clock, Demo, State};
use crate::experiments::Experiments;
use crate::run::{self, Event};
use jaw_proto::layout as saved;
use ratatui::crossterm::event::{Event as Input, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::sync::mpsc::Receiver;

const NOW: i64 = super::tests::NOW;
const UTC: FixedOffset = FixedOffset::east_opt(0).unwrap();

fn view_with(on: &str, width: u16, height: u16) -> (View, Receiver<Event>, Demo) {
    view_kept(on, width, height, None)
}

/// jaw with the experiments `on`, on the made-up account keeping `layout` for it, as jawd does.
fn view_kept(on: &str, width: u16, height: u16, layout: Option<(saved::Layout, u64)>) -> (View, Receiver<Event>, Demo) {
    let (tx, rx) = std::sync::mpsc::channel();
    let demo = Demo::start(tx, Clock::Fixed(NOW), State::Online, false);
    if let Some((layout, v)) = layout {
        demo.keeps_layout(layout, v);
    }
    let mut v = View::new(Box::new(demo.clone()));
    v.experiments = Experiments::parse(on);
    v.clock = Some(NOW);
    v.tz = UTC;
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

fn press(v: &mut View, rx: &Receiver<Event>, keys: &str) {
    run::press(v, rx, keys).unwrap();
}

/// Four panes: Ana's split across, Mãe beside it split down with Escola below, then Ana's split
/// down with Família below it, which has the keys.
const FOUR: &str = "<c-p>%<c-p>\"<c-p>1<c-p>\"";

/// What pane `n` (from 1) shows, by the open chat or its parked state.
fn shows(v: &View, n: usize) -> Option<String> {
    let p = v.panes_in_order().get(n - 1).copied()?;
    if p == v.pane_with_keys() { v.open.clone() } else { v.parked_chat(p) }
}

/// The view's clock `ms` on, and what's come due then done, as the loop does after each key.
fn later(v: &mut View, rx: &Receiver<Event>, ms: i64) {
    v.clock = v.clock.map(|now| now + ms);
    v.tick();
    run::settle(v, rx);
}

fn layout(focused: u32, tree: saved::Node) -> saved::Layout {
    saved::Layout { format: saved::FORMAT, focused, tree }
}

fn pane(chat: &str) -> saved::Node {
    saved::Node::Chat(chat.into())
}

fn halves(how: saved::How, first: u16, a: saved::Node, b: saved::Node) -> saved::Node {
    saved::Node::Split(Box::new(saved::Split { how, first, a, b }))
}

#[test]
fn with_panes_jaw_opens_with_the_panes_jawd_keeps_and_the_keys_where_they_were() {
    use saved::How::{Across, Down};
    let three = layout(2, halves(Across, 600, pane("ana"), halves(Down, 400, pane("familia"), pane("escola"))));
    let (mut v, rx, demo) = view_kept("panes", 160, 45, Some((three, 5)));
    let order: Vec<Option<String>> = (1..=3).map(|n| shows(&v, n)).collect();
    assert_eq!(order, ["ana", "familia", "escola"].map(|c| Some(c.to_string())).to_vec());
    assert_eq!(v.open.as_deref(), Some("escola"), "the keys where they were");
    let mut watched = demo.windows_watched();
    watched.sort();
    assert_eq!(watched, ["ana", "escola", "familia"], "a window for each pane");
    let screen = frame(&mut v);
    let top = screen.lines().nth(1).unwrap();
    let column = |title: &str| top[..top.find(title).expect(&screen)].chars().count();
    let (first, second) = (column("╭ 1"), column("╭ 2"));
    let share = (second - first) as f64 / (top.chars().count() - first) as f64;
    assert!((0.55..0.65).contains(&share), "Ana's pane takes 60% across: {share}\n{screen}");
    later(&mut v, &rx, 0);
    later(&mut v, &rx, 3 * KEEP_AFTER);
    assert_eq!(demo.layout_kept().1, 1, "asked for once");
    assert_eq!(demo.layout_kept().2, 0, "and not kept again, being as jawd keeps it");
}

#[test]
fn without_panes_jaw_neither_asks_for_the_layout_nor_opens_it() {
    let two = layout(1, halves(saved::How::Across, 500, pane("ana"), pane("mae")));
    let (mut v, rx, demo) = view_kept("", 160, 45, Some((two, 5)));
    let (mut plain, _rx, _) = view_with("", 160, 45);
    assert_eq!(frame(&mut v), frame(&mut plain), "the screen as ever");
    press(&mut v, &rx, "<c-p>%");
    later(&mut v, &rx, 0);
    later(&mut v, &rx, KEEP_AFTER);
    assert_eq!(demo.layout_kept().1, 0, "not asked for");
    assert_eq!(demo.layout_kept().2, 0, "nor kept");
}

#[test]
fn the_layout_is_kept_once_it_has_stayed_as_it_is_for_two_seconds_never_at_each_key() {
    let (mut v, rx, demo) = view_with("panes", 160, 45);
    press(&mut v, &rx, "<c-p>%");
    later(&mut v, &rx, 0);
    // A key every half second: the divider three steps left, then the keys to pane 1 and back.
    for keys in ["<c-p><a-left>", "<a-left>", "<a-left>", "<c-p>1", "<c-p>2"] {
        later(&mut v, &rx, KEEP_AFTER / 4);
        press(&mut v, &rx, keys);
        later(&mut v, &rx, 0);
    }
    assert_eq!(demo.layout_kept().2, 0, "nothing kept while it changes");
    later(&mut v, &rx, KEEP_AFTER - 1);
    assert_eq!(demo.layout_kept().2, 0, "nor before it has stayed so for two seconds");
    later(&mut v, &rx, 1);
    let (kept, _, keeps) = demo.layout_kept();
    assert_eq!(keeps, 1, "kept once");
    let (kept, version) = kept.unwrap();
    assert_eq!(kept, layout(1, halves(saved::How::Across, 350, pane("ana"), pane("mae"))));
    assert_eq!(version, (NOW + 5 * (KEEP_AFTER / 4) + KEEP_AFTER) as u64, "the time it was kept, in milliseconds");
    later(&mut v, &rx, 10 * KEEP_AFTER);
    assert_eq!(demo.layout_kept().2, 1, "and not again while nothing changes");
}

#[test]
fn going_from_chat_to_chat_in_one_pane_keeps_nothing_and_closing_the_split_keeps_that_pane() {
    let (mut v, rx, demo) = view_with("panes", 160, 45);
    for _ in 0..3 {
        press(&mut v, &rx, "<a-down>");
        later(&mut v, &rx, 0);
        later(&mut v, &rx, KEEP_AFTER);
    }
    assert_eq!(demo.layout_kept().2, 0, "one pane, nothing to keep");
    press(&mut v, &rx, "<c-p>%");
    later(&mut v, &rx, 0);
    later(&mut v, &rx, KEEP_AFTER);
    assert_eq!(demo.layout_kept().2, 1);
    press(&mut v, &rx, "<c-p>x");
    later(&mut v, &rx, 0);
    later(&mut v, &rx, KEEP_AFTER);
    let (kept, _, keeps) = demo.layout_kept();
    let open = v.open.clone().unwrap();
    assert_eq!((keeps, kept.unwrap().0), (2, layout(0, pane(&open))), "the pane left, so the split won't open again");
    press(&mut v, &rx, "<a-down>");
    later(&mut v, &rx, 0);
    later(&mut v, &rx, KEEP_AFTER);
    assert_eq!(demo.layout_kept().2, 2, "and going on in one pane keeps nothing");
}

#[test]
fn a_layout_kept_is_newer_than_any_jawd_has_said_it_keeps_and_another_jaws_only_moves_that_on() {
    let ahead = NOW as u64 + 60_000;
    let (mut v, rx, demo) = view_kept("panes", 160, 45, Some((layout(0, pane("ana")), ahead)));
    press(&mut v, &rx, "<c-p>%");
    later(&mut v, &rx, 0);
    later(&mut v, &rx, KEEP_AFTER);
    let (kept, _, keeps) = demo.layout_kept();
    assert_eq!(
        (keeps, kept.map(|(_, v)| v)),
        (1, Some(ahead + 1)),
        "one past jawd's, which a clock ahead of jaw's kept"
    );
    let theirs = saved::Saved {
        interface: "jaw".into(),
        v: ahead + 100,
        layout: Some(layout(0, pane("escola"))),
        kept: None,
        dropped: vec![],
    };
    v.apply(Update::LayoutKept(Box::new(theirs)));
    later(&mut v, &rx, 0);
    later(&mut v, &rx, 3 * KEEP_AFTER);
    assert_eq!((shows(&v, 1).as_deref(), shows(&v, 2).as_deref()), (Some("ana"), Some("mae")), "the screen as it was");
    assert_eq!(demo.layout_kept().2, 1, "and theirs not answered with ours again");
    press(&mut v, &rx, "<c-p>1");
    later(&mut v, &rx, 0);
    later(&mut v, &rx, KEEP_AFTER);
    let (kept, _, keeps) = demo.layout_kept();
    assert_eq!((keeps, kept.map(|(_, v)| v)), (2, Some(ahead + 101)), "past theirs");
}

#[test]
fn a_kept_layout_of_one_pane_or_of_more_than_jaw_shows_opens_no_panes() {
    let (one, _rx, _) = view_kept("panes", 160, 45, Some((layout(0, pane("escola")), 5)));
    assert_eq!((one.panes_in_order().len(), one.open.as_deref()), (0, Some("ana")), "opened as without panes");
    let chats = ["ana", "mae", "familia", "escola", "climbing", "tiago", "sofia"];
    let seven = chats[1..].iter().fold(pane(chats[0]), |tree, chat| halves(saved::How::Down, 500, tree, pane(chat)));
    let (many, _rx, _) = view_kept("panes", 160, 45, Some((layout(0, seven), 5)));
    assert_eq!(many.panes_in_order().len(), 0, "seven panes, past the six jaw shows");
}

#[test]
fn with_the_switch_on_and_one_pane_the_screen_is_as_it_was_but_for_its_key() {
    let (mut off, _rx, _) = view_with("", 160, 45);
    let (mut on, _rx2, _) = view_with("panes", 160, 45);
    let (was, now) = (frame(&mut off), frame(&mut on));
    let (was, now): (Vec<&str>, Vec<&str>) = (was.lines().collect(), now.lines().collect());
    assert_eq!(was[..was.len() - 1], now[..now.len() - 1], "every row but the keys'");
    assert!(now.last().unwrap().contains("ctrl-p panes"), "{}", now.last().unwrap());
}

#[test]
fn ctrl_p_splits_across_and_down_each_pane_with_its_own_chat_and_window() {
    let (mut v, rx, demo) = view_with("panes", 160, 45);
    assert_eq!(demo.windows_watched(), ["ana"]);
    press(&mut v, &rx, "<c-p>%");
    let screen = frame(&mut v);
    let top = screen.lines().nth(1).unwrap();
    assert!(top.contains("╭ 1 Ana Ribeiro · typing…") && top.contains("╭ 2  Mãe ─"), "{screen}");
    assert_eq!(v.open.as_deref(), Some("mae"), "the new pane has the keys, with the next chat");
    assert_eq!(demo.windows_watched(), ["mae", "ana"], "Ana's window kept for her pane");
    assert!(screen.contains("vens jantar sexta?") && screen.contains("liga-me quando puderes"), "{screen}");
    press(&mut v, &rx, "<c-p>\"<c-p>1<c-p>\"");
    let screen = frame(&mut v);
    let order: Vec<Option<String>> = (1..=4).map(|n| shows(&v, n)).collect();
    let names = |ids: [&str; 4]| ids.map(|id| Some(id.to_string())).to_vec();
    assert_eq!(order, names(["ana", "familia", "mae", "escola"]), "numbered in the layout's order\n{screen}");
    assert_eq!(v.open.as_deref(), Some("familia"));
    let mut watched = demo.windows_watched();
    watched.sort();
    assert_eq!(watched, ["ana", "escola", "familia", "mae"]);
    for title in ["╭ 1 Ana Ribeiro", "╭ 2  Família Gomes", "╭ 3 Mãe", "╭ 4 Escola do Bairro"] {
        assert!(screen.contains(title), "{title}:\n{screen}");
    }
}

#[test]
fn a_digit_an_arrow_o_or_a_click_gives_a_pane_the_keys() {
    let (mut v, rx, _) = view_with("panes", 160, 45);
    press(&mut v, &rx, FOUR);
    press(&mut v, &rx, "<c-p>3");
    assert_eq!(v.open.as_deref(), Some("mae"));
    press(&mut v, &rx, "<c-p>o");
    assert_eq!(v.open.as_deref(), Some("escola"), "o, the next");
    press(&mut v, &rx, "<c-p><left>");
    assert_eq!(v.open.as_deref(), Some("familia"), "the pane to its left");
    press(&mut v, &rx, "<c-p><up>");
    assert_eq!(v.open.as_deref(), Some("ana"), "and above that");
    let screen = frame(&mut v);
    let row = screen.lines().position(|l| l.contains("╭ 4 Escola do Bairro")).unwrap() as u16 + 3;
    let click =
        MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: 150, row, modifiers: KeyModifiers::NONE };
    v.on_input(Input::Mouse(click));
    run::settle(&mut v, &rx);
    assert_eq!(v.open.as_deref(), Some("escola"), "a click in a pane");
    press(&mut v, &rx, "<c-p>}");
    assert_eq!((shows(&v, 1).as_deref(), v.open.as_deref()), (Some("escola"), Some("escola")), "}} swaps it round");
    press(&mut v, &rx, "<c-p><a-right><a-right><a-right>");
    let screen = frame(&mut v);
    let top = screen.lines().nth(1).unwrap();
    let divider = top.rfind("╮╭").map(|i| top[..i].chars().count()).unwrap();
    assert!(divider >= 60 + 60, "alt-→ again and again moves the divider right, from 109:\n{screen}");
}

#[test]
fn a_click_on_a_message_in_the_pane_with_the_keys_selects_it_whatever_the_pane_beside_has_on_its_row() {
    let (mut v, rx, _) = view_with("panes", 160, 45);
    press(&mut v, &rx, "<c-p>%");
    let screen = frame(&mut v);
    let row = screen.lines().position(|l| l.contains("liga-me quando puderes")).unwrap() as u16;
    let beside: String = screen.lines().nth(row as usize).unwrap().chars().skip(62).take(46).collect();
    assert!(!beside.trim().is_empty(), "Ana's pane has a message on that row too: {beside:?}\n{screen}");
    let click =
        MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: 130, row, modifiers: KeyModifiers::NONE };
    v.on_input(Input::Mouse(click));
    let liga = v.messages.as_ref().unwrap().rows().find(|m| m.spans.iter().any(|s| s.s.contains("liga-me"))).unwrap();
    assert_eq!(v.focus, Focus::Message(liga.id.clone()));
}

#[test]
fn each_panes_box_keeps_what_was_typed_for_its_chat() {
    let (mut v, rx, demo) = view_with("panes", 160, 45);
    press(&mut v, &rx, "levo o vinho<c-p>%ligo às 8<c-p>1");
    assert_eq!((v.open.as_deref(), v.composer.text()), (Some("ana"), "levo o vinho"));
    let screen = frame(&mut v);
    assert!(screen.contains("│ ligo às 8"), "Mãe's pane keeps it in its box:\n{screen}");
    press(&mut v, &rx, "<c-p>2");
    assert_eq!((v.open.as_deref(), v.composer.text()), (Some("mae"), "ligo às 8"));
    let draft = |id: &str| v.chats.get(id).and_then(|c| c.draft.clone());
    assert_eq!(draft("ana").as_deref(), Some("levo o vinho"), "and jawd has it, saved as the pane lost the keys");
    drop(demo);

    let (mut v, rx, _) = view_with("panes,composer", 160, 45);
    press(&mut v, &rx, "levo o vinho<c-p>%ligo às 8");
    let screen = frame(&mut v);
    assert!(screen.contains("╭ to Mãe ─") && screen.contains("│ › ligo às 8"), "one box along the bottom:\n{screen}");
    assert_eq!(screen.matches("╭─").count(), 0, "and none in the panes:\n{screen}");
    press(&mut v, &rx, "<c-p>1");
    let screen = frame(&mut v);
    assert!(
        screen.contains("╭ to Ana Ribeiro ─") && screen.contains("│ › levo o vinho"),
        "it follows the keys:\n{screen}"
    );
}

#[test]
fn a_message_coming_into_a_chat_another_pane_shows_is_drawn_there_and_not_read() {
    let (mut v, rx, demo) = view_with("panes", 160, 45);
    press(&mut v, &rx, "<c-p>%");
    demo.writes("ana", ("person:1", "Ana", 0), "já agora, levas tu o *vinho*?");
    run::settle(&mut v, &rx);
    let screen = frame(&mut v);
    assert!(screen.contains("já agora, levas tu o vinho?"), "{screen}");
    assert_eq!(v.chats.get("ana").map(|c| c.unread), Some(1), "only the pane with the keys reads");
    press(&mut v, &rx, "<c-p>1");
    v.clock = Some(NOW + 2_000);
    v.tick();
    run::settle(&mut v, &rx);
    assert_eq!(
        v.chats.get("ana").map(|c| c.unread),
        Some(0),
        "given them, it reads it, a second at most after the last"
    );
}

#[test]
fn closing_a_pane_lets_its_window_go_and_the_last_left_is_the_conversation_as_it_was() {
    let (mut v, rx, demo) = view_with("panes", 160, 45);
    let was = frame(&mut v);
    press(&mut v, &rx, "<c-p>%<c-p>x");
    assert_eq!(demo.windows_watched(), ["ana"]);
    assert_eq!(frame(&mut v), was, "one pane again: the conversation as it was");
    press(&mut v, &rx, FOUR);
    for left in [3, 2, 1] {
        press(&mut v, &rx, "<c-p>x");
        assert_eq!(demo.windows_watched().len(), left, "{:?}", demo.windows_watched());
    }
    assert!(!v.split() && v.open.is_some());
}

#[test]
fn the_palette_and_alt_arrows_put_a_chat_in_the_pane_with_the_keys_or_go_to_its_pane() {
    let (mut v, rx, demo) = view_with("panes", 160, 45);
    press(&mut v, &rx, "<c-p>%<a-down>");
    assert_eq!((v.open.as_deref(), shows(&v, 1).as_deref()), (Some("escola"), Some("ana")), "the next chat");
    press(&mut v, &rx, "<a-up>");
    assert_eq!(v.open.as_deref(), Some("mae"));
    press(&mut v, &rx, "<c-p>1<a-down>");
    assert_eq!(v.open.as_deref(), Some("escola"), "Alt-↓ passes over Mãe, in the other pane");
    press(&mut v, &rx, "<c-k>mae\n");
    assert_eq!((v.open.as_deref(), shows(&v, 1).as_deref()), (Some("mae"), Some("escola")), "to Mãe's pane");
    press(&mut v, &rx, "<c-k>climbing\n");
    assert_eq!((shows(&v, 1).as_deref(), shows(&v, 2).as_deref()), (Some("escola"), Some("climbing")));
    let mut watched = demo.windows_watched();
    watched.sort();
    assert_eq!(watched, ["climbing", "escola"], "Mãe's window let go as her pane took another chat");
}

#[test]
fn the_status_line_marks_the_pane_with_the_keys_and_counts_only_chats_off_screen() {
    let (mut v, rx, _) = view_with("panes,status_line", 160, 45);
    press(&mut v, &rx, "<c-p>%<c-k>familia\n");
    let status = frame(&mut v).lines().nth_back(1).unwrap().to_string();
    assert!(status.starts_with("  1 Ana Ribeiro   2 Família Gomes "), "{status}");
    assert!(!status.contains("@ Família Gomes"), "on screen, it isn't counted as needing you: {status}");
    assert!(status.contains("4 chats · 26 unread"), "{status}");
    let (mut v, rx, _) = view_with("panes,status_line,composer", 160, 45);
    press(&mut v, &rx, "<c-p>%");
    let screen = frame(&mut v);
    let top = screen.lines().position(|l| l.contains("╭ to Mãe ─")).unwrap();
    let status = screen.lines().nth(top - 1).unwrap();
    assert!(
        status.starts_with("  1 Ana Ribeiro   2  seen 5 min ago"),
        "the box names the chat with the keys: {status}"
    );
}

#[test]
fn there_are_six_panes_at_most_and_each_needs_room() {
    let (mut v, rx, demo) = view_with("panes", 200, 60);
    press(&mut v, &rx, "<c-p>%<c-p>\"<c-p>1<c-p>\"<c-p>1<c-p>%<c-p>4<c-p>%");
    assert_eq!((v.panes_in_order().len(), demo.windows_watched().len()), (MOST, MOST));
    press(&mut v, &rx, "<c-p>6<c-p>\"");
    assert_eq!(v.panes_in_order().len(), MOST);
    assert!(frame(&mut v).contains("6 panes at most"));
    let (mut v, rx, _) = view_with("panes", 100, 30);
    press(&mut v, &rx, "<c-p>%<c-p>%");
    assert_eq!(v.panes_in_order().len(), 2, "no room for a third across");
    assert!(frame(&mut v).contains("no room to split this pane"));
}

/// Pictures of two, three and four panes at 160 × 45 and 100 × 30, with and without the box along
/// the bottom, as HTML in `JAW_SHOTS` (or /tmp/jaw-spike-panes), for a browser to turn into PNGs.
/// Each pane has something typed in its box, and Ana writes in hers while another has the keys.
#[test]
#[ignore = "writes pictures of the panes for people to look at"]
fn shots() {
    let dir = std::path::PathBuf::from(std::env::var("JAW_SHOTS").unwrap_or("/tmp/jaw-spike-panes".into()));
    std::fs::create_dir_all(&dir).unwrap();
    let two = "<c-p>%<c-k>familia\nlevo o pão e o vinho";
    let three = format!("{two}<c-p>\"<c-k>climbing\nsábado às 9 conto convosco");
    let four = format!("{three}<c-p>1<c-p>\"<c-k>mae\nligo-te às 8");
    for (n, keys) in [(2, two.to_string()), (3, three), (4, four)] {
        for (w, h) in [(160, 45), (100, 30)] {
            for (with, on) in [("", "panes,status_line"), ("-composer", "panes,status_line,composer")] {
                let (tx, rx) = std::sync::mpsc::channel();
                let demo = Demo::start(tx, Clock::Fixed(NOW), State::Online, false);
                let mut v = View::new(Box::new(demo.clone()));
                v.experiments = Experiments::parse(on);
                v.images = crate::graphics::Images::here(crate::graphics::Protocol::Blocks, (8, 16));
                v.clock = Some(NOW);
                v.tz = UTC;
                v.theme = Theme::with(true);
                v.no_color = false;
                v.resize(w, h);
                run::settle(&mut v, &rx);
                press(&mut v, &rx, &keys);
                demo.writes("ana", ("person:1", "Ana", 0), "já agora, levas tu o *vinho*?");
                run::settle(&mut v, &rx);
                let title = format!("jaw · {n} panes · {on} · {w}×{h}");
                let html = super::spike_tests::html(&mut v, &title);
                std::fs::write(dir.join(format!("{n}-panes{with}-{w}x{h}.html")), html).unwrap();
            }
        }
    }
}

#[test]
fn with_panes_nothing_due_nothing_is_drawn_and_only_the_clock_wakes_jaw() {
    let (mut v, rx, _) = view_with("all", 160, 45);
    press(&mut v, &rx, FOUR);
    later(&mut v, &rx, 0);
    assert_eq!(v.next_wake(), NOW + KEEP_AFTER, "once, to keep the layout once it has stayed so");
    later(&mut v, &rx, KEEP_AFTER);
    frame(&mut v);
    v.dirty = false;
    v.tick();
    assert!(!v.dirty, "nothing due, nothing to draw");
    assert_eq!(v.next_wake(), NOW + 60_000, "the clock's next minute");
}
