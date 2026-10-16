use super::*;
use ratatui::buffer::CellDiffOption;
use std::collections::HashMap as Vars;

const CELL: (u16, u16) = (8, 16);

/// A picture written to a temp folder, `f` painting each pixel.
fn picture(dir: &Path, name: &str, w: u32, h: u32, mut f: impl FnMut(u32, u32) -> [u8; 4]) -> PathBuf {
    let path = dir.join(name);
    image::RgbaImage::from_fn(w, h, |x, y| image::Rgba(f(x, y))).save(&path).expect("a picture");
    path
}

/// Red on top, blue below, four pixels square: four cells by two of half-blocks exactly.
fn halves(dir: &Path) -> PathBuf {
    picture(dir, "halves.png", 4, 4, |_, y| if y < 2 { [255, 0, 0, 255] } else { [0, 0, 255, 255] })
}

const ALL: Rect = Rect::new(0, 0, 60, 30);

fn screen() -> Buffer {
    Buffer::empty(ALL)
}

fn look() -> Look {
    Look { bg: Color::Rgb(26, 27, 38), truecolor: true, over: false }
}

fn place(x: u16, top: i32, cols: u16, rows: u16) -> Place {
    Place { x, top, cols, rows }
}

fn env(pairs: &[(&str, &str)]) -> Option<Protocol> {
    let vars: Vars<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    detect::from_env(|k| vars.get(k).cloned())
}

fn unbase64(s: &[u8]) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    };
    let digits: Vec<u8> = s.iter().copied().filter(|c| *c != b'=').map(value).collect();
    let mut out = vec![];
    for chunk in digits.chunks(4) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, d)| n | (*d as u32) << (18 - 6 * i));
        out.extend(&n.to_be_bytes()[1..chunk.len()]);
    }
    out
}

/// The PNG a kitty send carries, put together from its chunks.
fn kitty_png(writes: &[u8]) -> (String, Vec<u8>) {
    let text = String::from_utf8_lossy(writes).to_string();
    let mut first = String::new();
    let mut data = vec![];
    for chunk in text.split("\x1b_G").skip(1) {
        let chunk = chunk.trim_end_matches("\x1b\\");
        let (keys, payload) = chunk.split_once(';').unwrap_or((chunk, ""));
        if first.is_empty() {
            first = keys.to_string();
        }
        data.extend(payload.trim_end_matches("\x1b\\").as_bytes());
    }
    (first, unbase64(&data))
}

#[test]
fn a_photo_keeps_its_shape_within_the_most_cells() {
    assert_eq!(fit(4032, 3024, 40, 16, CELL), (40, 15));
    assert_eq!(fit(1920, 1080, 40, 16, CELL), (40, 11));
}

#[test]
fn a_tall_photo_is_held_to_the_most_rows_and_narrows() {
    assert_eq!(fit(3024, 4032, 40, 16, CELL), (24, 16));
}

#[test]
fn a_cell_the_terminal_says_is_wider_changes_the_rows() {
    assert_eq!(fit(4032, 3024, 40, 16, (10, 20)), (40, 15));
    assert_eq!(fit(4032, 3024, 40, 16, (9, 20)), (40, 14));
}

#[test]
fn jaw_graphics_overrides_what_the_terminal_says() {
    assert_eq!(env(&[("JAW_GRAPHICS", "blocks"), ("KITTY_WINDOW_ID", "1")]), Some(Protocol::Blocks));
    assert_eq!(env(&[("JAW_GRAPHICS", "none"), ("TERM_PROGRAM", "iTerm.app")]), Some(Protocol::None));
    assert_eq!(env(&[("JAW_GRAPHICS", "sixel")]), Some(Protocol::Sixel));
    assert_eq!(env(&[("JAW_GRAPHICS", "kitty"), ("TMUX", "/tmp/tmux")]), Some(Protocol::Kitty));
}

