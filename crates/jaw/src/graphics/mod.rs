//! Pictures in the terminal: photos, stickers and posters drawn with the terminal's own graphics
//! where it has them, kitty's, iTerm2's or sixel, and as half-blocks where it doesn't. Previews
//! are decoded, scaled and encoded on two workers of their own and kept by path and size in a
//! cache bounded in bytes, so drawing a frame only copies what's ready; what isn't yet draws its
//! space blank, or a smaller copy, and comes in a frame later.
//!
//! kitty's pictures are sent once and drawn as Unicode placeholders, so they move with the text
//! and leave nothing behind. iTerm2's and sixel are drawn over cells the frame leaves alone, and
//! only once a picture has stood still a moment: while the conversation moves they're half-blocks,
//! so scrolling never leaves a picture where it was.

mod detect;
mod encode;
mod kitty;

pub use detect::{Protocol, detect};

use crate::run::Event;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The cache's most, in bytes of what it keeps: half-blocks, and iTerm2's and sixel's encodings.
const MOST_BYTES: usize = 12 << 20;
/// kitty pictures the terminal is asked to keep at once; older ones are deleted from it.
const MOST_IN_TERMINAL: usize = 96;
/// How long a picture stands still before iTerm2 or sixel draw it, rather than half-blocks.
pub const STILL: Duration = Duration::from_millis(150);
/// The workers that decode, scale and encode.
const WORKERS: usize = 2;

pub type Rgb = [u8; 3];

/// A half-block cell: its top pixel and its bottom one, `None` where the picture is transparent.
pub type Block = (Option<Rgb>, Option<Rgb>);

/// A picture at a size: its file and the cells it fills.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub path: PathBuf,
    pub cols: u16,
    pub rows: u16,
}

/// A picture ready to draw.
#[derive(Clone, Debug)]
pub enum Ready {
    /// Two pixels a cell, the top one the glyph's colour and the bottom the cell's; `None` where
    /// the picture is transparent.
    Blocks(Arc<Vec<Block>>),
    /// Sent to the terminal under this ID, and drawn as placeholders.
    Kitty { id: u32, blocks: Arc<Vec<Block>> },
    /// iTerm2's or sixel's escape, drawn where the picture stands still, half-blocks meanwhile.
    Escape { bytes: Arc<Vec<u8>>, blocks: Arc<Vec<Block>> },
}

impl Ready {
    fn blocks(&self) -> &[Block] {
        match self {
            Ready::Blocks(b) | Ready::Kitty { blocks: b, .. } | Ready::Escape { blocks: b, .. } => b,
        }
    }

    fn bytes(&self) -> usize {
        let cells = self.blocks().len() * 8;
        match self {
            Ready::Escape { bytes, .. } => cells + bytes.len(),
            _ => cells,
        }
    }
}

/// A worker's answer.
pub struct Done {
    key: Key,
    outcome: Result<Encoded, String>,
}

struct Encoded {
    blocks: Vec<Block>,
    /// For kitty, what sends it to the terminal; for iTerm2 and sixel, what draws it.
    escape: Option<Vec<u8>>,
    kitty: Option<u32>,
}

struct Job {
    key: Key,
    protocol: Protocol,
    cell: (u16, u16),
    kitty: Option<u32>,
}

/// Where a picture was drawn with iTerm2's or sixel's escape, and since when it stood there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Spot {
    x: u16,
    y: u16,
}

pub struct Images {
    pub protocol: Protocol,
    /// A cell's size in pixels.
    pub cell: (u16, u16),
    root: PathBuf,
    cache: HashMap<Key, Ready>,
    /// Least recently drawn first.
    order: VecDeque<Key>,
    bytes: usize,
    waiting: HashSet<Key>,
    failed: HashSet<Key>,
    jobs: Option<Sender<Job>>,
    next_kitty: u32,
    in_terminal: VecDeque<u32>,
    /// What to write to the terminal: kitty's pictures and deletions, iTerm2's and sixel's
    /// drawings, in order.
    writes: Vec<u8>,
    /// Where pictures stood last frame and since when, for iTerm2 and sixel.
    stood: HashMap<(Key, Spot), Instant>,
    standing: HashMap<(Key, Spot), Instant>,
    /// What was drawn with an escape last frame, and where.
    drawn: HashSet<(Key, u16, u16)>,
    drawing: HashSet<(Key, u16, u16)>,
    /// When a picture standing still will be drawn with its escape.
    settles: Option<Instant>,
}

