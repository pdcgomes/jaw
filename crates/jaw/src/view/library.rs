//! The library: every photo, video, voice message, file and link shared, across all chats over
//! the whole screen, or in one chat in the conversation's pane, as the wireframes draw it. It's a
//! window jawd keeps (`library.watch`), newest first and paged by keyset; the kind strip and Tab
//! change kind, the search line takes words and `from:`, `in:`, `since:` and `until:` filters
//! with completion, Enter opens an item and Ctrl-G goes to its message.

use super::{Focus, View};
use crate::composer::Composer;
use crate::data::{LibraryAsk, LibraryKind, LibraryRow, LibraryWindow, MarkKind};
use crate::fmt;
use chrono::{Datelike, Days, Months, NaiveDate};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding};
use std::collections::HashMap;

/// A tile in the grid: the preview's cells and a caption under them, with a gap between tiles.
const TILE: (u16, u16) = (16, 4);
const GAP: u16 = 2;
/// Items asked of each kind when all show, and of one kind a page at a time.
const SHELF: u32 = 14;
const PAGE: u32 = 120;
/// The most items held, past which the oldest pages go.
const MOST: usize = 2_000;
/// The kinds in the strip's order, all of them first.
const KINDS: [Option<LibraryKind>; 6] = [
    None,
    Some(LibraryKind::Photo),
    Some(LibraryKind::Video),
    Some(LibraryKind::Voice),
    Some(LibraryKind::File),
    Some(LibraryKind::Link),
];

/// A filter set on the search line, shown as a chip.
#[derive(Clone, Debug, PartialEq)]
pub enum Chip {
    From { id: String, name: String },
    In { id: String, name: String },
    Since { at: i64, label: String },
    Until { at: i64, label: String },
}

impl Chip {
    fn label(&self) -> String {
        match self {
            Chip::From { name, .. } => format!("from: {name}"),
            Chip::In { name, .. } => format!("in: {name}"),
            Chip::Since { label, .. } => format!("since: {label}"),
            Chip::Until { label, .. } => format!("until: {label}"),
        }
    }
}

/// What a filter being typed could become.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub chip: Chip,
    pub said: String,
}

#[derive(Debug, Default)]
pub struct Library {
    /// The chat whose library it is, or none for all chats.
    pub chat: Option<(String, String)>,
    pub kind: Option<LibraryKind>,
    pub query: Composer,
    pub chips: Vec<Chip>,
    pub rows: Vec<LibraryRow>,
    pub total: u32,
    pub older: bool,
    pub counts: HashMap<LibraryKind, u32>,
    /// The latest ask, the page asked for if one is, and whether it's been answered.
    pub asked: u64,
    pub paging: Option<u64>,
    pub answered: bool,
    pub refused: Option<String>,
    /// The item selected, by its `id`, and the first line shown.
    pub at: Option<String>,
    pub top: usize,
    /// The photo shown full size.
    pub viewing: bool,
    /// The offer selected while a filter is typed, and the filter Esc put away.
    pub offer: usize,
    pub dismissed: Option<String>,
    /// The words the latest ask searched for.
    pub words: String,
    /// What jawd says the library can't hold yet, as its latest answer said.
    pub note: Option<String>,
    /// The items last named to jawd as on screen, so it's told again only when they change.
    pub shown: Vec<String>,
}

/// Where an item is drawn: its line from the top of the library's content, its column and size.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub item: usize,
    pub line: usize,
    pub col: u16,
    pub cols: u16,
    pub rows: u16,
    pub tile: bool,
}

/// The library's content laid out at a width: where each item goes, and the headings between.
#[derive(Clone, Debug, Default)]
pub struct Laid {
    pub slots: Vec<Slot>,
    pub heads: Vec<(usize, String, String)>,
    pub lines: usize,
}

/// The day an item was shared, short, as captions say it.
fn short(at: i64, now: i64, tz: chrono::FixedOffset) -> String {
    fmt::when(at, now, tz)
}

/// The most of a kind jawd counts when the library has a filter other than its chat; that many
/// means at least that many.
const COUNTED: u32 = 1_000;

/// How many there are, as the strip and the headings say it: `1,000+` where jawd stopped counting.
fn counted(n: u32, capped: bool) -> String {
    if capped && n >= COUNTED { format!("{}+", fmt::num(n as u64)) } else { fmt::num(n as u64) }
}

/// A month separator's label: August 2026.
fn month(at: i64, tz: chrono::FixedOffset) -> String {
    chrono::DateTime::from_timestamp_millis(at)
        .map(|d| d.with_timezone(&tz).format("%B %Y").to_string())
        .unwrap_or_default()
}

impl View {
    /// Ctrl-L: the open chat's library, or all chats' when none is open; again, the other one.
    pub(super) fn library_key(&mut self) {
        match self.library.as_ref().map(|l| l.chat.is_some()) {
            Some(true) => self.open_library(None, None, ""),
            Some(false) => match self.library_chat() {
                Some(chat) => self.open_library(Some(chat), None, ""),
                None => self.close_library(),
            },
            None => {
                let chat = self.library_chat();
                self.open_library(chat, None, "");
            }
        }
    }

    /// The chat a library would be for: the one open, while one is.
    fn library_chat(&self) -> Option<(String, String)> {
        let id = self.open.clone()?;
        let name = self.chat_known(&id).map_or_else(|| id.clone(), |c| c.name.clone());
        Some((id, name))
    }

    /// Opens the library on a chat or all chats, at a kind, with words already typed.
    pub fn open_library(&mut self, chat: Option<(String, String)>, kind: Option<LibraryKind>, words: &str) {
        let mut l = Library { chat, kind, ..Library::default() };
        l.query.set(words);
        self.library = Some(l);
        self.focus = Focus::Typing;
        self.ask_library(false);
    }

    pub(super) fn close_library(&mut self) {
        if self.library.take().is_some() {
            self.jawd.unwatch_library();
        }
    }