#[test]
fn the_environment_names_kitty_iterm_and_sixel_terminals() {
    assert_eq!(env(&[("KITTY_WINDOW_ID", "3"), ("TERM", "xterm-kitty")]), Some(Protocol::Kitty));
    assert_eq!(env(&[("TERM_PROGRAM", "ghostty"), ("TERM", "xterm-ghostty")]), Some(Protocol::Kitty));
    assert_eq!(env(&[("TERM_PROGRAM", "iTerm.app")]), Some(Protocol::Iterm));
    assert_eq!(env(&[("TERM_PROGRAM", "WezTerm")]), Some(Protocol::Iterm));
    assert_eq!(env(&[("TERM", "foot")]), Some(Protocol::Sixel));
}

#[test]
fn tmux_and_terminals_without_graphics_get_half_blocks() {
    assert_eq!(env(&[("TMUX", "/tmp/tmux-501/default"), ("TERM_PROGRAM", "iTerm.app")]), Some(Protocol::Blocks));
    assert_eq!(env(&[("TERM", "screen-256color")]), Some(Protocol::Blocks));
    assert_eq!(env(&[("TERM_PROGRAM", "Apple_Terminal")]), Some(Protocol::Blocks));
}

#[test]
fn a_terminal_the_environment_doesnt_name_is_asked() {
    assert_eq!(env(&[("TERM", "xterm-256color")]), None);
}

#[test]
fn the_terminals_answers_say_kitty_sixel_or_neither() {
    assert_eq!(detect::read_answer(b"\x1b_Gi=31;OK\x1b\\\x1b[?62;22c"), Protocol::Kitty);
    assert_eq!(detect::read_answer(b"\x1b[?62;4;22c"), Protocol::Sixel);
    assert_eq!(detect::read_answer(b"\x1b[?1;2c"), Protocol::Blocks);
    assert_eq!(detect::read_answer(b""), Protocol::Blocks);
}

#[test]
fn half_blocks_are_the_pictures_colours_two_pixels_a_cell() {
    let dir = tempfile::tempdir().unwrap();
    let mut images = Images::here(Protocol::Blocks, CELL);
    let mut buf = screen();
    assert!(images.draw(&mut buf, place(2, 1, 4, 2), ALL, &[halves(dir.path())], look()));
    let (top, bottom) = (&buf[(2, 1)], &buf[(2, 2)]);
    assert_eq!((top.symbol(), top.fg, top.bg), ("▀", Color::Rgb(255, 0, 0), Color::Rgb(255, 0, 0)));
    assert_eq!((bottom.symbol(), bottom.fg, bottom.bg), ("▀", Color::Rgb(0, 0, 255), Color::Rgb(0, 0, 255)));
    assert_eq!(buf[(6, 1)].symbol(), " ", "nothing is drawn past the picture's columns");
}

#[test]
fn half_blocks_in_256_colours_take_the_nearest() {
    let dir = tempfile::tempdir().unwrap();
    let mut images = Images::here(Protocol::Blocks, CELL);
    let mut buf = screen();
    images.draw(&mut buf, place(0, 0, 4, 2), ALL, &[halves(dir.path())], Look { truecolor: false, ..look() });
    assert_eq!(buf[(0, 0)].fg, Color::Indexed(crate::theme::nearest_256(255, 0, 0)));
}

#[test]
fn transparent_pixels_show_the_ground_beneath() {
    let dir = tempfile::tempdir().unwrap();
    let path = picture(dir.path(), "dot.png", 8, 8, |_, y| if y >= 4 { [0, 255, 0, 255] } else { [0, 0, 0, 0] });
    let mut images = Images::here(Protocol::Blocks, CELL);
    let mut buf = screen();
    images.draw(&mut buf, place(0, 0, 2, 2), ALL, &[path], look());
    assert_eq!((buf[(0, 0)].symbol(), buf[(0, 0)].bg), (" ", look().bg));
    assert_eq!((buf[(0, 1)].symbol(), buf[(0, 1)].fg), ("▀", Color::Rgb(0, 255, 0)));
}