impl Images {
    /// Pictures drawn on two workers of their own, their answers coming on `tx`.
    pub fn start(protocol: Protocol, cell: (u16, u16), tx: Sender<Event>) -> Images {
        let (jobs, queue) = channel::<Job>();
        let queue = Arc::new(Mutex::new(queue));
        for n in 0..WORKERS {
            let (queue, tx) = (queue.clone(), tx.clone());
            std::thread::Builder::new()
                .name(format!("jaw-images-{n}"))
                .spawn(move || work(&queue, &tx))
                .expect("an image worker");
        }
        Images { jobs: Some(jobs), ..Images::here(protocol, cell) }
    }

    /// Pictures drawn as they're asked for, on the drawing thread, for frames that must come out
    /// whole: snapshots and tests.
    pub fn here(protocol: Protocol, cell: (u16, u16)) -> Images {
        Images {
            protocol,
            cell,
            root: PathBuf::new(),
            cache: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            waiting: HashSet::new(),
            failed: HashSet::new(),
            jobs: None,
            next_kitty: 1,
            in_terminal: VecDeque::new(),
            writes: vec![],
            stood: HashMap::new(),
            standing: HashMap::new(),
            drawn: HashSet::new(),
            drawing: HashSet::new(),
            settles: None,
        }
    }

    /// jawd's folder, which the paths in its rows are under.
    pub fn set_root(&mut self, root: &str) {
        self.root = PathBuf::from(root);
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        let p = Path::new(relative);
        if p.is_absolute() { p.to_path_buf() } else { self.root.join(p) }
    }