    /// The words typed, without a filter still being typed at the end.
    fn library_words(&self) -> String {
        let Some(l) = &self.library else { return String::new() };
        let text = l.query.text();
        match self.typing_filter() {
            Some((at, _, _)) => text[..at].trim_end().to_string(),
            None => text.trim().to_string(),
        }
    }

    /// Asks jawd for what the library shows now: from the top, or the page after what it holds.
    pub(super) fn ask_library(&mut self, page: bool) {
        let words = self.library_words();
        self.asked += 1;
        let id = self.asked;
        let Some(l) = self.library.as_mut() else { return };
        l.words = words.clone();
        let mut ask = LibraryAsk {
            id,
            kind: l.kind,
            text: words,
            around: "top".into(),
            before: 0,
            after: PAGE,
            ..LibraryAsk::default()
        };
        ask.chat = l.chat.as_ref().map(|(id, _)| id.clone());
        for chip in &l.chips {
            match chip {
                Chip::From { id, .. } => ask.person = Some(id.clone()),
                Chip::In { id, .. } => ask.chat = Some(id.clone()),
                Chip::Since { at, .. } => ask.since = Some(*at),
                Chip::Until { at, .. } => ask.until = Some(*at),
            }
        }
        if l.kind.is_none() {
            ask.after = SHELF;
        }
        if page {
            let Some(last) = l.rows.last() else { return };
            ask.around = last.id.clone();
            l.paging = Some(id);
        } else {
            l.answered = false;
            l.paging = None;
        }
        l.asked = id;
        l.refused = None;
        self.jawd.watch_library(&ask);
    }

    pub(super) fn on_library(&mut self, asked: u64, window: LibraryWindow) {
        let LibraryWindow { upsert, remove, whole, total, older, counts, note } = window;
        let Some(l) = self.library.as_mut().filter(|l| l.asked == asked) else { return };
        if whole {
            l.note = note;
        }
        let paged = l.paging == Some(asked);
        if whole && !paged {
            l.rows = upsert;
            l.top = 0;
            l.answered = true;
        } else {
            for row in upsert {
                match l.rows.iter_mut().find(|r| r.id == row.id) {
                    Some(r) => *r = row,
                    None => l.rows.push(row),
                }
            }
            l.rows.retain(|r| !remove.contains(&r.id));
            l.rows.sort_by(|a, b| b.sort.cmp(&a.sort));
            l.rows.truncate(MOST);
        }
        if whole {
            l.paging = None;
        }
        if let Some(t) = total {
            l.total = t;
        }
        if let Some(o) = older {
            l.older = o;
        }
        if let Some(c) = counts {
            l.counts = c;
        }
        let lost = l.at.as_ref().is_none_or(|at| !l.rows.iter().any(|r| r.id == *at));
        if lost {
            let first = self.library_layout(self.library_width).slots.first().map(|s| s.item);
            if let Some(l) = self.library.as_mut() {
                l.at = first.and_then(|i| l.rows.get(i)).map(|r| r.id.clone());
            }
        }
        self.dirty = true;
    }

    pub(super) fn library_refused(&mut self, why: String) {
        if let Some(l) = self.library.as_mut() {
            l.refused = Some(why);
            l.answered = true;
        }
    }

    /// Whether jawd counts the library's items only so far: with words, a person or a time.
    fn capped(&self) -> bool {
        self.library
            .as_ref()
            .is_some_and(|l| !l.words.is_empty() || l.chips.iter().any(|chip| !matches!(chip, Chip::In { .. })))
    }

    /// The items laid out at `width`: shelves of each kind when all show, or one kind under its
    /// months, photos and videos in a grid and the rest as rows.
    pub(super) fn library_layout(&self, width: u16) -> Laid {
        let Some(l) = &self.library else { return Laid::default() };
        let per_row = ((width + GAP) / (TILE.0 + GAP)).max(1) as usize;
        let narrow = width < 100;
        let selected = l.at.as_deref();
        let mut laid = Laid::default();
        let mut line = 0;
        let row_height = |r: &LibraryRow| -> u16 {
            let open = selected == Some(r.id.as_str()) && r.kind == LibraryKind::Link;
            let base = if narrow && r.kind != LibraryKind::Voice { 2 } else { 1 };
            base + u16::from(open && r.link.as_ref().is_some_and(|k| k.description.is_some()))
        };
        let place_tiles = |laid: &mut Laid, line: &mut usize, items: &[usize]| {
            for chunk in items.chunks(per_row) {
                for (c, &item) in chunk.iter().enumerate() {
                    laid.slots.push(Slot {
                        item,
                        line: *line,
                        col: c as u16 * (TILE.0 + GAP),
                        cols: TILE.0,
                        rows: TILE.1 + 1,
                        tile: true,
                    });
                }
                *line += TILE.1 as usize + 1;
            }
        };
        let place_rows = |laid: &mut Laid, line: &mut usize, items: &[usize]| {
            for &item in items {
                let h = row_height(&l.rows[item]);
                laid.slots.push(Slot { item, line: *line, col: 0, cols: width, rows: h, tile: false });
                *line += h as usize + usize::from(narrow);
            }
        };
        let of = |kinds: &[LibraryKind]| -> Vec<usize> {
            l.rows.iter().enumerate().filter(|(_, r)| kinds.contains(&r.kind)).map(|(i, _)| i).collect()
        };
        let count =
            |kinds: &[LibraryKind]| -> u32 { kinds.iter().map(|k| l.counts.get(k).copied().unwrap_or(0)).sum() };
        let capped = self.capped();
        match l.kind {
            None => {
                let shelves: [(&str, &[LibraryKind], usize); 4] = [
                    ("photos and videos", &[LibraryKind::Photo, LibraryKind::Video], per_row * 2),
                    ("files", &[LibraryKind::File], 3),
                    ("links", &[LibraryKind::Link], 3),
                    ("voice messages", &[LibraryKind::Voice], 2),
                ];
                for (label, kinds, most) in shelves {
                    let items: Vec<usize> = of(kinds).into_iter().take(most).collect();
                    if items.is_empty() {
                        if let (true, Some(note)) = (kinds == [LibraryKind::Link], &l.note) {
                            laid.heads.push((line, label.to_string(), note.clone()));
                            line += 2;
                        }
                        continue;
                    }
                    laid.heads.push((line, label.to_string(), counted(count(kinds), capped)));
                    line += 1;
                    if kinds.contains(&LibraryKind::Photo) {
                        place_tiles(&mut laid, &mut line, &items);
                    } else {
                        place_rows(&mut laid, &mut line, &items);
                    }
                    line += 1;
                }
            }
            Some(kind) => {
                let items = of(&[kind]);
                let mut start = 0;
                while start < items.len() {
                    let m = month(l.rows[items[start]].at, self.tz);
                    let end = items[start..]
                        .iter()
                        .position(|&i| month(l.rows[i].at, self.tz) != m)
                        .map_or(items.len(), |p| start + p);
                    laid.heads.push((line, m, String::new()));
                    line += 1;
                    if matches!(kind, LibraryKind::Photo | LibraryKind::Video) {
                        place_tiles(&mut laid, &mut line, &items[start..end]);
                    } else {
                        place_rows(&mut laid, &mut line, &items[start..end]);
                    }
                    line += 1;
                    start = end;
                }
            }
        }
        laid.lines = line;
        laid
    }

