//! The terminal interface's state: what jawd has sent, which chat is open, where the conversation
//! is anchored, the message box, and what's selected. Updates apply here, keys drive it
//! (`keys`), and it draws itself (`draw`).

mod draw;
mod keys;
mod palette;

use crate::composer::Composer;
use crate::data::{Around, Jawd, Message, Typing, Update};
use crate::fmt::{self, MINUTE};
use crate::layout::{self, Shape};
use crate::theme::Theme;
use crate::window::{Chats, Messages};
use chrono::FixedOffset;
use jaw_proto::{Connection, ConnectionState, LinkStep};
use ratatui::layout::Rect;
use ratatui::text::Line;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// Below this width the chat list folds, and takes the screen until a chat is picked.
pub const NARROW: u16 = 90;
/// The chat list's width beside the conversation.
pub const LIST: u16 = 34;
/// How long a notice stays in the footer.
const NOTICE: i64 = 4_000;
/// The chat list keeps its order this long after the last move through it.
const HOLD: i64 = 1_500;
/// A draft goes to jawd once typing has paused this long.
const DRAFT: i64 = 1_500;
/// Typing is said again this often while it goes on.
const TYPING: i64 = 5_000;
/// Read receipts go at most this often while messages keep arriving.
const READ: i64 = 1_000;
/// The Mimic chomps for this long when an empty chat's first message comes.
const CHOMP: i64 = 1_200;
/// Messages kept each side of the anchor in the open chat's window.
pub const WINDOW: usize = 100;
/// Ask for the window to move once the screen is this close to its edge.
const EDGE: usize = 20;

/// Whether jawd answers.
#[derive(Clone, Debug, PartialEq)]
pub enum Reach {
    /// No hello yet.
    Waiting,
    Starting {
        failed: Option<String>,
    },
    Unreachable {
        why: String,
        retry_at: i64,
        since: i64,
    },
    Up,
}

/// Where the keys go: the message box, or a selected message.
#[derive(Clone, Debug, PartialEq)]
pub enum Focus {
    Typing,
    Message(String),
}

/// Where the conversation is: following the newest at the bottom, or anchored on a message at a
/// row of the screen, so what arrives above or below doesn't move it.
#[derive(Clone, Debug, PartialEq)]
pub enum Scroll {
    Bottom,
    Anchor { id: String, sort: String, top: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    Keys,
}

/// What the loop does for the view outside the terminal's frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Puts text on the clipboard, through the terminal.
    Copy(String),
}

/// A message as last drawn: its block's first row on screen and how tall it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub top: i32,
    pub height: i32,
}

struct Laid {
    v: u64,
    shape: Shape,
    lines: Rc<Vec<Line<'static>>>,
}

/// A chat's rows in the list as last drawn, and what they depended on: its version, whether it
/// was open, the width, and the minute its time was written in.
struct ChatRows {
    key: (u64, bool, usize, i64),
    rows: Rc<(Line<'static>, Line<'static>)>,
}

/// Where things were last drawn, for the mouse and for keys that depend on the layout.
#[derive(Debug, Default)]
struct Areas {
    list: Rect,
    /// Each chat row's first line and its chat.
    chat_rows: Vec<(u16, String)>,
    messages: Rect,
    /// Each message's rows on screen.
    message_rows: Vec<(u16, u16, String)>,
    composer: Rect,
    composer_width: usize,
    width: u16,
    height: u16,
}

pub struct View {
    pub theme: Theme,
    pub no_color: bool,
    /// A clock that stands still, for frames that must come out the same.
    pub clock: Option<i64>,
    pub tz: FixedOffset,
    jawd: Box<dyn Jawd>,

    reach: Reach,
    linked: Option<bool>,
    me: Option<String>,
    connection: Connection,
    link: Option<LinkStep>,
    link_asked: bool,
    /// The number being typed for a phone code.
    phone: Option<String>,