#[test]
fn a_picture_partly_above_the_screen_draws_only_its_rows_below_the_top() {
    let dir = tempfile::tempdir().unwrap();
    let mut images = Images::here(Protocol::Blocks, CELL);
    let mut buf = screen();
    let clip = Rect::new(0, 5, 60, 20);
    images.draw(&mut buf, place(0, 4, 4, 2), clip, &[halves(dir.path())], look());
    assert_eq!(buf[(0, 4)].symbol(), " ", "the row above the clip is left alone");
    assert_eq!(buf[(0, 5)].fg, Color::Rgb(0, 0, 255), "its second row is drawn at the clip's top");
}

#[test]
fn kitty_pictures_are_sent_once_at_the_cells_size_and_drawn_as_placeholders() {
    let dir = tempfile::tempdir().unwrap();
    let mut images = Images::here(Protocol::Kitty, CELL);
    let path = halves(dir.path());
    let mut buf = screen();
    assert!(images.draw(&mut buf, place(3, 2, 5, 3), ALL, std::slice::from_ref(&path), look()));
    let (keys, png) = kitty_png(&images.take_writes());
    assert!(keys.starts_with("a=T,U=1,f=100,t=d,q=2,i=1,c=5,r=3"), "{keys}");
    let sent = image::load_from_memory(&png).expect("a PNG");
    assert_eq!((sent.width(), sent.height()), (5 * 8, 3 * 16));

    let first = &buf[(3, 2)];
    assert_eq!(first.symbol(), "\u{10EEEE}\u{0305}\u{0305}");
    assert_eq!(first.fg, Color::Rgb(0, 0, 1), "the foreground is the picture's ID");
    assert_eq!(buf[(4, 2)].symbol(), "\u{10EEEE}\u{0305}\u{030D}", "every cell says its row and column");
    assert_eq!(buf[(3, 4)].symbol(), "\u{10EEEE}\u{030E}\u{0305}", "the third row says so");

    images.draw(&mut screen(), place(3, 9, 5, 3), ALL, &[path], look());
    assert!(images.take_writes().is_empty(), "a picture already sent isn't sent again where it moves");
}

#[test]
fn kitty_pictures_partly_above_the_screen_keep_their_rows() {
    let dir = tempfile::tempdir().unwrap();
    let mut images = Images::here(Protocol::Kitty, CELL);
    let mut buf = screen();
    images.draw(&mut buf, place(0, -2, 4, 4), ALL, &[halves(dir.path())], look());
    assert_eq!(buf[(0, 0)].symbol(), "\u{10EEEE}\u{030E}\u{0305}", "row 0 of the screen is the picture's third");
}

#[test]
fn kitty_keeps_at_most_its_number_of_pictures_in_the_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let path = halves(dir.path());
    let mut images = Images::here(Protocol::Kitty, CELL);
    for i in 0..MOST_IN_TERMINAL as u16 + 3 {
        let (cols, rows) = (1 + i % 60, 1 + i / 60);
        images.draw(&mut screen(), place(0, 0, cols, rows), ALL, std::slice::from_ref(&path), look());
    }
    let writes = String::from_utf8_lossy(&images.take_writes()).to_string();
    assert!(writes.contains("\x1b_Ga=d,d=I,q=2,i=1\x1b\\"), "the oldest is deleted");
    assert!(images.in_terminal.len() <= MOST_IN_TERMINAL);
    let kept: HashSet<u32> =
        images.cache.values().filter_map(|r| if let Ready::Kitty { id, .. } = r { Some(*id) } else { None }).collect();
    assert!(!kept.contains(&1), "what the terminal forgot isn't drawn from the cache");
}

