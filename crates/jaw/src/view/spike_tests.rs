//! The experimental switches, each on its own and all together: with every one off the screen is
//! as it was, byte for byte; the box along the bottom names the chat it sends to; the status line
//! names what needs you elsewhere; the focused mode folds the rest away and back; the quick look
//! is bounded and lets its windows go; and none of them wakes jaw when nothing changes.

use super::*;
use crate::demo::{Clock, Demo, State};
use crate::experiments::Experiments;
use crate::graphics::{Images, Protocol};
use crate::run::{self, Event, Probe};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event as Input, KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Modifier};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthStr;

const NOW: i64 = super::tests::NOW;
const UTC: FixedOffset = FixedOffset::east_opt(0).unwrap();

/// A view on the made-up account with the switches `on` names, and the account, to ask it what
/// it watches.
pub(super) fn view_with(on: &str, state: State, width: u16, height: u16) -> (View, Receiver<Event>, Demo) {
    let (tx, rx) = std::sync::mpsc::channel();
    let demo = Demo::start(tx, Clock::Fixed(NOW), state, false);
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

pub(super) fn frame(v: &mut View) -> String {
    let (w, h) = v.size().unwrap();
    run::snapshot(v, w, h)
}

pub(super) fn press(v: &mut View, rx: &Receiver<Event>, keys: &str) {
    run::press(v, rx, keys).unwrap();
}

fn row_of(screen: &str, text: &str) -> Option<usize> {
    screen.lines().position(|l| l.contains(text))
}

fn line(screen: &str, row: usize) -> &str {
    screen.lines().nth(row).unwrap_or_default()
}

/// Every cell of a frame, its symbol and its style, and where the cursor is, folded into one
/// number, with the frame as text to show when it isn't the one expected.
fn digest(v: &mut View) -> (u64, String) {
    let (w, h) = v.size().unwrap();
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| v.draw(f)).unwrap();
    let cursor = term.get_cursor_position().unwrap();
    let buf = term.backend().buffer();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut fold = |s: &str| {
        for b in s.bytes() {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    };
    for cell in &buf.content {
        fold(&format!("{}\u{1f}{:?}\u{1f}{:?}\u{1f}{:?}\u{1e}", cell.symbol(), cell.fg, cell.bg, cell.modifier));
    }
    fold(&format!("{cursor:?}"));
    let text = buf
        .content
        .chunks(w as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>().trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    (hash, text)
}

/// The screens the switches could change, with the digest of each as `main` drew it before them,
/// every switch off. A change made to the default screen on purpose changes these too.
const AS_IT_WAS: [(State, u16, u16, &str, u64); 14] = [
    (State::Online, 160, 45, "", 0x150e_d699_1be5_c3a8),
    (State::Online, 140, 40, "", 0x7883_a78e_0896_e06e),
    (State::Online, 100, 30, "", 0x81a9_aa21_01c5_e567),
    (State::Online, 80, 24, "", 0xb982_f07f_f882_04e1),
    (State::Online, 80, 24, "<down>\n", 0xfae9_0b86_f22e_e0a7),
    (State::Online, 100, 30, "<up>", 0xbf06_df15_7c2f_e04a),
    (State::Online, 140, 34, "levo o vinho<c-k>", 0x2242_9616_c510_3a94),
    (State::Online, 100, 30, "?", 0xde97_6c5d_6a5f_7b97),
    (State::Online, 100, 30, "sim!<a-enter>e levo a sobremesa", 0x09d0_74e0_bbfe_20ea),
    (State::Online, 120, 30, "<c-b>", 0x43f3_8f0f_3f68_b16c),
    (State::CatchingUp, 140, 30, "<a-down><a-down><a-down><a-down>", 0xd53c_32d8_d30c_8818),
    (State::Online, 100, 30, "<c-r>", 0x0d72_2f05_ac1f_aa25),
    (State::Online, 140, 40, "<c-l>", 0x768b_3558_bafb_af09),
    // The switches' keys do what they did before them while typing: nothing.
    (State::Online, 140, 40, "<c-f><c-g><c-p>", 0x7883_a78e_0896_e06e),
];

#[test]
fn with_every_switch_off_the_screen_is_as_it_was_byte_for_byte() {
    for (state, w, h, keys, was) in AS_IT_WAS {
        let (mut v, rx, demo) = view_with("", state, w, h);
        assert_eq!(v.experiments, Experiments::default());
        press(&mut v, &rx, keys);
        let (now, text) = digest(&mut v);
        assert_eq!(now, was, "{state:?} at {w}×{h} after {keys:?} isn't as it was: 0x{now:016x}\n{text}");
        assert!(demo.quick_look_watched().is_empty());
    }
}

#[test]
fn the_box_along_the_bottom_is_the_screens_whole_width_and_names_the_chat_it_sends_to() {
    let (mut v, rx, _) = view_with("composer", State::Online, 160, 45);
    let screen = frame(&mut v);
    let top = row_of(&screen, "╭ to Ana Ribeiro ─").unwrap_or_else(|| panic!("{screen}"));
    assert!(line(&screen, top).starts_with("╭ to Ana Ribeiro ─") && line(&screen, top).ends_with('╮'), "{screen}");
    assert_eq!(crate::fmt::width(line(&screen, top)), 160, "the whole width:\n{screen}");
    let above = line(&screen, top - 1);
    assert!(above.starts_with("╰─") && above.matches("╯╰").count() == 2, "under the sidebar and the list:\n{screen}");
    assert!(line(&screen, top + 1).starts_with("│ › "), "{screen}");
    assert_eq!(screen.lines().count(), top + 4, "then the keys:\n{screen}");
    assert_eq!(screen.matches("╭─").count() + screen.matches("╭ to").count(), 1, "no box in the conversation");
    assert_eq!(v.areas.composer.y, top as u16 + 1);

    press(&mut v, &rx, "levo o vinho<a-down>");
    let screen = frame(&mut v);
    assert!(screen.contains("╭ to Mãe ─"), "{screen}");
    assert!(v.composer.is_empty() && v.chats.get("ana").unwrap().draft.as_deref() == Some("levo o vinho"));
    press(&mut v, &rx, "<a-up>");
    let screen = frame(&mut v);
    assert!(screen.contains("╭ to Ana Ribeiro ─") && screen.contains("│ › levo o vinho"), "{screen}");

    press(&mut v, &rx, "<c-u><up>r");
    let screen = frame(&mut v);
    assert!(screen.contains("╭ to Ana Ribeiro ─ ↩ Ana vens jantar sexta?"), "replying, it still names it:\n{screen}");
    press(&mut v, &rx, "<esc><c-r>");
    assert!(frame(&mut v).contains("● recording for Ana Ribeiro"), "and recording");
}

#[test]
fn on_a_narrow_screen_the_box_comes_with_the_chat_and_goes_with_the_list() {
    let (mut v, rx, _) = view_with("composer,status_line", State::Online, 80, 24);
    let screen = frame(&mut v);
    assert!(screen.contains("chats · All") && !screen.contains("╭ to "), "{screen}");
    press(&mut v, &rx, "<down>\n");
    let screen = frame(&mut v);
    assert!(screen.contains("╭ to Mãe ─"), "{screen}");
    assert!(screen.lines().all(|l| crate::fmt::width(l) <= 80), "{screen}");
}

/// The status line names the chat you're in only where nothing else on its line would: the box
/// along the bottom, right below it, says "to" the chat, and then the line keeps only how it is.
#[test]
fn the_status_line_names_the_chat_only_where_the_box_below_it_doesnt() {
    let status_above_box = |v: &mut View| {
        let screen = frame(v);
        let top = row_of(&screen, "╭ to ").unwrap_or_else(|| panic!("{screen}"));
        line(&screen, top - 1).to_string()
    };
    let status_above_keys = |v: &mut View| frame(v).lines().nth_back(1).unwrap().to_string();

    let (mut v, rx, _) = view_with("composer,status_line", State::Online, 160, 45);
    let status = status_above_box(&mut v);
    assert!(status.starts_with("  typing…") && !status.contains("Ana Ribeiro"), "{status}");
    assert!(status.contains("@ Família Gomes"), "{status}");
    press(&mut v, &rx, "<c-l>");
    let status = status_above_keys(&mut v);
    assert!(status.starts_with("  Ana Ribeiro   typing…"), "the box away with the library, it names it: {status}");
    press(&mut v, &rx, "<esc><c-k>mae\n");
    let status = status_above_box(&mut v);
    assert!(status.starts_with("  seen 5 min ago") && !status.contains("Mãe"), "{status}");

    let (mut v, _rx, _) = view_with("status_line", State::Online, 160, 45);
    assert!(status_above_keys(&mut v).starts_with("  Ana Ribeiro   typing…"), "alone, it names it");

    let (mut v, rx, _) = view_with("focus", State::Online, 160, 45);
    press(&mut v, &rx, "<c-f>");
    assert!(status_above_keys(&mut v).starts_with("  Ana Ribeiro   typing…"), "focused without the box, too");

    let (mut v, rx, _) = view_with("focus,composer", State::Online, 160, 45);
    press(&mut v, &rx, "<c-f>");
    let status = status_above_box(&mut v);
    assert!(status.starts_with("  typing…") && !status.contains("Ana Ribeiro"), "focused with it, not: {status}");

    let (mut v, rx, _) = view_with("composer,status_line", State::Online, 80, 24);
    assert!(status_above_keys(&mut v).starts_with("  Ana Ribeiro"), "with the list, no box: it names it");
    press(&mut v, &rx, "<down>\n");
    let status = status_above_box(&mut v);
    assert!(status.starts_with("  seen 5 min ago") && !status.contains("Mãe"), "{status}");
}

#[test]
fn the_status_line_says_the_chat_youre_in_and_names_what_needs_you_elsewhere() {
    let (mut v, rx, demo) = view_with("status_line", State::Online, 160, 45);
    let screen = frame(&mut v);
    let n = screen.lines().count();
    let status = line(&screen, n - 2);
    assert!(status.starts_with("  Ana Ribeiro   typing…"), "{screen}");
    assert!(status.contains("@ Família Gomes   4 more chats · 26 unread"), "{status}");
    assert!(!status.contains("Ginásio") && !status.contains("Avó"), "muted chats don't need you: {status}");
    assert!(row_of(&screen, "╭─").is_some_and(|r| r < n - 2), "the box stays in the conversation:\n{screen}");

    press(&mut v, &rx, "<c-k>climbing\n");
    let screen = frame(&mut v);
    let status = line(&screen, screen.lines().count() - 2);
    assert!(status.starts_with("  Lisbon Climbing "), "{status}");
    assert!(status.contains("@ Família Gomes   3 more chats · 12 unread"), "Ana's was read as it opened: {status}");
    demo.writes("ana", ("person:1", "Ana", 0), "já agora, levas tu o *vinho*?");
    demo.writes("ana", ("person:1", "Ana", 0), "ou compro eu no caminho, diz-me");
    run::settle(&mut v, &rx);
    let status = frame(&mut v).lines().nth_back(1).unwrap().to_string();
    assert!(status.contains("@ Família Gomes   ● Ana · 2   3 more chats · 12 unread"), "{status}");

    press(&mut v, &rx, "<c-k>mae\n");
    let status = frame(&mut v).lines().nth_back(1).unwrap().to_string();
    assert!(status.starts_with("  Mãe   seen 5 min ago"), "{status}");
}

#[test]
fn short_of_room_the_status_line_shortens_its_count_before_it_stops_naming() {
    for (w, said) in [
        (100, "@ Família Gomes   ● Ana · 1   4 more chats · 26 unread   ctrl-g look"),
        (80, "@ Família Gomes   ● Ana · 2   +4   ctrl-g look"),
        (64, "@ Família Gomes   +5   ctrl-g look"),
    ] {
        let (mut v, rx, demo) = view_with("status_line,quick_look", State::Online, w, 24);
        press(&mut v, &rx, if w < 90 { "<down>\n" } else { "<a-down>" });
        demo.writes("ana", ("person:1", "Ana", 0), "já agora, levas tu o *vinho*?");
        run::settle(&mut v, &rx);
        let screen = frame(&mut v);
        let status = line(&screen, screen.lines().count() - 2);
        assert!(status.starts_with("  Mãe   seen 5 min ago") && status.ends_with(said), "{w}: {status}");
        assert!(crate::fmt::width(status) <= w as usize, "{status}");
    }
}

#[test]
fn ctrl_f_folds_the_sidebar_and_the_list_away_for_one_chat_and_brings_them_back() {
    let (mut v, rx, _) = view_with("focus", State::Online, 160, 45);
    let (before, _) = digest(&mut v);
    let screen = frame(&mut v);
    assert!(screen.contains("╭ folders ─") && screen.contains("╭ chats · All ─"), "{screen}");
    assert!(screen.lines().last().unwrap().contains("ctrl-f focus"), "{screen}");
    assert!(!screen.contains("4 more chats"), "no status line until it's focused:\n{screen}");

    press(&mut v, &rx, "<c-f>");
    let screen = frame(&mut v);
    assert!(!screen.contains("╭ folders") && !screen.contains("╭ chats · All"), "{screen}");
    assert!(line(&screen, 1).starts_with("╭ Ana Ribeiro · typing… ─") && line(&screen, 1).ends_with('╮'), "{screen}");
    let n = screen.lines().count();
    assert!(line(&screen, n - 2).contains("@ Família Gomes"), "the status line, the only glimpse:\n{screen}");
    assert!(line(&screen, n - 1).contains("ctrl-f all chats"), "{screen}");

    press(&mut v, &rx, "<a-down>");
    let screen = frame(&mut v);
    assert!(
        line(&screen, 1).starts_with("╭ Mãe ─") && !screen.contains("╭ chats"),
        "focused, Alt-↓ moves on:\n{screen}"
    );
    press(&mut v, &rx, "<a-up><c-f><c-f><c-f>");
    assert!(!v.focus_mode);
    let (after, text) = digest(&mut v);
    assert_eq!(after, before, "brought back, the screen is as it was:\n{text}");

    press(&mut v, &rx, "<c-f><c-b>");
    assert!(!v.focus_mode && v.sidebar.keys.is_some(), "Ctrl-B brings the rest back with the sidebar");

    press(&mut v, &rx, "<c-b><c-f>");
    v.experiments.focus = false;
    let screen = frame(&mut v);
    assert!(screen.contains("╭ chats · All") && !screen.contains("4 more chats"), "switched off, nothing stays folded");
}

#[test]
fn focused_the_folder_key_names_the_folder_and_the_key_that_shows_its_chats() {
    for keys in ["", "<up>", "<up>r", "<up><up>e"] {
        let (mut v, rx, _) = view_with("focus", State::Online, 160, 45);
        press(&mut v, &rx, &format!("<c-f>{keys}<a-s-down>"));
        let screen = frame(&mut v);
        let footer = screen.lines().last().unwrap_or_default();
        assert!(footer.contains("Unread: ctrl-f shows its chats"), "after {keys:?}:\n{screen}");
        press(&mut v, &rx, "<c-f>");
        assert!(frame(&mut v).contains("╭ chats · Unread"), "Ctrl-F shows them after {keys:?} too");
    }
}

#[test]
fn the_quick_look_shows_a_few_lines_of_a_few_chats_and_lets_their_windows_go() {
    let (mut v, rx, demo) = view_with("quick_look", State::Online, 160, 45);
    press(&mut v, &rx, "<c-g>");
    let watched = demo.quick_look_watched();
    assert_eq!(watched, ["familia", "escola", "climbing", "bairro-avisos"], "a mention first, then by the list");
    assert_eq!(v.open.as_deref(), Some("ana"), "the chat you're in stays open");
    let q = v.quick_look.as_ref().unwrap();
    assert_eq!(q.chats.len(), quick_look::CHATS);
    assert!(q.chats.iter().all(|(_, w)| w.loaded && w.rows().count() <= quick_look::LINES));
    let screen = frame(&mut v);
    assert!(screen.contains("╭ quick look · 4 of the 5 chats that need you "), "{screen}");
    assert!(screen.contains("@ Família Gomes") && screen.contains("@1 · 3 unread"), "{screen}");
    assert!(screen.contains("Inês") && screen.contains("@Pedro vens buscar a avó?"), "{screen}");
    let school = row_of(&screen, "Escola do Bairro").unwrap();
    let said: Vec<&str> = screen.lines().skip(school + 1).take(4).collect();
    assert!(said[..3].iter().all(|l| l.contains("Prof. Helena")), "three of its nine:\n{screen}");
    assert!(said[3].contains("⋯ 6 more"), "{screen}");
    assert!(screen.lines().last().unwrap().contains("⏎ goes to it"), "{screen}");

    press(&mut v, &rx, "<esc>");
    assert!(v.quick_look.is_none() && demo.quick_look_watched().is_empty(), "Esc lets the windows go");
    assert_eq!(v.open.as_deref(), Some("ana"));

    press(&mut v, &rx, "<c-g><down>\n");
    assert_eq!(v.open.as_deref(), Some("escola"), "Enter goes to the chat selected");
    assert!(v.quick_look.is_none() && demo.quick_look_watched().is_empty());

    press(&mut v, &rx, "<c-k>quick\n");
    assert!(v.quick_look.is_some(), "the palette opens it too");
    press(&mut v, &rx, "<c-g>");
    assert!(v.quick_look.is_none() && demo.quick_look_watched().is_empty(), "and its key closes it");
}

#[test]
fn a_quick_look_takes_what_comes_while_its_open_and_nothing_once_its_closed() {
    let (mut v, rx, demo) = view_with("quick_look", State::Online, 140, 40);
    press(&mut v, &rx, "<c-g>");
    demo.flood(400, 0.3).join().unwrap();
    run::settle(&mut v, &rx);
    let q = v.quick_look.as_ref().unwrap();
    assert!(q.chats.iter().all(|(_, w)| w.rows().count() <= quick_look::LINES), "bounded however much comes");
    press(&mut v, &rx, "<esc>");
    demo.flood(400, 0.2).join().unwrap();
    let after: Vec<Update> =
        rx.try_iter().filter_map(|e| if let Event::Update(u) = e { Some(u) } else { None }).collect();
    assert!(!after.iter().any(|u| matches!(u, Update::QuickLook { .. })), "nothing once it's closed");
}

#[test]
fn with_every_switch_on_nothing_due_nothing_is_drawn_and_only_the_clock_wakes_jaw() {
    let (mut v, rx, _) = view_with("all", State::Online, 160, 45);
    for keys in ["", "<c-f>", "<c-g>"] {
        press(&mut v, &rx, keys);
        frame(&mut v);
        v.dirty = false;
        v.tick();
        assert!(!v.dirty, "after {keys:?}: nothing due, nothing to draw");
        assert_eq!(v.next_wake(), NOW + 60_000, "after {keys:?}: the clock's next minute");
    }
}

/// The loop, with every switch on, the chat focused and the quick look open, draws its first
/// frames and then nothing.
#[test]
fn with_every_switch_on_the_loop_goes_quiet() {
    let (tx, rx) = std::sync::mpsc::channel();
    let demo = Demo::start(tx.clone(), Clock::Wall, State::Online, false);
    let mut v = View::new(Box::new(demo));
    v.experiments = Experiments::parse("all");
    let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
    let mut probe = Probe::default();
    let start = Instant::now();
    let stop = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        for c in ['f', 'g'] {
            let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
            let _ = tx.send(Event::Input(Input::Key(key), Instant::now()));
        }
        std::thread::sleep(Duration::from_millis(650));
        let _ = tx.send(Event::Stop);
    });
    run::drive(&mut v, &rx, &mut terminal, &mut probe, &mut |_| {}).unwrap();
    stop.join().unwrap();
    assert!(v.focus_mode && v.quick_look.is_some());
    let quiet = start.elapsed() - (probe.last.unwrap() - start);
    let minute = |t: i64| t / crate::fmt::MINUTE;
    let crossed = minute(v.now()) != minute(v.now() - start.elapsed().as_millis() as i64);
    assert!(crossed || quiet >= Duration::from_millis(450), "no frame in the last {quiet:?}");
    assert!(probe.frames <= 8, "a few frames while the account arrives, then none: {}", probe.frames);
}