    /// The filter being typed at the end of the search line: where it starts, its name and what
    /// follows the colon.
    fn typing_filter(&self) -> Option<(usize, &'static str, String)> {
        let l = self.library.as_ref()?;
        let text = &l.query.text()[..l.query.cursor()];
        let start = text.rfind(' ').map_or(0, |i| i + 1);
        let word = &text[start..];
        for name in ["from:", "in:", "since:", "until:"] {
            if let Some(rest) = word.strip_prefix(name) {
                return Some((start, name, rest.to_string()));
            }
        }
        // A name of several words: the last filter whose value hasn't been taken.
        for name in ["from:", "in:", "since:", "until:"] {
            if let Some(at) = text.rfind(name).filter(|at| *at == 0 || text[..*at].ends_with(' ')) {
                let rest = &text[at + name.len()..];
                if !rest.is_empty() && rest.split(' ').count() <= 3 {
                    return Some((at, name, rest.to_string()));
                }
            }
        }
        None
    }

    /// What the filter being typed could become, best first.
    pub(super) fn offers(&self) -> Vec<Offer> {
        let Some((_, name, rest)) = self.typing_filter() else { return vec![] };
        if self.library.as_ref().is_some_and(|l| l.dismissed.as_deref() == Some(l.query.text())) {
            return vec![];
        }
        let matches = |s: &str| rest.trim().is_empty() || crate::find::name(rest.trim(), s).is_some();
        let mut out = vec![];
        match name {
            "from:" => {
                let mut seen: Vec<(String, String)> = vec![("me".into(), "you".into())];
                for r in &self.roster {
                    seen.push((r.id.clone(), r.name.clone()));
                }
                if let Some(l) = &self.library {
                    for r in &l.rows {
                        if let Some(f) = &r.from {
                            seen.push((f.id.clone(), f.name.clone()));
                        }
                    }
                }
                let mut ids = std::collections::HashSet::new();
                for (id, name) in seen.into_iter().filter(|(id, _)| ids.insert(id.clone())) {
                    if matches(&name) {
                        out.push(Offer { said: name.clone(), chip: Chip::From { id, name } });
                    }
                }
            }
            "in:" => {
                for c in self.chats.rows().filter(|c| matches(&c.name)) {
                    out.push(Offer { said: c.name.clone(), chip: Chip::In { id: c.id.clone(), name: c.name.clone() } });
                }
            }
            _ => {
                let since = name == "since:";
                for (label, first, last) in self.dates(&rest) {
                    let chip = if since {
                        Chip::Since { at: first, label: label.clone() }
                    } else {
                        Chip::Until { at: last, label: label.clone() }
                    };
                    out.push(Offer { said: label, chip });
                }
            }
        }
        out.truncate(6);
        out
    }

    /// Dates a `since:` or `until:` could mean: a day typed, or the usual ones and recent months.
    /// Each with its first and last millisecond in the terminal's time zone, however long its
    /// clocks make it, so `until:` takes in the whole of it, as the Mac's do.
    fn dates(&self, typed: &str) -> Vec<(String, i64, i64)> {
        let zone = &*self.zone;
        // From the first millisecond of `first` to the last before `next` starts.
        let span = |label: String, first: NaiveDate, next: Option<NaiveDate>| {
            Some((label, zone.start(first)?, zone.start(next?)? - 1))
        };
        if let Ok(d) = NaiveDate::parse_from_str(typed.trim(), "%Y-%m-%d") {
            return span(d.format("%-d %b %Y").to_string(), d, d.succ_opt()).into_iter().collect();
        }
        let Some(today) = zone.day(self.now()) else { return vec![] };
        let days = [("today", 0), ("yesterday", 1), ("a week ago", 7), ("a month ago", 30), ("a year ago", 365)];
        let mut out: Vec<_> = days
            .into_iter()
            .filter_map(|(label, back)| {
                let day = today.checked_sub_days(Days::new(back))?;
                span(label.to_string(), day, day.succ_opt())
            })
            .collect();
        let months = std::iter::successors(today.with_day(1), |first| first.checked_sub_months(Months::new(1)));
        for first in months.take(12) {
            out.extend(span(first.format("%B %Y").to_string(), first, first.checked_add_months(Months::new(1))));
        }
        let t = typed.trim().to_lowercase();
        out.retain(|(label, ..)| t.is_empty() || crate::find::fold(label).starts_with(&crate::find::fold(&t)[..]));
        out
    }

    /// Takes the offer selected: the filter becomes a chip, and the search asks again.
    fn take_offer(&mut self) -> bool {
        let offers = self.offers();
        let Some((start, _, _)) = self.typing_filter() else { return false };
        let Some(l) = self.library.as_mut() else { return false };
        let Some(offer) = offers.get(l.offer.min(offers.len().saturating_sub(1))).cloned() else { return false };
        l.query.replace_back_to(start, "");
        l.chips.retain(|c| std::mem::discriminant(c) != std::mem::discriminant(&offer.chip));
        l.chips.push(offer.chip);
        l.offer = 0;
        self.ask_library(false);
        true
    }