    /// What's to be written to the terminal now.
    pub fn take_writes(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.writes)
    }

    /// When a picture that's stood still will be drawn with its escape, if one is waiting to be.
    pub fn next_wake(&self) -> Option<Instant> {
        self.settles
    }

    fn ask(&mut self, key: &Key) {
        if self.waiting.contains(key) || self.failed.contains(key) {
            return;
        }
        let kitty = (self.protocol == Protocol::Kitty).then(|| {
            let id = self.next_kitty;
            self.next_kitty = self.next_kitty % 0x00ff_ffff + 1;
            id
        });
        let job = Job { key: key.clone(), protocol: self.protocol, cell: self.cell, kitty };
        match &self.jobs {
            Some(jobs) => {
                self.waiting.insert(key.clone());
                let _ = jobs.send(job);
            }
            None => {
                let done = run(job);
                self.take(done);
            }
        }
    }

    /// A worker's answer: kept, and for kitty sent to the terminal before it's drawn.
    pub fn take(&mut self, done: Done) {
        self.waiting.remove(&done.key);
        match done.outcome {
            Ok(e) => {
                let blocks = Arc::new(e.blocks);
                let ready = match (self.protocol, e.escape, e.kitty) {
                    (Protocol::Kitty, Some(send), Some(id)) => {
                        self.writes.extend(send);
                        self.in_terminal.push_back(id);
                        while self.in_terminal.len() > MOST_IN_TERMINAL {
                            if let Some(old) = self.in_terminal.front().copied() {
                                self.forget_kitty(old);
                            }
                        }
                        Ready::Kitty { id, blocks }
                    }
                    (Protocol::Iterm | Protocol::Sixel, Some(bytes), _) => {
                        Ready::Escape { bytes: Arc::new(bytes), blocks }
                    }
                    _ => Ready::Blocks(blocks),
                };
                self.bytes += ready.bytes();
                self.cache.insert(done.key.clone(), ready);
                self.order.push_back(done.key);
                self.evict();
            }
            Err(_) => {
                self.failed.insert(done.key);
            }
        }
    }

    fn evict(&mut self) {
        while self.bytes > MOST_BYTES {
            let Some(old) = self.order.front().cloned() else { break };
            self.remove(&old);
        }
    }

    /// Drops a picture from the cache, and from the terminal if kitty keeps it there.
    fn remove(&mut self, key: &Key) {
        self.order.retain(|k| k != key);
        if let Some(gone) = self.cache.remove(key) {
            self.bytes = self.bytes.saturating_sub(gone.bytes());
            if let Ready::Kitty { id, .. } = gone {
                self.in_terminal.retain(|i| *i != id);
                self.writes.extend(kitty::delete(id));
            }
        }
    }

    /// Deletes kitty's picture `id` from the terminal, and so from the cache: drawn again, it's
    /// sent again.
    fn forget_kitty(&mut self, id: u32) {
        let key = self
            .cache
            .iter()
            .find(|(_, r)| matches!(r, Ready::Kitty { id: i, .. } if *i == id))
            .map(|(k, _)| k.clone());
        match key {
            Some(key) => self.remove(&key),
            None => {
                self.in_terminal.retain(|i| *i != id);
                self.writes.extend(kitty::delete(id));
            }
        }
    }

    fn touch(&mut self, key: &Key) {
        if let Some(at) = self.order.iter().position(|k| k == key)
            && let Some(k) = self.order.remove(at)
        {
            self.order.push_back(k);
        }
    }

    /// The best of `sources` that's ready, asking for the best one not yet ready.
    fn pick(&mut self, sources: &[PathBuf], cols: u16, rows: u16) -> Option<Key> {
        let mut asked = false;
        for path in sources {
            let key = Key { path: path.clone(), cols, rows };
            if self.cache.contains_key(&key) {
                self.touch(&key);
                return Some(key);
            }
            if !asked && !self.failed.contains(&key) {
                self.ask(&key);
                asked = true;
                if self.cache.contains_key(&key) {
                    return Some(key);
                }
            }
        }
        None
    }

    /// Whether every source has been tried and none could be read.
    pub fn gone(&self, sources: &[PathBuf], cols: u16, rows: u16) -> bool {
        sources.iter().all(|path| self.failed.contains(&Key { path: path.clone(), cols, rows }))
    }

    /// The next frame is drawn whole, as after a resize: every escape is drawn again.
    pub fn forget_drawn(&mut self) {
        self.drawn.clear();
    }

    /// Begins a frame: what's drawn with escapes is noted afresh.
    pub fn begin(&mut self) {
        self.drawing.clear();
        self.settles = None;
    }

    /// Ends a frame: what it drew with escapes the terminal keeps, so the next needn't again.
    pub fn end(&mut self) {
        self.drawn = std::mem::take(&mut self.drawing);
        self.stood = std::mem::take(&mut self.standing);
    }

    /// Draws a picture at `place`, as much of it as `clip` shows; says whether it was ready.
    pub fn draw(&mut self, buf: &mut Buffer, place: Place, clip: Rect, sources: &[PathBuf], look: Look) -> bool {
        let Look { bg, truecolor, over } = look;
        let Place { x, top, cols, rows } = place;
        let Some(key) = self.pick(sources, cols, rows) else { return false };
        let Some(ready) = self.cache.get(&key).cloned() else { return false };
        let whole = top >= clip.y as i32 && top + rows as i32 <= (clip.y + clip.height) as i32;
        match &ready {
            Ready::Kitty { id, .. } => kitty::placeholders(buf, place, clip, *id),
            Ready::Escape { bytes, blocks } => {
                let spot = Spot { x, y: top.max(0) as u16 };
                let now = Instant::now();
                let since = self.stood.get(&(key.clone(), spot)).copied().unwrap_or(now);
                self.standing.insert((key.clone(), spot), since);
                let still = now.duration_since(since) >= STILL;
                if whole && still && !over {
                    self.escape(buf, Rect::new(x, top as u16, cols, rows), &key, bytes);
                } else {
                    half_blocks(buf, place, clip, blocks, bg, truecolor);
                    if whole && !over {
                        let at = since + STILL;
                        self.settles = Some(self.settles.map_or(at, |s| s.min(at)));
                    }
                }
            }
            Ready::Blocks(blocks) => half_blocks(buf, place, clip, blocks, bg, truecolor),
        }
        true
    }

    /// iTerm2's or sixel's drawing: the cells left to it, and drawn again only where it wasn't
    /// last frame, since the terminal keeps it while the cells under it don't change.
    fn escape(&mut self, buf: &mut Buffer, area: Rect, key: &Key, bytes: &[u8]) {
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                buf[(x, y)].set_diff_option(ratatui::buffer::CellDiffOption::Skip);
            }
        }
        let at = (key.clone(), area.x, area.y);
        if !self.drawn.contains(&at) {
            self.writes.extend(format!("\x1b7\x1b[{};{}H", area.y + 1, area.x + 1).as_bytes());
            self.writes.extend(bytes);
            self.writes.extend(b"\x1b8");
        }
        self.drawing.insert(at);
    }
}