/// A frame as HTML, each cell in its colours and styles, wide ones over two cells, and the
/// cursor where it is, for pictures of the switches.
pub(super) fn html(v: &mut View, title: &str) -> String {
    let (w, h) = v.size().unwrap();
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| v.draw(f)).unwrap();
    let cursor = term.backend().cursor_visible().then(|| term.backend().cursor_position());
    let buf = term.backend().buffer();
    let hex = |c: Color, or: &str| match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => or.to_string(),
    };
    let mut out = format!(
        "<!doctype html><meta charset=utf-8><title>{title}</title><style>body{{margin:0;background:#1a1b26}}\
         .t{{display:inline-block;padding:14px;font:14px/17px Menlo,monospace}}.r{{height:17px;white-space:pre}}\
         .r span{{display:inline-block;height:17px;vertical-align:top;width:1ch;overflow:visible}}\
         .r span.w{{width:2ch}}</style><div class=t>"
    );
    for y in 0..h {
        out.push_str("<div class=r>");
        let mut x = 0;
        while x < w {
            let cell = &buf[(x, y)];
            let (mut fg, mut bg) = (hex(cell.fg, "#c0caf5"), hex(cell.bg, "#1a1b26"));
            if cell.modifier.contains(Modifier::REVERSED) ^ (cursor == Some(ratatui::layout::Position { x, y })) {
                std::mem::swap(&mut fg, &mut bg);
            }
            let mut style = format!("color:{fg};background:{bg}");
            for (m, css) in [
                (Modifier::BOLD, ";font-weight:700"),
                (Modifier::ITALIC, ";font-style:italic"),
                (Modifier::UNDERLINED, ";text-decoration:underline"),
                (Modifier::CROSSED_OUT, ";text-decoration:line-through"),
                (Modifier::DIM, ";opacity:.6"),
            ] {
                if cell.modifier.contains(m) {
                    style.push_str(css);
                }
            }
            let symbol = cell.symbol();
            let wide = symbol.width() > 1 && x + 1 < w;
            let text = symbol.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
            out.push_str(&format!("<span{} style=\"{style}\">{text}</span>", if wide { " class=w" } else { "" }));
            x += if wide { 2 } else { 1 };
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");
    out
}

/// Pictures of the switches on the made-up account, as HTML in `JAW_SHOTS` (or
/// /tmp/jaw-spike-composer), for a browser to turn into PNGs.
#[test]
#[ignore = "writes pictures of the switches for people to look at"]
fn shots() {
    let dir = std::path::PathBuf::from(std::env::var("JAW_SHOTS").unwrap_or("/tmp/jaw-spike-composer".into()));
    std::fs::create_dir_all(&dir).unwrap();
    // Lisbon Climbing open with something typed for it, while Ana writes twice elsewhere.
    let shots: [(&str, &str, u16, u16, &str); 10] = [
        ("01-default-160x45", "", 160, 45, ""),
        ("02-composer-160x45", "composer", 160, 45, ""),
        ("03-composer-status-line-160x45", "composer,status_line", 160, 45, ""),
        ("04-status-line-160x45", "status_line", 160, 45, ""),
        ("05-focus-160x45", "focus", 160, 45, "<c-f>"),
        ("06-quick-look-160x45", "quick_look", 160, 45, "<c-g>"),
        ("07-all-160x45", "all", 160, 45, ""),
        ("08-all-focused-quick-look-160x45", "all", 160, 45, "<c-f><c-g>"),
        ("09-default-100x30", "", 100, 30, ""),
        ("10-all-100x30", "all", 100, 30, ""),
    ];
    for (name, on, w, h, keys) in shots {
        let (tx, rx) = std::sync::mpsc::channel();
        let demo = Demo::start(tx, Clock::Fixed(NOW), State::Online, false);
        let mut v = View::new(Box::new(demo.clone()));
        v.experiments = Experiments::parse(on);
        v.images = Images::here(Protocol::Blocks, (8, 16));
        v.clock = Some(NOW);
        v.tz = UTC;
        v.theme = Theme::with(true);
        v.no_color = false;
        v.resize(w, h);
        run::settle(&mut v, &rx);
        press(&mut v, &rx, "<c-k>climbing\nsábado às 9 conto convosco");
        demo.writes("ana", ("person:1", "Ana", 0), "já agora, levas tu o *vinho*?");
        demo.writes("ana", ("person:1", "Ana", 0), "ou compro eu no caminho, diz-me");
        run::settle(&mut v, &rx);
        press(&mut v, &rx, keys);
        let title = format!("jaw · {} · {w}×{h}", if on.is_empty() { "default" } else { on });
        std::fs::write(dir.join(format!("{name}.html")), html(&mut v, &title)).unwrap();
    }
}
