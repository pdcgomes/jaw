//! Disappearing and view-once messages: a chat's timer in its title, set from the palette; a
//! message's mark beside its time and when it goes in its info; and a view-once photo's mark,
//! opened once over the screen and gone once it's closed, however it closes.

use super::*;
use crate::data::{Kind, Opened};
use crate::demo::{Clock, Demo, State};
use crate::run::{self, Event};
use std::sync::mpsc::Receiver;

const NOW: i64 = super::tests::NOW;
const UTC: FixedOffset = FixedOffset::east_opt(0).unwrap();

fn view(width: u16, height: u16) -> (View, Receiver<Event>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let demo = Demo::start(tx, Clock::Fixed(NOW), State::Online, false);
    let mut v = View::new(Box::new(demo));
    v.clock = Some(NOW);
    v.tz = UTC;
    v.theme = Theme::with(true);
    v.no_color = false;
    v.resize(width, height);
    run::settle(&mut v, &rx);
    (v, rx)
}

fn frame(v: &mut View) -> String {
    let (w, h) = v.size().unwrap();
    run::snapshot(v, w, h)
}

fn press(v: &mut View, rx: &Receiver<Event>, keys: &str) {
    run::press(v, rx, keys).unwrap();
}

/// The made-up account with Sofia's chat open: seven days on, her view-once photo, one of yours
/// she has opened, and her voice message, newest.
fn at_sofias(width: u16, height: u16) -> (View, Receiver<Event>) {
    let (mut v, rx) = view(width, height);
    press(&mut v, &rx, "<c-k>sofia\n");
    assert_eq!(v.open.as_deref(), Some("sofia"));
    let _ = frame(&mut v);
    (v, rx)
}

fn line<'a>(screen: &'a str, text: &str) -> &'a str {
    screen.lines().find(|l| l.contains(text)).unwrap_or_else(|| panic!("no line with {text:?} in\n{screen}"))
}

#[test]
fn a_chat_with_disappearing_messages_says_its_timer_in_its_title_and_each_message_by_its_time() {
    let (mut v, rx) = view(140, 40);
    let screen = frame(&mut v);
    let title = line(&screen, "╭ Ana Ribeiro ");
    assert!(!title.contains('⏱'), "a chat without one says nothing: {title}");
    press(&mut v, &rx, "<c-k>sofia\n");
    let screen = frame(&mut v);
    let title = line(&screen, "╭ Sofia Martins ");
    assert!(title.contains("Sofia Martins · ⏱ 7 days ─"), "{title}");
    let voice = v.messages.as_ref().unwrap().get("sofia-00003").unwrap().clone();
    let clock = fmt::clock(voice.at, UTC);
    let said = screen.lines().find(|l| l.contains("0:42") && l.contains('⏱')).unwrap_or_else(|| panic!("{screen}"));
    assert!(said.ends_with(&format!(" ⏱{clock} │")), "its mark in the gap before its time: {said}");
}

#[test]
fn the_info_of_a_message_that_disappears_says_when() {
    let (mut v, rx) = at_sofias(140, 40);
    press(&mut v, &rx, "<up>i");
    let voice = v.messages.as_ref().unwrap().get("sofia-00003").unwrap().clone();
    let screen = frame(&mut v);
    let said = line(&screen, "disappears");
    assert!(said.contains(&fmt::full(voice.expires_at.unwrap(), UTC)), "{said}");
}

#[test]
fn a_view_once_photo_says_only_what_it_is_and_yours_says_once_its_been_opened() {
    let (mut v, rx) = at_sofias(140, 40);
    let screen = frame(&mut v);
    assert!(line(&screen, "View once photo · ⏎ opens it once").contains("① View once photo"), "{screen}");
    assert!(line(&screen, "View once photo · opened").contains("✓✓"), "yours, opened, with its ticks: {screen}");
    // Yours can't be opened here, as on your phone, and neither is forwarded.
    press(&mut v, &rx, "<up><up>\n");
    assert!(v.once.is_none());
    assert!(frame(&mut v).contains("a view-once message you sent can't be opened here"));
    press(&mut v, &rx, "f");
    assert!(v.palette.is_none());
    assert!(frame(&mut v).contains("a view-once message can't be forwarded"));
}

#[test]
fn a_view_once_photo_opens_once_over_the_screen_with_its_caption_and_time_and_goes_when_closed() {
    let (mut v, rx) = at_sofias(140, 40);
    press(&mut v, &rx, "<up><up><up>");
    assert_eq!(v.focus, Focus::Message("sofia-00001".into()));
    press(&mut v, &rx, "\n");
    let screen = frame(&mut v);
    let title = line(&screen, "① view once");
    assert!(title.contains("· photo from Sofia") && title.contains("goes in 1:00"), "{title}");
    assert!(screen.contains("a vista daqui"), "its caption, which its row never had: {screen}");
    assert!(line(&screen, "esc").contains("esc closes it, and it's gone"), "{screen}");
    // Keys go to it, and only Esc closes it.
    press(&mut v, &rx, "xy<up>\n");
    assert!(v.once.is_some() && v.composer.is_empty());
    press(&mut v, &rx, "<esc>");
    assert!(v.once.is_none());
    let screen = frame(&mut v);
    assert!(!screen.contains("① view once") && !screen.contains("⏎ opens it once"), "it's gone: {screen}");
    assert!(v.messages.as_ref().unwrap().get("sofia-00001").is_none());
}