    chats: Chats,
    messages: Option<Messages>,
    /// The open chat, kept by ID however the list moves.
    open: Option<String>,
    /// The chat the list has selected, which is the open one except while picking on a narrow
    /// screen.
    selected: Option<String>,
    list_top: usize,
    list_rows: usize,
    /// Below 90 columns: whether the chat list has the screen.
    picking: bool,

    focus: Focus,
    scroll: Scroll,
    /// The newest message seen on screen in the open chat, by `sort`.
    seen: Option<String>,
    /// Unread when the chat was opened, to anchor its window on the first of them.
    unread_at_open: u32,
    placed: HashMap<String, Placed>,
    laid: HashMap<String, Laid>,
    /// Each chat's rows in the list, kept per version, whether it's open, width and minute.
    rows_laid: HashMap<String, ChatRows>,
    unfolded: HashSet<String>,
    asked_around: Option<String>,

    composer: Composer,
    draft_sent: String,
    draft_due: Option<i64>,
    typing_at: i64,
    read_upto: Option<String>,
    read_at: i64,
    read_due: Option<i64>,

    overlay: Option<Overlay>,
    /// Ctrl-K's palette, while it's open.
    palette: Option<palette::Palette>,
    /// The newest request number the interface has given a `find`.
    asked: u64,
    /// The open chat's row when it isn't in the chat list, as an archived one found by the palette.
    outside: Option<crate::data::Chat>,
    /// The person a chat is being started with, to open it when it comes.
    starting: Option<String>,
    /// A message to anchor on and select once it's in the window.
    jump: Option<String>,
    notice: Option<(String, i64)>,
    hold_until: Option<i64>,
    chomp_until: Option<i64>,
    last_click: Option<(i64, String)>,
    /// The creature, decoded once.
    mimic: crate::art::Sprite,
    areas: Areas,
    /// The size snapshots draw at.
    size: Option<(u16, u16)>,
    /// The second the last frame was drawn in, so clocks and countdowns redraw as it passes.
    drawn_at: i64,
    effects: Vec<Effect>,
    /// Something on screen changed since the last frame.
    pub dirty: bool,
    pub quit: bool,
}

impl View {
    pub fn new(jawd: Box<dyn Jawd>) -> View {
        let truecolor = crate::theme::truecolor();
        View {
            theme: Theme::with(truecolor),
            no_color: crate::theme::no_color(),
            clock: None,
            tz: *chrono::Local::now().offset(),
            jawd,
            reach: Reach::Waiting,
            linked: None,
            me: None,
            connection: Connection::new(ConnectionState::Connecting),
            link: None,
            link_asked: false,
            phone: None,
            chats: Chats::default(),
            messages: None,
            open: None,
            selected: None,
            list_top: 0,
            list_rows: 0,
            picking: true,
            focus: Focus::Typing,
            scroll: Scroll::Bottom,
            seen: None,
            unread_at_open: 0,
            placed: HashMap::new(),
            laid: HashMap::new(),
            rows_laid: HashMap::new(),
            unfolded: HashSet::new(),
            asked_around: None,
            composer: Composer::default(),
            draft_sent: String::new(),
            draft_due: None,
            typing_at: 0,
            read_upto: None,
            read_at: 0,
            read_due: None,
            overlay: None,
            palette: None,
            asked: 0,
            outside: None,
            starting: None,
            jump: None,
            notice: None,
            hold_until: None,
            chomp_until: None,
            last_click: None,
            mimic: crate::art::Sprite::mimic(),
            areas: Areas::default(),
            size: None,
            drawn_at: 0,
            effects: vec![],
            dirty: true,
            quit: false,
        }
    }

    pub fn now(&self) -> i64 {
        self.clock.unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
    }