/// How a frame draws its pictures: the ground under a transparent pixel, whether colours are
/// 24-bit, and whether something is drawn over the screen, so escapes wait.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub bg: Color,
    pub truecolor: bool,
    pub over: bool,
}

/// Where a picture is drawn: its first column and row, which can be above the screen, and its
/// size in cells.
#[derive(Clone, Copy, Debug)]
pub struct Place {
    pub x: u16,
    pub top: i32,
    pub cols: u16,
    pub rows: u16,
}

impl Place {
    /// The screen rows of the picture's rows that `clip` shows.
    pub fn rows_in(self, clip: Rect) -> impl Iterator<Item = (u16, u16)> {
        let (from, to) = (clip.y as i32, (clip.y + clip.height) as i32);
        (0..self.rows).filter_map(move |row| {
            let y = self.top + row as i32;
            (y >= from && y < to).then_some((row, y as u16))
        })
    }
}

/// Two pixels a cell, as much of the picture as `clip` shows.
fn half_blocks(buf: &mut Buffer, area: Place, clip: Rect, blocks: &[Block], bg: Color, truecolor: bool) {
    let colour = |p: Rgb| {
        if truecolor {
            Color::Rgb(p[0], p[1], p[2])
        } else {
            Color::Indexed(crate::theme::nearest_256(p[0], p[1], p[2]))
        }
    };
    for (row, y) in area.rows_in(clip) {
        for col in 0..area.cols {
            let Some(&(top, bottom)) = blocks.get(row as usize * area.cols as usize + col as usize) else { continue };
            let cell = &mut buf[(area.x + col, y)];
            match (top, bottom) {
                (Some(t), Some(b)) => cell.set_char('▀').set_fg(colour(t)).set_bg(colour(b)),
                (Some(t), None) => cell.set_char('▀').set_fg(colour(t)).set_bg(bg),
                (None, Some(b)) => cell.set_char('▄').set_fg(colour(b)).set_bg(bg),
                (None, None) => cell.set_char(' ').set_bg(bg),
            };
        }
    }
}

fn run(job: Job) -> Done {
    let outcome = encode::encode(&job.key, job.protocol, job.cell, job.kitty);
    Done { key: job.key, outcome }
}

fn work(queue: &Mutex<Receiver<Job>>, tx: &Sender<Event>) {
    loop {
        let job = match queue.lock() {
            Ok(q) => q.recv(),
            Err(_) => return,
        };
        let Ok(job) = job else { return };
        if tx.send(Event::Picture(Box::new(run(job)))).is_err() {
            return;
        }
    }
}

/// How many cells a picture of `w` by `h` pixels fills, at most `cols` by `rows`, keeping its
/// shape on cells of `cell` pixels.
pub fn fit(w: u32, h: u32, cols: u16, rows: u16, cell: (u16, u16)) -> (u16, u16) {
    let (w, h) = (w.max(1) as f64, h.max(1) as f64);
    let (cw, ch) = (cell.0.max(1) as f64, cell.1.max(1) as f64);
    let mut c = cols as f64;
    let mut r = (c * cw * h / w / ch).round();
    if r > rows as f64 {
        r = rows as f64;
        c = (r * ch * w / h / cw).round();
    }
    (c.clamp(1.0, cols as f64) as u16, r.clamp(1.0, rows as f64) as u16)
}

#[cfg(test)]
mod tests;