#[test]
fn a_view_once_photo_left_open_closes_at_its_time_and_one_closed_as_it_opens_goes_once_it_has() {
    let (mut v, rx) = at_sofias(140, 40);
    press(&mut v, &rx, "<up><up><up>\n");
    assert!(v.once.as_ref().is_some_and(|o| o.message == "sofia-00001"));
    v.clock = Some(NOW + 59_000);
    v.tick();
    assert!(line(&frame(&mut v), "① view once").contains("goes in 0:01"));
    v.clock = Some(NOW + 60_000);
    v.tick();
    run::settle(&mut v, &rx);
    assert!(v.once.is_none(), "it closed at its time");
    assert!(v.messages.as_ref().unwrap().get("sofia-00001").is_none(), "and went");
    // Closed before jawd answered: the answer, when it comes, closes it.
    let (mut v, rx) = at_sofias(140, 40);
    let m = v.messages.as_ref().unwrap().get("sofia-00001").unwrap().clone();
    v.open_once(&m);
    run::settle(&mut v, &rx);
    let answered = v.once.take().and_then(|o| o.opened_for_tests());
    let opened = answered.expect("the made-up account answered");
    v.on_opened("sofia".into(), "sofia-00001".into(), Ok(opened));
    run::settle(&mut v, &rx);
    assert!(v.messages.as_ref().unwrap().get("sofia-00001").is_none(), "closed, so it went");
    // Refused, it says why and shows nothing.
    let (mut v, _rx) = at_sofias(140, 40);
    let m = v.messages.as_ref().unwrap().get("sofia-00003").unwrap().clone();
    v.once = Some(once::Once::opening("sofia", &m));
    v.on_opened("sofia".into(), "sofia-00003".into(), Err("WhatsApp no longer has this file".into()));
    assert!(v.once.is_none());
    assert!(frame(&mut v).contains("WhatsApp no longer has this file"));
}

#[test]
fn a_view_once_video_plays_in_the_systems_app_from_the_file_jawd_answered_with() {
    let (mut v, rx) = at_sofias(140, 40);
    let mut m = v.messages.as_ref().unwrap().get("sofia-00001").unwrap().clone();
    m.kind = Kind::Video;
    v.once = Some(once::Once::opening("sofia", &m));
    let opened = Opened {
        message: m.id.clone(),
        kind: Kind::Video,
        path: "media/files/aa.mp4".into(),
        spans: vec![],
        media: None,
        until: NOW + 75_000,
    };
    v.on_opened("sofia".into(), m.id.clone(), Ok(Box::new(opened)));
    let screen = frame(&mut v);
    assert!(line(&screen, "① view once").contains("· video from Sofia"), "{screen}");
    let hints = screen.lines().last().unwrap();
    assert!(hints.contains("⏎ plays it  esc closes it, and it's gone"), "{hints}");
    let _ = v.take_effects();
    press(&mut v, &rx, "\n");
    let opened = v.take_effects();
    assert!(matches!(opened.as_slice(), [Effect::Open(path)] if path.ends_with("media/files/aa.mp4")), "{opened:?}");
    assert!(v.once.is_some(), "and stays open until it's closed");
}

#[test]
fn the_palette_sets_a_chats_timer_to_off_24_hours_7_days_or_90_days() {
    let (mut v, rx) = view(140, 40);
    assert_eq!(v.open.as_deref(), Some("ana"));
    press(&mut v, &rx, "<c-k>disappearing");
    let screen = frame(&mut v);
    assert!(line(&screen, "Disappearing messages").contains("Ana Ribeiro · off · ⏎ asks which"), "{screen}");
    press(&mut v, &rx, "\n");
    let screen = frame(&mut v);
    for choice in ["Off", "24 hours", "7 days", "90 days"] {
        assert!(screen.contains(choice), "{choice}: {screen}");
    }
    assert!(screen.lines().any(|l| l.contains("▸ Off") && l.contains("now")), "{screen}");
    press(&mut v, &rx, "<down><down>\n");
    assert!(v.palette.is_none());
    let screen = frame(&mut v);
    assert!(screen.contains("new messages disappear after 7 days"), "{screen}");
    assert!(line(&screen, "╭ Ana Ribeiro ").contains("Ana Ribeiro · ⏱ 7 days"), "{screen}");
    // And back off, Esc first stepping back out of the choices.
    press(&mut v, &rx, "<c-k>disappearing\n<esc>");
    assert!(v.palette.as_ref().is_some_and(|p| p.timing.is_none()));
    press(&mut v, &rx, "\n\n");
    let screen = frame(&mut v);
    assert!(screen.contains("disappearing messages off"), "{screen}");
    assert!(!line(&screen, "╭ Ana Ribeiro ").contains('⏱'), "{screen}");
}

#[test]
fn a_timer_is_named_as_the_phones_name_it() {
    let named: Vec<String> = [86_400, 604_800, 7_776_000, 3_600, 7_200, 172_800, 90].map(fmt::timer).to_vec();
    assert_eq!(named, ["24 hours", "7 days", "90 days", "1 hour", "2 hours", "2 days", "2 minutes"]);
}