    fn selected_item(&self) -> Option<LibraryRow> {
        let l = self.library.as_ref()?;
        l.rows.iter().find(|r| Some(&r.id) == l.at.as_ref()).cloned()
    }

    /// Moves the selection to the slot that way, as the eye would: across a row of tiles, up and
    /// down to the nearest column.
    fn library_move(&mut self, dx: i32, dy: i32) {
        let laid = self.library_layout(self.library_width);
        let Some(l) = self.library.as_mut() else { return };
        if laid.slots.is_empty() {
            return;
        }
        let i = laid
            .slots
            .iter()
            .position(|s| l.rows.get(s.item).is_some_and(|r| Some(&r.id) == l.at.as_ref()))
            .unwrap_or(0);
        let here = &laid.slots[i];
        let next = if dx != 0 {
            let j = (i as i32 + dx).clamp(0, laid.slots.len() as i32 - 1) as usize;
            Some(j)
        } else {
            let lines: Vec<usize> = {
                let mut v: Vec<usize> = laid.slots.iter().map(|s| s.line).collect();
                v.dedup();
                v
            };
            let at = lines.iter().position(|&ln| ln == here.line).unwrap_or(0) as i32;
            let target = (at + dy).clamp(0, lines.len() as i32 - 1) as usize;
            let line = lines[target];
            laid.slots
                .iter()
                .enumerate()
                .filter(|(_, s)| s.line == line)
                .min_by_key(|(_, s)| (s.col as i32 - here.col as i32).abs())
                .map(|(j, _)| j)
        };
        if let Some(j) = next {
            l.at = l.rows.get(laid.slots[j].item).map(|r| r.id.clone());
            let near_end = laid.slots.len() - j <= 2 * ((self.library_width + GAP) / (TILE.0 + GAP)).max(1) as usize;
            if near_end && l.older && l.kind.is_some() && l.paging.is_none() {
                self.ask_library(true);
            }
        }
    }

    /// Enter: the photo full size, a video or file in the system's app, a link in the browser, a
    /// voice message played in place.
    fn library_open(&mut self) {
        let Some(item) = self.selected_item() else { return };
        match item.kind {
            LibraryKind::Photo => {
                if let Some(l) = self.library.as_mut() {
                    l.viewing = true;
                }
            }
            LibraryKind::Video => {
                let path = item.media.as_ref().and_then(|m| m.path.clone().or(m.previews.large.clone()));
                match path {
                    Some(p) => {
                        self.effects.push(super::Effect::Open(self.images.path(&p).display().to_string()));
                        self.say("opened in the system's player");
                    }
                    None => self.say("it opens once jawd has it"),
                }
            }
            LibraryKind::File => match item.file.as_ref().and_then(|f| f.path.clone()) {
                Some(p) => {
                    self.effects.push(super::Effect::Open(self.images.path(&p).display().to_string()));
                    self.say("opened");
                }
                None => self.say("it opens once jawd has downloaded it"),
            },
            LibraryKind::Link => {
                if let Some(link) = &item.link {
                    self.effects.push(super::Effect::Open(link.url.clone()));
                    self.say("opened in the browser");
                }
            }
            LibraryKind::Voice => {
                let seconds = item.voice.as_ref().map_or(0.0, |v| v.seconds);
                self.toggle_voice_id(&item.message, seconds);
            }
        }
    }

    /// Ctrl-G: the item's message in its chat, the library closed.
    fn library_go(&mut self) {
        let Some(item) = self.selected_item() else { return };
        self.close_library();
        self.go_to_message(&item.chat, &item.message);
    }