    fn say(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), self.now() + NOTICE));
        self.dirty = true;
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.size = Some((width, height));
    }

    pub fn size(&self) -> Option<(u16, u16)> {
        self.size
    }

    /// Whether the open chat's messages are on screen, which is when the first frame is done.
    pub fn showing_messages(&self) -> bool {
        self.messages.as_ref().is_some_and(|w| w.loaded) || (self.reach == Reach::Up && self.chats.is_empty())
    }

    /// Whether something on screen counts seconds: a QR code's or a retry's.
    fn counting(&self) -> bool {
        matches!(self.reach, Reach::Unreachable { .. })
            || (self.linking() && matches!(self.link, Some(LinkStep::Qr { .. })))
    }

    fn narrow(&self) -> bool {
        self.areas.width > 0 && self.areas.width < NARROW
    }

    pub fn apply(&mut self, update: Update) {
        self.dirty = true;
        match update {
            Update::Hello { linked, me, connection } => {
                self.reach = Reach::Up;
                self.linked = Some(linked);
                self.me = me;
                self.connection = connection;
                if linked {
                    self.jawd.watch_chats(self.selected.as_deref(), 20, 100);
                    if let Some(chat) = self.open.clone() {
                        self.watch(&chat, Around::Newest);
                    }
                } else {
                    self.start_linking();
                }
            }
            Update::Connection(c) => {
                if c.state == ConnectionState::LoggedOut {
                    self.link = None;
                    self.link_asked = false;
                }
                self.connection = c;
            }
            Update::Link(step) => {
                if let LinkStep::Done { phone } = &step {
                    self.say(format!("Linked {phone}. jawd is fetching your history."));
                    self.linked = Some(true);
                    self.connection = Connection::new(ConnectionState::CatchingUp);
                    self.jawd.watch_chats(None, 20, 100);
                }
                self.link = Some(step);
            }
            Update::Chats { reset, upsert, remove, total } => {
                let left = reset || !remove.is_empty();
                if let Some(open) = self.open.as_ref().filter(|o| remove.contains(o)) {
                    self.outside = self.chats.get(open).cloned().or(self.outside.take());
                }
                if let Some(back) = upsert.iter().find(|c| self.outside.as_ref().is_some_and(|o| o.id == c.id)) {
                    self.outside = (back.archived).then(|| back.clone());
                }
                self.keep_selected_row(|v| v.chats.apply(reset, upsert, &remove, total));
                if left {
                    let chats = &self.chats;
                    self.rows_laid.retain(|id, _| chats.get(id).is_some());
                }
                if self.open.is_none() {
                    let first = self.chats.order().first().map(|s| s.to_string());
                    if let Some(first) = first {
                        self.open_chat(&first);
                        self.picking = true;
                    }
                }
            }
            Update::Messages { chat, reset, upsert, remove, older, newer } => {
                let Some(window) = self.messages.as_mut().filter(|w| w.chat == chat) else { return };
                let first = !window.loaded && reset;
                let was_empty = window.loaded && window.is_empty();
                let gone = window.apply(reset, upsert, &remove, older, newer);
                let now_empty = window.is_empty();
                for id in gone {
                    self.laid.remove(&id);
                    self.placed.remove(&id);
                }
                if was_empty && !now_empty {
                    self.chomp_until = Some(self.now() + CHOMP);
                }
                if first {
                    self.anchor_on_unread();
                }
                self.jump_if_here();
                if let Focus::Message(id) = &self.focus
                    && self.messages.as_ref().is_some_and(|w| w.get(id).is_none())
                {
                    self.focus = Focus::Typing;
                }
            }
            Update::Found { id, results, counts } => self.found(id, results, counts),
            Update::FindFailed { id, cancelled, why } => self.find_failed(id, cancelled, why),
            Update::ChatStarted { chat } => {
                if self.starting.take().is_some() {
                    let id = chat.id.clone();
                    if self.chats.get(&id).is_none() {
                        self.outside = Some(chat);
                    }
                    self.open_chat(&id);
                    self.jawd.opened(&id);
                }
            }
            Update::Notice(text) => self.say(text),
            Update::Starting { failed } => self.reach = Reach::Starting { failed },
            Update::Unreachable { why, retry_at } => {
                let since = match &self.reach {
                    Reach::Unreachable { since, .. } => *since,
                    _ => self.now(),
                };
                self.reach = Reach::Unreachable { why, retry_at, since };
            }
        }
    }

    /// Runs `change` on the chat list keeping the selected chat on the row of the screen it was
    /// on, so it holds its place while others move.
    fn keep_selected_row(&mut self, change: impl FnOnce(&mut View)) {
        let row = self.selected_index().map(|i| i as isize - self.list_top as isize);
        change(self);
        if let (Some(row), Some(i)) = (row, self.selected_index())
            && (0..self.list_rows.max(1) as isize).contains(&row)
        {
            self.list_top = (i as isize - row).max(0) as usize;
        }
        self.clamp_list();
    }

    fn selected_index(&self) -> Option<usize> {
        self.chats.position(self.selected.as_deref()?)
    }

    fn clamp_list(&mut self) {
        let most = self.chats.len().saturating_sub(self.list_rows.max(1));
        self.list_top = self.list_top.min(most);
    }

    fn reveal_selected_chat(&mut self) {
        if let Some(i) = self.selected_index() {
            let rows = self.list_rows.max(1);
            if i < self.list_top {
                self.list_top = i;
            } else if i >= self.list_top + rows {
                self.list_top = i + 1 - rows;
            }
        }
    }

    fn start_linking(&mut self) {
        if !self.link_asked {
            self.link_asked = true;
            self.jawd.link_start(None);
        }
    }

    /// Opens a chat: its draft into the box, its window from the newest, typing in the box.
    pub fn open_chat(&mut self, id: &str) {
        self.open_chat_at(id, None);
    }

    /// Opens a chat at a message, or at its newest.
    pub fn open_chat_at(&mut self, id: &str, at: Option<String>) {
        if self.open.as_deref() == Some(id) {
            self.selected = Some(id.to_string());
            self.picking = false;
            return;
        }
        self.leave_chat();
        let draft = self.chats.get(id).and_then(|c| c.draft.clone()).unwrap_or_default();
        self.unread_at_open = self.chats.get(id).map_or(0, |c| c.unread);
        self.composer.set(&draft);
        self.draft_sent = draft;
        self.open = Some(id.to_string());
        self.selected = Some(id.to_string());
        self.messages = Some(Messages::new(id));
        self.focus = Focus::Typing;
        self.scroll = Scroll::Bottom;
        self.seen = None;
        self.placed.clear();
        self.laid.clear();
        self.unfolded.clear();
        self.read_upto = None;
        self.chomp_until = None;
        self.picking = false;
        if self.outside.as_ref().is_some_and(|o| o.id != id) {
            self.outside = None;
        }
        self.watch(id, at.map_or(Around::Newest, Around::Message));
        self.reveal_selected_chat();
        self.dirty = true;
    }

    /// The open chat's draft and typing, settled before another opens.
    fn leave_chat(&mut self) {
        let Some(chat) = self.open.clone() else { return };
        self.save_draft(&chat);
        if self.typing_at > 0 {
            self.jawd.typing(&chat, Typing::Paused);
            self.typing_at = 0;
        }
        if let Some(due) = self.read_due.take()
            && due > 0
        {
            self.send_read();
        }
    }

    fn save_draft(&mut self, chat: &str) {
        self.draft_due = None;
        if self.composer.text() != self.draft_sent {
            self.draft_sent = self.composer.text().to_string();
            self.jawd.draft(chat, &self.draft_sent);
        }
    }

    fn watch(&mut self, chat: &str, around: Around) {
        let (before, after) = match around {
            Around::Newest => (WINDOW, 0),
            Around::Message(_) => (WINDOW, WINDOW),
        };
        if let Some(w) = self.messages.as_mut() {
            w.following = around == Around::Newest;
        }
        self.asked_around = match &around {
            Around::Message(id) => Some(id.clone()),
            Around::Newest => None,
        };
        self.jawd.watch_messages(chat, around, before, after);
    }

    /// The message a jump is for, once it's in the window: anchored a third of the way down,
    /// and selected.
    fn jump_if_here(&mut self) {
        let Some(id) = self.jump.clone() else { return };
        let Some(m) = self.messages.as_ref().and_then(|w| w.get(&id)) else { return };
        let top = self.areas_messages_height() / 3;
        self.scroll = Scroll::Anchor { id: id.clone(), sort: m.sort.clone(), top };
        self.focus = Focus::Message(id);
        self.jump = None;
    }

    fn areas_messages_height(&self) -> i32 {
        self.areas.messages.height.max(12) as i32
    }

    /// A chat opened with unread messages starts at the first of them, the rest counted below.
    fn anchor_on_unread(&mut self) {
        let n = self.unread_at_open as usize;
        let Some(w) = self.messages.as_ref() else { return };
        if n == 0 {
            return;
        }
        if let Some(first) = w.nth_newest_theirs(n) {
            self.seen = w.before(&first.sort).map(|m| m.sort.clone());
            self.scroll = Scroll::Anchor { id: first.id.clone(), sort: first.sort.clone(), top: 1 };
        }
    }

    /// The box's text changed: typing is said, and the draft saved once it pauses.
    fn typed(&mut self) {
        let Some(chat) = self.open.clone() else { return };
        let now = self.now();
        if self.composer.is_empty() {
            if self.typing_at > 0 {
                self.jawd.typing(&chat, Typing::Paused);
                self.typing_at = 0;
            }
        } else if now - self.typing_at >= TYPING {
            self.jawd.typing(&chat, Typing::Composing);
            self.typing_at = now;
        }
        self.draft_due = Some(now + DRAFT);
    }

    fn send(&mut self) {
        let Some(chat) = self.open.clone() else { return };
        let text = self.composer.text().trim_end().to_string();
        if text.trim().is_empty() {
            return;
        }
        if let Some(words) = text.strip_prefix("/search").filter(|w| w.is_empty() || w.starts_with(' ')) {
            self.composer.take();
            let name = self.chat_known(&chat).map_or_else(|| chat.clone(), |c| c.name.clone());
            self.palette_search(Some((chat, name)), words.trim());
            return;
        }
        if text == "/poll" || text.starts_with("/poll ") {
            self.say("polls come with M2");
            return;
        }
        self.composer.take();
        self.jawd.send_text(&chat, &text);
        self.typing_at = 0;
        self.draft_sent.clear();
        self.draft_due = None;
        if !self.messages.as_ref().is_some_and(|w| w.following) {
            self.watch(&chat, Around::Newest);
        }
        self.scroll = Scroll::Bottom;
    }

    fn send_read(&mut self) {
        let (Some(chat), Some(upto)) = (self.open.clone(), self.read_upto.clone()) else { return };
        self.jawd.read(&chat, &upto);
        self.read_at = self.now();
        self.read_due = None;
    }

    /// The newest message is on screen: what's unread in the chat has been seen, said at most
    /// once a second while messages keep coming.
    fn seen_newest(&mut self, newest: &Message) {
        let unread = self.open.as_deref().and_then(|id| self.chats.get(id)).is_some_and(|c| c.unread > 0);
        if !unread || self.read_upto.as_deref() == Some(newest.id.as_str()) && self.read_due.is_none() {
            return;
        }
        self.read_upto = Some(newest.id.clone());
        if self.now() - self.read_at >= READ {
            self.send_read();
        } else {
            self.read_due = Some(self.read_at + READ);
        }
    }

    /// Whatever has come due: the chat list's order let go, a draft saved, a notice gone.
    pub fn tick(&mut self) {
        let now = self.now();
        let (second, minute) = (now / 1_000, now / MINUTE);
        if minute != self.drawn_at / 60 || (self.counting() && second != self.drawn_at) {
            self.dirty = true;
        }
        if self.hold_until.is_some_and(|t| t <= now) {
            self.hold_until = None;
            self.keep_selected_row(|v| {
                v.chats.release();
            });
            self.dirty = true;
        }
        if self.draft_due.is_some_and(|t| t <= now)
            && let Some(chat) = self.open.clone()
        {
            self.save_draft(&chat);
        }
        if self.read_due.is_some_and(|t| t <= now) {
            self.send_read();
        }
        if self.notice.as_ref().is_some_and(|(_, until)| *until <= now) {
            self.notice = None;
            self.dirty = true;
        }
        if self.chomp_until.is_some_and(|t| t <= now) {
            self.chomp_until = None;
            self.dirty = true;
        }
    }

    /// When the view next needs to wake with nothing arriving: the clock's next minute, and
    /// anything sooner that's due or moving on screen.
    pub fn next_wake(&self) -> i64 {
        let now = self.now();
        let mut wake = (now / MINUTE + 1) * MINUTE;
        let mut soon = |t: Option<i64>| {
            if let Some(t) = t {
                wake = wake.min(t);
            }
        };
        soon(self.hold_until);
        soon(self.draft_due);
        soon(self.read_due);
        soon(self.notice.as_ref().map(|(_, t)| *t));
        if self.chomp_until.is_some() {
            soon(Some(now + 100));
        }
        if self.counting() {
            soon(Some((now / 1_000 + 1) * 1_000));
        }
        wake
    }

    fn linking(&self) -> bool {
        self.reach == Reach::Up && (self.linked == Some(false) || self.connection.state == ConnectionState::LoggedOut)
    }

    /// What a message's lines depend on, from the one before it.
    fn shape(&self, m: &Message, prev: Option<&Message>, width: usize) -> Shape {
        fn who(m: &Message) -> Option<&str> {
            if m.mine { None } else { m.from.as_ref().map(|f| f.id.as_str()) }
        }
        let system = |m: &Message| matches!(m.kind, crate::data::Kind::System | crate::data::Kind::Call);
        let new_day = prev.is_none_or(|p| !fmt::same_day(p.at, m.at, self.tz));
        let changed = prev.is_none_or(|p| who(p) != who(m) || p.mine != m.mine || system(p)) || new_day;
        Shape {
            width,
            name: changed || self.gap(m, prev),
            time: changed || prev.is_some_and(|p| !fmt::same_minute(p.at, m.at, self.tz)),
            unfolded: self.unfolded.contains(&m.id),
        }
    }

    /// A blank line before a message: a new sender in a group, or half an hour since the last.
    fn gap(&self, m: &Message, prev: Option<&Message>) -> bool {
        let Some(p) = prev else { return false };
        if !fmt::same_day(p.at, m.at, self.tz) {
            return false;
        }
        let group = self.open.as_deref().and_then(|id| self.chats.get(id)).is_some_and(|c| c.group());
        let sender_changed = p.mine != m.mine || p.from.as_ref().map(|f| &f.id) != m.from.as_ref().map(|f| &f.id);
        m.at - p.at >= 30 * MINUTE || (group && sender_changed)
    }

    /// A message's lines, laid out once per version and shape.
    fn lines(&mut self, m: &Message, shape: Shape) -> Rc<Vec<Line<'static>>> {
        if let Some(l) = self.laid.get(&m.id)
            && l.v == m.v
            && l.shape == shape
        {
            return l.lines.clone();
        }
        let ctx = layout::Ctx { theme: &self.theme, tz: self.tz, me: self.me.as_deref() };
        let lines = Rc::new(layout::message(m, shape, &ctx));
        self.laid.insert(m.id.clone(), Laid { v: m.v, shape, lines: lines.clone() });
        lines
    }
}

#[cfg(test)]
pub mod tests;