#[test]
fn an_iterm_picture_is_half_blocks_while_it_moves_and_its_own_once_still() {
    let dir = tempfile::tempdir().unwrap();
    let path = vec![halves(dir.path())];
    let mut images = Images::here(Protocol::Iterm, CELL);
    let frame = |images: &mut Images, top: i32| {
        let mut buf = screen();
        images.begin();
        images.draw(&mut buf, place(1, top, 4, 2), ALL, &path, look());
        images.end();
        (buf, String::from_utf8_lossy(&images.take_writes()).to_string())
    };
    let (buf, writes) = frame(&mut images, 3);
    assert_eq!(buf[(1, 3)].symbol(), "▀", "just arrived, it's half-blocks");
    assert!(writes.is_empty());
    assert!(images.next_wake().is_some(), "it asks to be drawn again once still");

    std::thread::sleep(STILL + Duration::from_millis(20));
    let (buf, writes) = frame(&mut images, 3);
    assert!(writes.starts_with("\x1b7\x1b[4;2H\x1b]1337;File=inline=1;"), "{:?}", &writes[..40.min(writes.len())]);
    assert!(writes.contains(";width=4;height=2;preserveAspectRatio=0:"));
    assert!(writes.ends_with("\x07\x1b8"));
    assert_eq!(buf[(1, 3)].diff_option, CellDiffOption::Skip, "the frame leaves its cells to the picture");
    assert_eq!(buf[(5, 3)].diff_option, CellDiffOption::None);

    let (_, writes) = frame(&mut images, 3);
    assert!(writes.is_empty(), "standing where it was drawn, it isn't drawn again");

    let (buf, writes) = frame(&mut images, 2);
    assert!(writes.is_empty());
    assert_eq!(buf[(1, 2)].symbol(), "▀", "moved, it's half-blocks again");
}

#[test]
fn an_escape_waits_while_something_is_drawn_over_the_screen() {
    let dir = tempfile::tempdir().unwrap();
    let path = vec![halves(dir.path())];
    let mut images = Images::here(Protocol::Sixel, CELL);
    let mut buf = screen();
    images.begin();
    images.draw(&mut buf, place(0, 0, 4, 2), ALL, &path, look());
    images.end();
    std::thread::sleep(STILL + Duration::from_millis(20));
    images.begin();
    images.draw(&mut buf, place(0, 0, 4, 2), ALL, &path, Look { over: true, ..look() });
    images.end();
    assert!(images.take_writes().is_empty());
    assert_eq!(buf[(0, 0)].symbol(), "▀");
}

#[test]
fn an_escape_is_drawn_again_after_a_resize() {
    let dir = tempfile::tempdir().unwrap();
    let path = vec![halves(dir.path())];
    let mut images = Images::here(Protocol::Iterm, CELL);
    let frame = |images: &mut Images| {
        images.begin();
        images.draw(&mut screen(), place(0, 0, 4, 2), ALL, &path, look());
        images.end();
        images.take_writes()
    };
    frame(&mut images);
    std::thread::sleep(STILL + Duration::from_millis(20));
    assert!(!frame(&mut images).is_empty());
    images.forget_drawn();
    assert!(!frame(&mut images).is_empty());
}

#[test]
fn a_picture_only_partly_shown_stays_half_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let path = vec![halves(dir.path())];
    let mut images = Images::here(Protocol::Iterm, CELL);
    let clip = Rect::new(0, 2, 60, 20);
    let mut buf = screen();
    images.begin();
    images.draw(&mut buf, place(0, 1, 4, 3), clip, &path, look());
    images.end();
    std::thread::sleep(STILL + Duration::from_millis(20));
    images.draw(&mut buf, place(0, 1, 4, 3), clip, &path, look());
    assert!(images.take_writes().is_empty(), "an escape can't be cut at the clip");
}

#[test]
fn sixel_is_drawn_in_bands_of_six_rows_in_the_cubes_colours() {
    let dir = tempfile::tempdir().unwrap();
    let key = Key { path: halves(dir.path()), cols: 2, rows: 1 };
    let encoded = encode::encode(&key, Protocol::Sixel, CELL, None).unwrap();
    let sixel = String::from_utf8(encoded.escape.unwrap()).unwrap();
    assert!(sixel.starts_with("\x1bP0;1;0q\"1;1;16;16"), "{}", &sixel[..30]);
    assert!(sixel.contains("#180;2;100;0;0") && sixel.contains("#5;2;0;0;100"), "red and blue are defined");
    assert_eq!(sixel.matches('-').count(), 3, "16 rows are three bands");
    assert!(sixel.ends_with("\x1b\\"));
}