    /// Keys while the library is open.
    pub(super) fn on_library_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        let offering = !self.offers().is_empty();
        if self.library.as_ref().is_some_and(|l| l.viewing) {
            match k.code {
                KeyCode::Left => self.view_step(-1),
                KeyCode::Right => self.view_step(1),
                KeyCode::Char('g') if ctrl => self.library_go(),
                _ => {
                    if let Some(l) = self.library.as_mut() {
                        l.viewing = false;
                    }
                }
            }
            return;
        }
        match k.code {
            KeyCode::Tab | KeyCode::Enter if offering => {
                self.take_offer();
                return;
            }
            KeyCode::Up if offering => {
                if let Some(l) = self.library.as_mut() {
                    l.offer = l.offer.saturating_sub(1);
                }
                return;
            }
            KeyCode::Down if offering => {
                if let Some(l) = self.library.as_mut() {
                    l.offer += 1;
                }
                return;
            }
            KeyCode::Esc if offering => {
                if let Some(l) = self.library.as_mut() {
                    l.dismissed = Some(l.query.text().to_string());
                }
                return;
            }
            _ => {}
        }
        match k.code {
            KeyCode::Esc => return self.close_library(),
            KeyCode::Char('l') if ctrl => return self.library_key(),
            KeyCode::Char('g') if ctrl => return self.library_go(),
            KeyCode::Enter => return self.library_open(),
            KeyCode::Tab | KeyCode::BackTab => {
                let back = k.code == KeyCode::BackTab || k.modifiers.contains(KeyModifiers::SHIFT);
                let by: i32 = if back { -1 } else { 1 };
                if let Some(l) = self.library.as_mut() {
                    let at = KINDS.iter().position(|kind| *kind == l.kind).unwrap_or(0) as i32;
                    l.kind = KINDS[(at + by).rem_euclid(KINDS.len() as i32) as usize];
                    l.at = None;
                }
                return self.ask_library(false);
            }
            KeyCode::Left if !ctrl && !alt => return self.library_move(-1, 0),
            KeyCode::Right if !ctrl && !alt => return self.library_move(1, 0),
            KeyCode::Up => return self.library_move(0, -1),
            KeyCode::Down => return self.library_move(0, 1),
            KeyCode::PageUp => return self.library_move(0, -6),
            KeyCode::PageDown => return self.library_move(0, 6),
            _ => {}
        }
        let Some(l) = self.library.as_mut() else { return };
        let before = l.query.text().to_string();
        let q = &mut l.query;
        match k.code {
            KeyCode::Backspace if q.is_empty() => {
                if l.chips.pop().is_some() {
                    return self.ask_library(false);
                }
            }
            KeyCode::Char(c) if ctrl => match c {
                'a' => q.home(),
                'e' => q.end(),
                'u' => q.kill_line_back(),
                'w' => q.kill_to_space(),
                _ => {}
            },
            KeyCode::Char(c) if !alt => q.insert(c.encode_utf8(&mut [0; 4])),
            KeyCode::Backspace => q.backspace(),
            KeyCode::Delete => q.delete(),
            KeyCode::Home => q.home(),
            KeyCode::End => q.end(),
            _ => {}
        }
        if l.query.text() != before {
            l.offer = 0;
            let asked = l.words.clone();
            if self.library_words() != asked {
                self.ask_library(false);
            }
        }
    }

    /// ← and → while a photo is shown full size: the photo before or after it.
    fn view_step(&mut self, by: isize) {
        let Some(l) = self.library.as_mut() else { return };
        let photos: Vec<&LibraryRow> = l.rows.iter().filter(|r| r.kind == LibraryKind::Photo).collect();
        let at = photos.iter().position(|r| Some(&r.id) == l.at.as_ref()).unwrap_or(0) as isize;
        let next = (at + by).clamp(0, photos.len() as isize - 1) as usize;
        l.at = photos.get(next).map(|r| r.id.clone());
    }

    /// The footer's keys while the library is open.
    pub(super) fn library_hints(&self) -> Vec<(&'static str, &'static str, bool)> {
        let l = self.library.as_ref();
        if l.is_some_and(|l| l.viewing) {
            return vec![
                ("←→", "the one before, after", true),
                ("ctrl-g", "go to its message", true),
                ("any key", "back", true),
            ];
        }
        if !self.offers().is_empty() {
            return vec![("tab", "takes it", true), ("↑↓", "moves", true), ("esc", "puts them away", true)];
        }
        let widen = if l.is_some_and(|l| l.chat.is_some()) { "all chats" } else { "this chat, all chats" };
        let back = if l.is_some_and(|l| l.chat.is_some()) { "back to the conversation" } else { "back" };
        vec![
            ("⏎", "open", true),
            ("ctrl-g", "go to its message", true),
            ("tab", "kind", true),
            ("←↑↓→", "move", true),
            ("ctrl-l", widen, true),
            ("esc", back, true),
        ]
    }

    /// The library into `area`: its panel, the kind strip, the search line, then the items.
    pub(super) fn draw_library(&mut self, f: &mut Frame, area: Rect) {
        let t = self.theme;
        let capped = self.capped();
        let Some(l) = self.library.as_ref() else { return };
        let scope = l.chat.as_ref().map_or_else(|| "all chats".to_string(), |(_, n)| n.clone());
        let shared: u32 = l.counts.values().sum();
        let more = if capped && l.counts.values().any(|&n| n >= COUNTED) { "+" } else { "" };
        let right =
            Line::from(Span::styled(format!(" {}{more} shared ", fmt::num(shared as u64)), Style::new().fg(t.comment)))
                .right_aligned();
        let block = self
            .panel(t.focus)
            .title(self.title("library", vec![Span::styled(format!("· {scope} "), Style::new().fg(t.comment))]))
            .title(right)
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(Clear, area);
        f.render_widget(block, area);
        self.library_width = inner.width;
        if inner.height < 4 {
            return;
        }
        if self.library.as_ref().is_some_and(|l| l.viewing) {
            return self.draw_viewer(f, inner);
        }
        let w = inner.width as usize;
        let col = self.library.as_mut().map_or(0, |l| l.query.view(10_000).1.1);
        let buf = f.buffer_mut();
        let Some(l) = self.library.as_ref() else { return };
        // The kind strip.
        let mut strip = vec![];
        for kind in KINDS {
            let label = match kind {
                None => "all".to_string(),
                Some(k) => {
                    let n = l.counts.get(&k).copied().unwrap_or(0);
                    if n > 0 { format!("{} {}", k.plural(), counted(n, capped)) } else { k.plural().to_string() }
                }
            };
            let style = if kind == l.kind {
                Style::new().fg(t.fg).bg(t.selection).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(t.comment)
            };
            if !strip.is_empty() {
                strip.push(Span::raw(" "));
            }
            strip.push(Span::styled(format!(" {label} "), style));
        }
        buf.set_line(inner.x, inner.y, &Line::from(strip), inner.width);
        // The search line: chips, the words, and how many there are.
        let mut search = vec![Span::styled("› ", Style::new().fg(t.comment))];
        for chip in &l.chips {
            search.push(Span::styled(format!(" {} ", chip.label()), Style::new().fg(t.fg).bg(t.selection)));
            search.push(Span::raw(" "));
        }
        let lead: usize = search.iter().map(|s| s.width()).sum();
        let text = l.query.text().to_string();
        search.push(Span::styled(text.clone(), Style::new().fg(t.fg)));
        if text.is_empty() && l.chips.is_empty() {
            search.push(Span::styled(
                " captions, file names, link titles · from:  in:  since:  until:",
                Style::new().fg(t.gutter),
            ));
        }
        let said = match (&l.refused, l.answered) {
            (Some(_), _) => String::new(),
            (None, false) => "finding… ".to_string(),
            (None, true) if text.is_empty() && l.chips.is_empty() && l.kind.is_none() => {
                if l.chat.is_some() {
                    "newest first ".into()
                } else {
                    "all chats · newest first ".into()
                }
            }
            (None, true) => {
                let of = l.kind.map_or(shared, |k| l.counts.get(&k).copied().unwrap_or(0));
                format!("{} of {} ", counted(l.total, capped), counted(of.max(l.total), capped))
            }
        };
        let mut search = fit(search, w.saturating_sub(said.chars().count() + 1));
        let used: usize = search.iter().map(|s| s.width()).sum();
        search.push(Span::raw(" ".repeat(w.saturating_sub(used + said.chars().count()))));
        search.push(Span::styled(said, Style::new().fg(t.comment)));
        buf.set_line(inner.x, inner.y + 1, &Line::from(search), inner.width);
        let cursor = (inner.x + (lead + col).min(w.saturating_sub(1)) as u16, inner.y + 1);
        let content = Rect::new(inner.x, inner.y + 3, inner.width, inner.height.saturating_sub(3));
        let offers_at = Rect::new(inner.x + lead as u16, inner.y + 2, 36.min(inner.width), 8);
        if let Some(why) = l.refused.clone() {
            buf.set_line(
                content.x,
                content.y,
                &Line::from(Span::styled(why, Style::new().fg(t.comment))),
                content.width,
            );
            f.set_cursor_position(cursor);
            return;
        }
        if l.answered && l.rows.is_empty() {
            let said = match (&l.note, l.kind) {
                (Some(note), Some(LibraryKind::Link) | None) => note.as_str(),
                _ if text.is_empty() && l.chips.is_empty() => "Nothing shared here yet.",
                _ => "Nothing matches.",
            };
            buf.set_line(
                content.x,
                content.y,
                &Line::from(Span::styled(said, Style::new().fg(t.comment))),
                content.width,
            );
        } else {
            let laid = self.library_layout(content.width);
            self.draw_library_items(f, content, &laid);
        }
        self.draw_offers(f, offers_at);
        f.set_cursor_position(cursor);
    }

    fn draw_library_items(&mut self, f: &mut Frame, content: Rect, laid: &Laid) {
        let t = self.theme;
        let Some(l) = self.library.as_mut() else { return };
        let rows = content.height as usize;
        if let Some(sel) = laid.slots.iter().find(|s| l.rows.get(s.item).is_some_and(|r| Some(&r.id) == l.at.as_ref()))
        {
            let heading = laid.heads.iter().rev().find(|h| h.0 < sel.line && sel.line - h.0 <= 1).map(|h| h.0);
            let first = heading.unwrap_or(sel.line);
            if first < l.top {
                l.top = first;
            } else if sel.line + sel.rows as usize > l.top + rows {
                l.top = sel.line + sel.rows as usize - rows;
            }
        }
        let top = l.top;
        // jawd asks media only for what's on screen, so it's told what that is as it changes.
        let on_screen: Vec<String> = laid
            .slots
            .iter()
            .filter(|s| s.line < top + rows && s.line + s.rows as usize > top)
            .filter_map(|s| l.rows.get(s.item).map(|r| r.id.clone()))
            .take(jaw_proto::library::SHOWN)
            .collect();
        let newly_shown = (on_screen != l.shown).then(|| {
            l.shown.clone_from(&on_screen);
            on_screen
        });
        let scoped = l.chat.is_some();
        let selected = l.at.clone();
        let rows_data = l.rows.clone();
        let now = self.now();
        let w = content.width as usize;
        let narrow = content.width < 100;
        let y_of = |line: usize| -> Option<u16> {
            (line >= top && line < top + rows).then(|| content.y + (line - top) as u16)
        };
        for (line, label, count) in &laid.heads {
            let buf = f.buffer_mut();
            let Some(y) = y_of(*line) else { continue };
            let mut spans = vec![Span::styled(label.clone(), Style::new().fg(t.muted).add_modifier(Modifier::BOLD))];
            if !count.is_empty() {
                spans.push(Span::styled(format!(" · {count}"), Style::new().fg(t.gutter)));
            }
            let used: usize = spans.iter().map(|s| s.width()).sum::<usize>() + 1;
            spans.push(Span::raw(" "));
            spans.push(Span::styled("─".repeat(w.saturating_sub(used)), Style::new().fg(t.gutter)));
            buf.set_line(content.x, y, &Line::from(spans), content.width);
        }
        let who = |r: &LibraryRow| -> Vec<Span<'static>> {
            let name =
                if r.mine { "You".to_string() } else { r.from.as_ref().map_or_else(String::new, |f| f.name.clone()) };
            let colour = r.from.as_ref().map_or(t.text, |f| t.sender(f.colour));
            let mut out = vec![Span::styled(name, Style::new().fg(colour).add_modifier(Modifier::BOLD))];
            if !scoped {
                out.push(Span::styled(format!(" in {}", r.chat_name), Style::new().fg(t.comment)));
            }
            out
        };
        let mut tiles = vec![];
        for slot in &laid.slots {
            let r = &rows_data[slot.item];
            let sel = selected.as_deref() == Some(r.id.as_str());
            let band = Style::new().bg(t.selection).add_modifier(Modifier::BOLD);
            if slot.tile {
                tiles.push((slot.clone(), r.clone(), sel));
                if let Some(y) = y_of(slot.line + TILE.1 as usize) {
                    let mut caption: Vec<Span<'static>> = vec![];
                    if r.kind == LibraryKind::Video
                        && let Some(s) = r.media.as_ref().and_then(|m| m.seconds)
                    {
                        caption.push(Span::styled(format!("▶ {} ", fmt::length(s)), Style::new().fg(t.focus)));
                    }
                    let marked = r.caption.iter().any(|m| m.t == MarkKind::Mark);
                    if marked {
                        for m in &r.caption {
                            let style = if m.t == MarkKind::Mark {
                                Style::new().fg(t.mark).add_modifier(Modifier::BOLD)
                            } else {
                                Style::new().fg(t.comment)
                            };
                            caption.push(Span::styled(m.s.clone(), style));
                        }
                    } else {
                        let name = if r.mine {
                            "You".into()
                        } else {
                            r.from.as_ref().map_or_else(String::new, |f| f.name.clone())
                        };
                        caption.push(Span::styled(
                            format!("{name} · {}", short(r.at, now, self.tz)),
                            Style::new().fg(t.comment),
                        ));
                    }
                    let line = Line::from(fit(caption, TILE.0 as usize));
                    let buf = f.buffer_mut();
                    buf.set_line(content.x + slot.col, y, &line, TILE.0);
                    if sel {
                        buf.set_style(Rect::new(content.x + slot.col, y, TILE.0, 1), band);
                    }
                }
                continue;
            }
            let lines = self.library_row(r, w, narrow, sel, &who, now);
            for (k, line) in lines.into_iter().enumerate() {
                let Some(y) = y_of(slot.line + k) else { continue };
                let buf = f.buffer_mut();
                buf.set_line(content.x, y, &line, content.width);
                if sel {
                    buf.set_style(Rect::new(content.x.saturating_sub(1), y, content.width + 2, 1), band);
                }
            }
        }
        let look = crate::graphics::Look {
            bg: t.bg,
            truecolor: t.truecolor,
            over: self.palette.is_some() || self.overlay.is_some() || self.quick_look.is_some() || self.once.is_some(),
        };
        for (slot, r, _) in tiles {
            let top = content.y as i32 + slot.line as i32 - self.library.as_ref().map_or(0, |l| l.top) as i32;
            let place = crate::graphics::Place { x: content.x + slot.col, top, cols: TILE.0, rows: TILE.1 };
            let sources: Vec<std::path::PathBuf> = r
                .media
                .as_ref()
                .map(|m| {
                    [&m.previews.small, &m.previews.large, &m.thumbnail]
                        .into_iter()
                        .flatten()
                        .map(|p| self.images.path(p))
                        .collect()
                })
                .unwrap_or_default();
            let clip = Rect::new(content.x, content.y, content.width, content.height);
            let drawn = self.images.protocol != crate::graphics::Protocol::None
                && !self.no_color
                && self.images.draw(f.buffer_mut(), place, clip, &sources, look);
            if !drawn && top >= content.y as i32 && top + 1 < (content.y + content.height) as i32 {
                let mark = if r.kind == LibraryKind::Video { "▶" } else { "▣" };
                let line =
                    Line::from(Span::styled(format!("{mark:^w$}", w = TILE.0 as usize), Style::new().fg(t.gutter)));
                f.buffer_mut().set_line(place.x, (top + 1) as u16, &line, TILE.0);
            }
        }
        if let Some(items) = newly_shown {
            self.jawd.library_shown(&items);
        }
    }

    /// A file's, link's or voice message's lines in the library.
    fn library_row(
        &self,
        r: &LibraryRow,
        w: usize,
        narrow: bool,
        sel: bool,
        who: &dyn Fn(&LibraryRow) -> Vec<Span<'static>>,
        now: i64,
    ) -> Vec<Line<'static>> {
        let t = self.theme;
        let when = short(r.at, now, self.tz);
        let ends = |mut spans: Vec<Span<'static>>, right: &str| -> Line<'static> {
            let used: usize = spans.iter().map(|s| s.width()).sum();
            let room = w.saturating_sub(right.chars().count() + 1);
            if used > room {
                spans = fit(spans, room);
            }
            let used: usize = spans.iter().map(|s| s.width()).sum();
            spans.push(Span::raw(" ".repeat(w.saturating_sub(used + right.chars().count()))));
            spans.push(Span::styled(right.to_string(), Style::new().fg(t.comment)));
            Line::from(spans)
        };
        let name_style = Style::new().fg(t.fg).add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() });
        match r.kind {
            LibraryKind::File => {
                let f = r.file.clone().unwrap_or_default();
                let mut meta = vec![];
                meta.extend(f.kind.clone());
                if let Some(b) = f.bytes {
                    meta.push(bytes(b));
                }
                if let Some(p) = f.pages {
                    meta.push(format!("{p} {}", if p == 1 { "page" } else { "pages" }));
                }
                let meta = meta.join(" · ");
                if narrow {
                    let first = ends(
                        vec![Span::styled("▣ ", Style::new().fg(t.muted)), Span::styled(f.name, name_style)],
                        &when,
                    );
                    let mut second =
                        vec![Span::raw("  "), Span::styled(format!("{meta} · "), Style::new().fg(t.comment))];
                    second.extend(who(r));
                    return vec![first, Line::from(fit(second, w))];
                }
                let mut spans = vec![
                    Span::styled("▣ ", Style::new().fg(t.muted)),
                    Span::styled(pad(&f.name, 38), name_style),
                    Span::styled(pad(&meta, 26), Style::new().fg(t.comment)),
                ];
                spans.extend(who(r));
                vec![ends(spans, &when)]
            }
            LibraryKind::Link => {
                let link = r.link.clone().unwrap_or_default();
                let title = link.title.clone().unwrap_or_else(|| link.url.clone());
                let title_spans =
                    |room: usize| -> Vec<Span<'static>> { vec![Span::styled(fmt::fit(&title, room), name_style)] };
                let description = link.description.clone().filter(|_| sel);
                if narrow {
                    let mut first = vec![Span::styled("↗ ", Style::new().fg(t.link))];
                    first.extend(title_spans(w.saturating_sub(10)));
                    let mut second = vec![
                        Span::raw("  "),
                        Span::styled(link.host.clone(), Style::new().fg(t.link)),
                        Span::styled(" · ", Style::new().fg(t.comment)),
                    ];
                    second.extend(who(r));
                    let mut out = vec![ends(first, &when), Line::from(fit(second, w))];
                    if let Some(d) = description {
                        out.push(Line::from(Span::styled(
                            format!("  {}", fmt::fit(&d, w.saturating_sub(2))),
                            Style::new().fg(t.muted),
                        )));
                    }
                    return out;
                }
                let mut spans = vec![Span::styled("↗ ", Style::new().fg(t.link))];
                spans.extend(title_spans(41));
                let used: usize = spans.iter().map(|s| s.width()).sum();
                spans.push(Span::raw(" ".repeat(44usize.saturating_sub(used))));
                spans.push(Span::styled(pad(&link.host, 22), Style::new().fg(t.link)));
                spans.extend(who(r));
                let mut out = vec![ends(spans, &when)];
                if let Some(d) = description {
                    out.push(Line::from(Span::styled(
                        format!("  {}", fmt::fit(&d, w.saturating_sub(2))),
                        Style::new().fg(t.muted),
                    )));
                }
                out
            }
            LibraryKind::Voice => {
                let v = r.voice.clone().unwrap_or_default();
                let play = self.play_of(&r.message);
                let playing = play.is_some_and(|p| !p.paused);
                let cells = if narrow { 16 } else { 20 };
                let bars = crate::layout::waveform(&v.waveform, cells);
                let mut spans = vec![Span::styled(if playing { "⏸ " } else { "▶ " }, Style::new().fg(t.focus))];
                match play {
                    Some(p) => {
                        let played = ((p.at as f64 / 10.0 / v.seconds.max(0.1)) * cells as f64).round() as usize;
                        let split = bars.char_indices().nth(played.min(cells)).map_or(bars.len(), |(i, _)| i);
                        spans.push(Span::styled(bars[..split].to_string(), Style::new().fg(t.progress)));
                        spans.push(Span::styled(bars[split..].to_string(), Style::new().fg(t.comment)));
                        spans.push(Span::styled(
                            pad(&format!("  {} / {}", fmt::length(p.at as f64 / 10.0), fmt::length(v.seconds)), 16),
                            Style::new().fg(t.muted),
                        ));
                    }
                    None => {
                        spans.push(Span::styled(bars, Style::new().fg(t.muted)));
                        spans.push(Span::styled(
                            pad(&format!("  {}", fmt::length(v.seconds)), 16),
                            Style::new().fg(t.muted),
                        ));
                    }
                }
                spans.extend(who(r));
                vec![ends(spans, &when)]
            }
            LibraryKind::Photo | LibraryKind::Video => vec![],
        }
    }

    /// A photo full size in the library's area, its caption under it.
    fn draw_viewer(&mut self, f: &mut Frame, area: Rect) {
        let t = self.theme;
        let Some(r) = self.selected_item() else { return };
        let Some(m) = r.media.clone() else { return };
        let (cols, rows) = crate::graphics::fit(m.w, m.h, area.width, area.height.saturating_sub(2), self.images.cell);
        let x = area.x + (area.width - cols) / 2;
        let place = crate::graphics::Place { x, top: area.y as i32, cols, rows };
        let sources: Vec<std::path::PathBuf> = [&m.previews.large, &m.previews.small, &m.thumbnail]
            .into_iter()
            .flatten()
            .map(|p| self.images.path(p))
            .collect();
        let look = crate::graphics::Look { bg: t.bg, truecolor: t.truecolor, over: false };
        if self.images.protocol == crate::graphics::Protocol::None
            || self.no_color
            || !self.images.draw(f.buffer_mut(), place, area, &sources, look)
        {
            let line =
                Line::from(Span::styled("▣ the photo comes once its preview is here", Style::new().fg(t.comment)))
                    .centered();
            f.buffer_mut().set_line(area.x, area.y + rows / 2, &line, area.width);
        }
        let name =
            if r.mine { "You".to_string() } else { r.from.as_ref().map_or_else(String::new, |f| f.name.clone()) };
        let caption: String = r.caption.iter().map(|m| m.s.as_str()).collect();
        let mut said = format!("{name} in {} · {}", r.chat_name, fmt::full(r.at, self.tz));
        if !caption.is_empty() {
            said = format!("{caption} · {said}");
        }
        let line = Line::from(Span::styled(fmt::fit(&said, area.width as usize), Style::new().fg(t.muted))).centered();
        f.buffer_mut().set_line(area.x, area.y + rows + 1, &line, area.width);
    }

    /// What a filter being typed could become, in a dashed box under the search line.
    fn draw_offers(&mut self, f: &mut Frame, at: Rect) {
        let offers = self.offers();
        if offers.is_empty() {
            return;
        }
        let t = self.theme;
        let h = offers.len() as u16 + 2;
        let area = Rect::new(at.x, at.y, at.width, h.min(at.height + 10));
        f.render_widget(Clear, area);
        let block = Block::bordered()
            .border_type(BorderType::Plain)
            .border_set(ratatui::symbols::border::Set {
                top_left: "┌",
                top_right: "┐",
                bottom_left: "└",
                bottom_right: "┘",
                vertical_left: "┆",
                vertical_right: "┆",
                horizontal_top: "╌",
                horizontal_bottom: "╌",
            })
            .border_style(Style::new().fg(t.gutter))
            .style(Style::new().bg(t.bg))
            .title_bottom(Line::from(Span::styled(" tab takes it ", Style::new().fg(t.comment))).right_aligned())
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let selected = self.library.as_ref().map_or(0, |l| l.offer).min(offers.len() - 1);
        let buf = f.buffer_mut();
        for (i, o) in offers.iter().enumerate() {
            let y = inner.y + i as u16;
            buf.set_line(
                inner.x,
                y,
                &Line::from(Span::styled(fmt::fit(&o.said, inner.width as usize), Style::new().fg(t.fg))),
                inner.width,
            );
            if i == selected {
                buf.set_style(Rect::new(area.x + 1, y, area.width - 2, 1), Style::new().bg(t.selection));
            }
        }
    }
}

/// A size as files show it: 310 KB, 2.4 MB.
fn bytes(n: u64) -> String {
    match n {
        n if n >= 1_000_000_000 => format!("{:.1} GB", n as f64 / 1e9),
        n if n >= 1_000_000 => format!("{:.1} MB", n as f64 / 1e6),
        n if n >= 1_000 => format!("{} KB", n / 1_000),
        n => format!("{n} B"),
    }
}

/// Text cut or padded to `w` columns.
fn pad(s: &str, w: usize) -> String {
    let cut = fmt::fit(s, w.saturating_sub(1));
    let used = fmt::width(&cut);
    format!("{cut}{}", " ".repeat(w.saturating_sub(used)))
}

/// Spans cut to `w` columns.
fn fit(spans: Vec<Span<'static>>, w: usize) -> Vec<Span<'static>> {
    let mut out = vec![];
    let mut used = 0;
    for s in spans {
        let sw = s.width();
        if used + sw <= w {
            used += sw;
            out.push(s);
            continue;
        }
        let room = w.saturating_sub(used);
        if room > 0 {
            out.push(Span::styled(fmt::fit(&s.content, room), s.style));
        }
        break;
    }
    out
}