#[test]
fn iterms_picture_is_a_png_at_the_cells_size() {
    let dir = tempfile::tempdir().unwrap();
    let key = Key { path: halves(dir.path()), cols: 6, rows: 3 };
    let encoded = encode::encode(&key, Protocol::Iterm, CELL, None).unwrap();
    let text = String::from_utf8(encoded.escape.unwrap()).unwrap();
    let data = text.split(':').nth(1).unwrap().trim_end_matches('\x07');
    let png = image::load_from_memory(&unbase64(data.as_bytes())).unwrap();
    assert_eq!((png.width(), png.height()), (48, 48));
}

#[test]
fn an_album_tile_crops_the_picture_to_fill_it_rather_than_stretch() {
    let dir = tempfile::tempdir().unwrap();
    let tall = picture(dir.path(), "tall.png", 30, 90, |_, y| match y {
        0..10 => [255, 0, 0, 255],
        10..80 => [0, 255, 0, 255],
        _ => [0, 0, 255, 255],
    });
    let mut images = Images::here(Protocol::Blocks, CELL);
    let mut buf = screen();
    images.draw(&mut buf, place(0, 0, 4, 2), ALL, &[tall], look());
    for (x, y) in [(0, 0), (3, 1)] {
        assert_eq!(buf[(x, y)].fg, Color::Rgb(0, 255, 0), "a square tile of a tall picture is its middle");
    }
}

#[test]
fn a_preview_that_cant_be_read_gives_way_to_the_next_and_then_to_none() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing-768.jpg");
    let broken = dir.path().join("broken-256.jpg");
    std::fs::write(&broken, b"not a picture").unwrap();
    let mut images = Images::here(Protocol::Blocks, CELL);
    let sources = vec![missing.clone(), broken.clone(), halves(dir.path())];
    let mut buf = screen();
    assert!(!images.draw(&mut buf, place(0, 0, 4, 2), ALL, &sources, look()));
    assert!(!images.draw(&mut buf, place(0, 0, 4, 2), ALL, &sources, look()));
    assert!(images.draw(&mut buf, place(0, 0, 4, 2), ALL, &sources, look()), "the third is the thumbnail");
    assert!(!images.gone(&sources, 4, 2));
    assert!(images.gone(&[missing, broken], 4, 2), "with none left, the message is its summary");
}

#[test]
fn the_cache_is_bounded_in_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let mut seed = 1u32;
    let noise = picture(dir.path(), "noise.png", 256, 256, |_, _| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let [r, g, b, _] = seed.to_le_bytes();
        [r, g, b, 255]
    });
    let path = vec![noise];
    let mut images = Images::here(Protocol::Iterm, CELL);
    let mut evicted = false;
    for rows in 10..=30 {
        for cols in [60, 58, 56] {
            images.draw(&mut screen(), place(0, 0, cols, rows), ALL, &path, look());
            evicted |=
                images.cache.len() < images.order.len().max(1) || images.cache.len() < ((rows - 10) * 3) as usize;
        }
    }
    assert!(evicted, "the pictures outgrew the cache");
    assert!(images.bytes <= MOST_BYTES, "{} bytes kept", images.bytes);
    assert_eq!(images.bytes, images.cache.values().map(Ready::bytes).sum::<usize>());
    assert_eq!(images.order.len(), images.cache.len());
}

#[test]
fn workers_make_pictures_ready_without_the_drawing_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let path = vec![halves(dir.path())];
    let (tx, rx) = channel();
    let mut images = Images::start(Protocol::Blocks, CELL, tx);
    let mut buf = screen();
    assert!(!images.draw(&mut buf, place(0, 0, 4, 2), ALL, &path, look()), "not ready, it draws nothing");
    assert!(!images.draw(&mut buf, place(0, 0, 4, 2), ALL, &path, look()), "and isn't asked for twice");
    let Ok(Event::Picture(done)) = rx.recv_timeout(Duration::from_secs(5)) else { panic!("no picture came") };
    images.take(*done);
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err(), "one job for one picture");
    assert!(images.draw(&mut buf, place(0, 0, 4, 2), ALL, &path, look()));
}
