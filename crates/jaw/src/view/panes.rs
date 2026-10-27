//! Split panes, as tmux has them, for several chats at once: the conversation splits across or
//! down into panes, each with one chat in a window of its own, six at most. Ctrl-P is the prefix,
//! as Ctrl-B is tmux's: then % or | splits across and " or - down, a digit goes straight to a
//! pane, o to the next and an arrow to the one that way, x closes, { and } swap, and Alt or Ctrl
//! with an arrow moves the divider, again and again until another key. The palette and Alt-Up and
//! Alt-Down put the chat chosen in the pane with the keys, or go to the pane that shows it.
//!
//! The pane with the keys has the open chat, as ever, so all that works on the open chat works
//! there. The others keep their chat's state parked (its window, where it's scrolled to, its box
//! with what's typed in it) and are drawn by taking that out for a moment; only the pane with the
//! keys reads what it shows. Hopping leaves a chat as switching chats does, its draft saved. jawd
//! keeps the layout for jaw, so jaw opens with the panes it was left with (`kept`).

mod kept;

#[cfg(test)]
pub use self::kept::KEEP_AFTER;
pub use self::kept::LayoutKept;
use super::actions::{Editing, Replying, Taken};
use super::{Focus, Laid, Placed, Scroll, View};
use crate::composer::Composer;
use crate::data::{Chat, ChatKind, Message, Place};
use jaw_client::{Messages, Update as Window};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Padding, Paragraph};
use std::collections::{HashMap, HashSet};

/// The most panes, the least room a pane needs, and how far a divider moves at a time, in
/// percent, between its least and most.
pub const MOST: usize = 6;
const LEAST: (u16, u16) = (24, 8);
const STEP: i16 = 5;
const SHARE: (i16, i16) = (20, 80);
/// The longest a chat's name runs in the status line's list of panes.
const TAB: usize = 16;

/// Which way a split runs: its two sides beside each other, or one above the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split {
    Across,
    Down,
}

/// The layout: a pane, by its place among the panes, or a split of two layouts with the first
/// side's share in percent.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Pane(usize),
    Split { how: Split, first: u16, a: Box<Node>, b: Box<Node> },
}

/// What the prefix has armed: a command, or more of moving a divider.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Prefix {
    #[default]
    Off,
    Armed,
    Resizing,
}

/// What the palette's pane actions do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneDo {
    Split(Split),
    Next,
    Close,
}

/// A pane's chat and everything the open chat keeps of it, while another pane has the keys.
pub struct Parked {
    open: Option<String>,
    messages: Option<Messages<Message>>,
    focus: Focus,
    scroll: Scroll,
    seen: Option<String>,
    unread_at_open: u32,
    placed: HashMap<String, Placed>,
    laid: HashMap<String, Laid>,
    unfolded: HashSet<String>,
    asked_place: Option<Place>,
    composer: Composer,
    draft_sent: String,
    draft_due: Option<i64>,
    typing_at: i64,
    read_upto: Option<String>,
    read_at: i64,
    read_due: Option<i64>,
    replying: Option<Replying>,
    editing: Option<Editing>,
    taken: Vec<Taken>,
    dismissed: Option<usize>,
    naming_selected: usize,
    unsent: Option<String>,
    outside: Option<Chat>,
    jump: Option<String>,
    chomp_until: Option<i64>,
}

impl Default for Parked {
    fn default() -> Parked {
        Parked {
            open: None,
            messages: None,
            focus: Focus::Typing,
            scroll: Scroll::Bottom,
            seen: None,
            unread_at_open: 0,
            placed: HashMap::new(),
            laid: HashMap::new(),
            unfolded: HashSet::new(),
            asked_place: None,
            composer: Composer::default(),
            draft_sent: String::new(),
            draft_due: None,
            typing_at: 0,
            read_upto: None,
            read_at: 0,
            read_due: None,
            replying: None,
            editing: None,
            taken: vec![],
            dismissed: None,
            naming_selected: 0,
            unsent: None,
            outside: None,
            jump: None,
            chomp_until: None,
        }
    }
}

#[derive(Default)]
pub struct Panes {
    /// Each pane's parked state, by its place here; the pane with the keys holds an empty one.
    parked: Vec<Parked>,
    /// The layout once the conversation is split; none while it's one pane, as without panes.
    tree: Option<Node>,
    /// The pane with the keys.
    focused: usize,
    pub prefix: Prefix,
    /// A parked pane is being drawn, so drawing it neither reads it nor moves its window.
    pub drawing: bool,
    /// Each pane where it was last drawn, in the layout's order.
    drawn: Vec<(usize, Rect)>,
}

/// The panes under `node` in the layout's order, each with its place in `area`.
fn lay(node: &Node, area: Rect, out: &mut Vec<(usize, Rect)>) {
    match node {
        Node::Pane(p) => out.push((*p, area)),
        Node::Split { how, first, a, b } => {
            let (one, two) = match how {
                Split::Across => {
                    let w = (u32::from(area.width) * u32::from(*first) / 100) as u16;
                    (Rect { width: w, ..area }, Rect { x: area.x + w, width: area.width - w, ..area })
                }
                Split::Down => {
                    let h = (u32::from(area.height) * u32::from(*first) / 100) as u16;
                    (Rect { height: h, ..area }, Rect { y: area.y + h, height: area.height - h, ..area })
                }
            };
            lay(a, one, out);
            lay(b, two, out);
        }
    }
}

/// The layout with pane `at` split `how`, pane `new` its second side.
fn split_at(node: &mut Node, at: usize, how: Split, new: usize) -> bool {
    match node {
        Node::Pane(p) if *p == at => {
            *node = Node::Split { how, first: 50, a: Box::new(Node::Pane(at)), b: Box::new(Node::Pane(new)) };
            true
        }
        Node::Pane(_) => false,
        Node::Split { a, b, .. } => split_at(a, at, how, new) || split_at(b, at, how, new),
    }
}

/// The layout without pane `at`, the other side of its split taking the split's place, and the
/// panes after it one place earlier, as they are once it's gone.
fn without(node: Node, at: usize) -> Option<Node> {
    match node {
        Node::Pane(p) if p == at => None,
        Node::Pane(p) => Some(Node::Pane(if p > at { p - 1 } else { p })),
        Node::Split { how, first, a, b } => match (without(*a, at), without(*b, at)) {
            (Some(a), Some(b)) => Some(Node::Split { how, first, a: Box::new(a), b: Box::new(b) }),
            (one, None) | (None, one) => one,
        },
    }
}

/// The layout with panes `x` and `y` in each other's places.
fn swapped(node: &mut Node, x: usize, y: usize) {
    match node {
        Node::Pane(p) if *p == x => *p = y,
        Node::Pane(p) if *p == y => *p = x,
        Node::Pane(_) => {}
        Node::Split { a, b, .. } => {
            swapped(a, x, y);
            swapped(b, x, y);
        }
    }
}

/// Moves the divider of the split nearest pane `at` that runs `how` by `by` percent: `None` if
/// the pane isn't under `node`, else whether a divider moved.
fn moved(node: &mut Node, at: usize, how: Split, by: i16) -> Option<bool> {
    match node {
        Node::Pane(p) => (*p == at).then_some(false),
        Node::Split { how: runs, first, a, b } => {
            let done = match moved(a, at, how, by) {
                Some(done) => done,
                None => moved(b, at, how, by)?,
            };
            if !done && *runs == how {
                *first = (*first as i16 + by).clamp(SHARE.0, SHARE.1) as u16;
                return Some(true);
            }
            Some(done)
        }
    }
}

/// How far two spans overlap.
fn overlap(a: (u16, u16), b: (u16, u16)) -> u16 {
    a.1.min(b.1).saturating_sub(a.0.max(b.0))
}

#[cfg(test)]
impl View {
    pub(super) fn panes_in_order(&self) -> Vec<usize> {
        self.order()
    }

    pub(super) fn pane_with_keys(&self) -> usize {
        self.panes.focused
    }

    pub(super) fn parked_chat(&self, p: usize) -> Option<String> {
        self.panes.parked.get(p).and_then(|s| s.open.clone())
    }

    /// Where pane `p` was last drawn.
    pub(super) fn pane_area(&self, p: usize) -> Option<Rect> {
        self.panes.drawn.iter().find(|(q, _)| *q == p).map(|(_, r)| *r)
    }
}

impl View {
    /// Whether the conversation is split into panes.
    pub(super) fn split(&self) -> bool {
        self.experiments.panes && self.panes.tree.is_some()
    }

    /// The panes in the layout's order: pane 1 is the first.
    fn order(&self) -> Vec<usize> {
        let mut out = vec![];
        if let Some(tree) = &self.panes.tree {
            let mut placed = vec![];
            lay(tree, Rect::new(0, 0, 100, 100), &mut placed);
            out = placed.into_iter().map(|(p, _)| p).collect();
        }
        out
    }

    /// The parked pane that shows `chat`, if one does.
    fn parked_showing(&self, chat: &str) -> Option<usize> {
        if !self.split() {
            return None;
        }
        self.panes.parked.iter().position(|p| p.open.as_deref() == Some(chat))
    }

    /// Whether a pane shows `chat`: the one with the keys, or another.
    pub(super) fn on_screen(&self, chat: &str) -> bool {
        self.open.as_deref() == Some(chat) || self.parked_showing(chat).is_some()
    }

    /// The open chat's state and `s` swapped: the open chat's goes into it, and its comes out.
    fn exchange(&mut self, s: &mut Parked) {
        use std::mem::swap;
        swap(&mut self.open, &mut s.open);
        swap(&mut self.messages, &mut s.messages);
        swap(&mut self.focus, &mut s.focus);
        swap(&mut self.scroll, &mut s.scroll);
        swap(&mut self.seen, &mut s.seen);
        swap(&mut self.unread_at_open, &mut s.unread_at_open);
        swap(&mut self.placed, &mut s.placed);
        swap(&mut self.laid, &mut s.laid);
        swap(&mut self.unfolded, &mut s.unfolded);
        swap(&mut self.asked_place, &mut s.asked_place);
        swap(&mut self.composer, &mut s.composer);
        swap(&mut self.draft_sent, &mut s.draft_sent);
        swap(&mut self.draft_due, &mut s.draft_due);
        swap(&mut self.typing_at, &mut s.typing_at);
        swap(&mut self.read_upto, &mut s.read_upto);
        swap(&mut self.read_at, &mut s.read_at);
        swap(&mut self.read_due, &mut s.read_due);
        swap(&mut self.replying, &mut s.replying);
        swap(&mut self.editing, &mut s.editing);
        swap(&mut self.taken, &mut s.taken);
        swap(&mut self.dismissed, &mut s.dismissed);
        swap(&mut self.naming_selected, &mut s.naming_selected);
        swap(&mut self.unsent, &mut s.unsent);
        swap(&mut self.outside, &mut s.outside);
        swap(&mut self.jump, &mut s.jump);
        swap(&mut self.chomp_until, &mut s.chomp_until);
    }

    /// Pane `p`'s parked state swapped with what's in the open chat's place.
    fn exchange_with(&mut self, p: usize) {
        let mut s = std::mem::take(&mut self.panes.parked[p]);
        self.exchange(&mut s);
        self.panes.parked[p] = s;
    }

    /// `then`, with pane `p`'s parked state in the open chat's place for it.
    fn with_parked<R>(&mut self, p: usize, then: impl FnOnce(&mut View) -> R) -> R {
        self.exchange_with(p);
        let out = then(self);
        self.exchange_with(p);
        out
    }

    /// The chat with the keys left as switching chats leaves one, the library shown in its pane
    /// closed, its state parked in its pane and its window kept for it.
    fn park(&mut self) {
        self.leave_chat();
        self.close_library();
        self.focus = Focus::Typing;
        self.exchange_with(self.panes.focused);
        self.jawd.park_messages();
    }

    /// Pane `p`'s state taken out to have the keys, its window made the open one; its unread is
    /// read once it's on screen with them.
    fn unpark(&mut self, p: usize) {
        self.exchange_with(p);
        self.panes.focused = p;
        if let Some(chat) = self.open.clone() {
            self.jawd.unpark_messages(&chat);
            self.selected = Some(chat);
        }
        self.read_upto = None;
        self.read_due = None;
        self.reveal_selected_chat();
        self.dirty = true;
    }

    /// Gives pane `to` the keys.
    pub(super) fn focus_pane(&mut self, to: usize) {
        if !self.split() || to == self.panes.focused || to >= self.panes.parked.len() {
            return;
        }
        self.park();
        self.unpark(to);
    }

    /// The next chat in the list after the open one that no pane shows, from the top again.
    fn next_off_screen(&self) -> Option<String> {
        let order = self.list_order();
        let at = self.open.as_ref().and_then(|o| order.iter().position(|id| id == o)).map_or(0, |i| i + 1);
        (0..order.len())
            .map(|k| &order[(at + k) % order.len()])
            .find(|id| !self.on_screen(id) && self.chats.get(id).is_some_and(|c| c.kind != ChatKind::Community))
            .cloned()
    }

    /// Whether the pane with the keys, as last drawn, has room for two panes `how`.
    fn room_to_split(&self, how: Split) -> bool {
        let area = match self.panes.drawn.iter().find(|(p, _)| *p == self.panes.focused) {
            Some((_, area)) => *area,
            None => {
                let m = self.areas.messages;
                Rect { width: m.width + 4, height: m.height + 5, ..m }
            }
        };
        match how {
            Split::Across => area.width >= 2 * LEAST.0,
            Split::Down => area.height >= 2 * LEAST.1,
        }
    }

    /// Splits the pane with the keys `how`; the new pane takes them, with the next chat in the
    /// list that no pane shows.
    pub(super) fn split_pane(&mut self, how: Split) {
        if self.open.is_none() {
            return;
        }
        if self.split() && self.panes.parked.len() >= MOST {
            return self.say(format!("{MOST} panes at most"));
        }
        let Some(chat) = self.next_off_screen() else { return self.say("every chat is on screen already") };
        if !self.room_to_split(how) {
            return self.say("no room to split this pane; close one, or a bigger window");
        }
        if !self.split() {
            self.panes = Panes { parked: vec![Parked::default()], tree: Some(Node::Pane(0)), ..Panes::default() };
        }
        let (at, new) = (self.panes.focused, self.panes.parked.len());
        self.panes.parked.push(Parked::default());
        if let Some(tree) = self.panes.tree.as_mut() {
            split_at(tree, at, how, new);
        }
        self.park();
        self.panes.focused = new;
        self.open_chat(&chat);
    }

    /// Closes the pane with the keys, letting its window go; the pane before it in the layout, or
    /// the one after, takes them, and the last pane left is the conversation as it was.
    pub(super) fn close_pane(&mut self) {
        if !self.split() {
            return;
        }
        let (order, closing) = (self.order(), self.panes.focused);
        let k = order.iter().position(|p| *p == closing).unwrap_or(0);
        let next = if k > 0 { order[k - 1] } else { order[1] };
        self.leave_chat();
        self.jawd.park_messages();
        if let Some(chat) = self.open.clone() {
            self.jawd.unwatch_parked(&chat);
        }
        self.exchange_with(next);
        self.panes.parked.remove(closing);
        self.panes.tree = self.panes.tree.take().and_then(|tree| without(tree, closing));
        let next = if next > closing { next - 1 } else { next };
        self.panes.parked[next] = Parked::default();
        self.panes.focused = next;
        if let Some(chat) = self.open.clone() {
            self.jawd.unpark_messages(&chat);
            self.selected = Some(chat);
        }
        self.read_upto = None;
        self.read_due = None;
        if self.panes.parked.len() < 2 {
            self.panes = Panes::default();
        }
        self.reveal_selected_chat();
        self.dirty = true;
    }

    /// Swaps the pane with the keys with the one `by` places away in the layout; the keys go
    /// with it.
    fn swap_pane(&mut self, by: isize) {
        let order = self.order();
        let Some(k) = order.iter().position(|p| *p == self.panes.focused) else { return };
        let other = order[(k as isize + by).rem_euclid(order.len() as isize) as usize];
        let focused = self.panes.focused;
        if let Some(tree) = self.panes.tree.as_mut() {
            swapped(tree, focused, other);
        }
    }

    /// The pane `by` places from the one with the keys, in the layout's order.
    fn pane_after(&self, by: isize) -> Option<usize> {
        let order = self.order();
        let k = order.iter().position(|p| *p == self.panes.focused)?;
        order.get((k as isize + by).rem_euclid(order.len() as isize) as usize).copied()
    }

    /// The pane beside the one with the keys towards `dx`, `dy`, as last drawn: of those whose
    /// edge meets it on that side, the nearest, and of those the one alongside it most.
    fn pane_towards(&self, dx: i32, dy: i32) -> Option<usize> {
        let (_, from) = *self.panes.drawn.iter().find(|(p, _)| *p == self.panes.focused)?;
        self.panes
            .drawn
            .iter()
            .filter(|(p, _)| *p != self.panes.focused)
            .filter_map(|(p, r)| {
                let (gap, beside) = match (dx, dy) {
                    (1, 0) => (r.x.checked_sub(from.right())?, overlap((r.y, r.bottom()), (from.y, from.bottom()))),
                    (-1, 0) => (from.x.checked_sub(r.right())?, overlap((r.y, r.bottom()), (from.y, from.bottom()))),
                    (0, 1) => (r.y.checked_sub(from.bottom())?, overlap((r.x, r.right()), (from.x, from.right()))),
                    (0, -1) => (from.y.checked_sub(r.bottom())?, overlap((r.x, r.right()), (from.x, from.right()))),
                    _ => return None,
                };
                (beside > 0).then_some((gap, u16::MAX - beside, *p))
            })
            .min()
            .map(|(_, _, p)| p)
    }

    /// What the palette's pane actions do.
    pub(super) fn pane_do(&mut self, does: PaneDo) {
        match does {
            PaneDo::Split(how) => self.split_pane(how),
            PaneDo::Next => {
                if let Some(p) = self.pane_after(1) {
                    self.focus_pane(p);
                }
            }
            PaneDo::Close => self.close_pane(),
        }
    }

    /// A key after Ctrl-P, the prefix; says whether it took it. Once a divider has moved the
    /// prefix stays armed for more of that, and any other key is a key as ever.
    pub(super) fn on_pane_key(&mut self, k: KeyEvent) -> bool {
        let resizing = self.panes.prefix == Prefix::Resizing;
        self.panes.prefix = Prefix::Off;
        self.dirty = true;
        let held = k.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL);
        let divider = match k.code {
            KeyCode::Left if held => Some((Split::Across, -STEP)),
            KeyCode::Right if held => Some((Split::Across, STEP)),
            KeyCode::Up if held => Some((Split::Down, -STEP)),
            KeyCode::Down if held => Some((Split::Down, STEP)),
            _ => None,
        };
        if let Some((how, by)) = divider {
            let (split, focused) = (self.split(), self.panes.focused);
            if let Some(tree) = self.panes.tree.as_mut().filter(|_| split) {
                moved(tree, focused, how, by);
                self.panes.prefix = Prefix::Resizing;
            }
            return true;
        }
        if resizing {
            return false;
        }
        let go = |v: &mut View, p: Option<usize>| {
            if let Some(p) = p {
                v.focus_pane(p);
            }
        };
        match k.code {
            KeyCode::Char('%' | '|') => self.split_pane(Split::Across),
            KeyCode::Char('"' | '-') => self.split_pane(Split::Down),
            KeyCode::Char(c @ '1'..='9') => {
                let p = self.order().get(c as usize - '1' as usize).copied();
                go(self, p);
            }
            KeyCode::Char('o') | KeyCode::Tab => go(self, self.pane_after(1)),
            KeyCode::BackTab => go(self, self.pane_after(-1)),
            KeyCode::Left => go(self, self.pane_towards(-1, 0)),
            KeyCode::Right => go(self, self.pane_towards(1, 0)),
            KeyCode::Up => go(self, self.pane_towards(0, -1)),
            KeyCode::Down => go(self, self.pane_towards(0, 1)),
            KeyCode::Char('x') => self.close_pane(),
            KeyCode::Char('{') => self.swap_pane(-1),
            KeyCode::Char('}') => self.swap_pane(1),
            _ => {}
        }
        true
    }

    /// The footer's keys while the prefix is armed.
    pub(super) fn pane_hints(&self) -> Vec<(&'static str, &'static str, bool)> {
        if self.panes.prefix == Prefix::Resizing {
            return vec![("alt-←→↑↓", "more", true), ("any other key", "done", true)];
        }
        let split = self.split();
        vec![
            ("%", "across", true),
            ("\"", "down", true),
            ("1-6", "a pane", split),
            ("o", "next", split),
            ("x", "close", split),
            ("{ }", "swap", split),
            ("alt-←→↑↓", "size", split),
            ("esc", "never mind", true),
        ]
    }

    /// A click in a pane without the keys gives it them, and goes no further.
    pub(super) fn pane_click(&mut self, m: &MouseEvent) -> bool {
        if !self.split() || self.recorder.is_some() || m.kind != MouseEventKind::Down(MouseButton::Left) {
            return false;
        }
        let at = Position::new(m.column, m.row);
        match self.panes.drawn.iter().find(|(p, r)| *p != self.panes.focused && r.contains(at)) {
            Some(&(p, _)) => {
                self.focus_pane(p);
                true
            }
            None => false,
        }
    }

    /// Opening a chat another pane shows goes to that pane, and to the message asked for, if any;
    /// says whether it did.
    pub(super) fn open_in_its_pane(&mut self, chat: &str, at: Option<&str>) -> bool {
        let Some(p) = self.parked_showing(chat) else { return false };
        self.focus_pane(p);
        if let Some(m) = at {
            self.asked_place = None;
            self.watch(chat, View::around(m));
        }
        true
    }

    /// Alt-Up and Alt-Down pass over the chats other panes show.
    pub(super) fn step_past_panes(&mut self, by: isize) {
        while let Some(id) = self.selected.clone().filter(|id| self.parked_showing(id).is_some()) {
            self.move_selection(by.signum());
            if self.selected.as_deref() == Some(id.as_str()) {
                break;
            }
        }
    }

    /// A window's update for a chat a parked pane shows, into its window.
    pub(super) fn parked_messages_came(&mut self, chat: &str, update: Window<Message>) {
        let Some(p) = self.parked_showing(chat) else { return };
        let parked = &mut self.panes.parked[p];
        let Some(window) = parked.messages.as_mut().filter(|w| w.chat == chat) else { return };
        if update.moved.is_some() {
            parked.asked_place = None;
        }
        for id in window.apply(update) {
            parked.laid.remove(&id);
            parked.placed.remove(&id);
        }
    }

    /// The panes without the keys forget the lines they laid out, so a new theme draws them again
    /// in its colours.
    pub(super) fn restyle_panes(&mut self) {
        for parked in &mut self.panes.parked {
            parked.laid.clear();
        }
    }

    /// A message jawd didn't take, for a chat a parked pane shows: back in that pane's box, and its
    /// draft saved with it; says whether a parked pane shows it.
    pub(super) fn give_back_in_pane(&mut self, chat: &str, key: &str, text: &str, why: &str) -> bool {
        let Some(p) = self.parked_showing(chat) else { return false };
        let (chat, key, text, why) = (chat.to_string(), key.to_string(), text.to_string(), why.to_string());
        self.with_parked(p, move |v| {
            v.give_back(chat.clone(), key, text, why);
            v.save_draft(&chat);
        });
        true
    }

    /// The status line's panes, as tmux's has its windows: each one's number and chat, the one
    /// with the keys marked and how its chat is (`state`) after it, its name left to the box along
    /// the bottom when that says it (`named` false).
    pub(super) fn pane_tabs(&self, named: bool, state: Vec<Span<'static>>) -> Vec<Span<'static>> {
        let t = &self.theme;
        let pill = Style::new().bg(t.focus).fg(t.bg).add_modifier(Modifier::BOLD);
        let mut state = Some(state);
        let mut out = vec![];
        for (n, p) in self.order().into_iter().enumerate() {
            let keys = p == self.panes.focused;
            let row = match keys {
                true => self.open.as_deref().and_then(|id| self.chat_known(id)),
                false => {
                    let parked = &self.panes.parked[p];
                    parked.open.as_deref().and_then(|id| self.chats.get(id).or(parked.outside.as_ref()))
                }
            };
            let name = crate::fmt::fit(row.map_or("", |c| c.name.as_str()), TAB);
            if n > 0 {
                out.push(Span::raw(" "));
            }
            if !keys {
                out.push(Span::styled(format!(" {} ", n + 1), Style::new().fg(t.comment)));
                out.push(Span::styled(format!("{name} "), Style::new().fg(t.muted)));
                continue;
            }
            out.push(Span::styled(if named { format!(" {} {name} ", n + 1) } else { format!(" {} ", n + 1) }, pill));
            let state = state.take().unwrap_or_default();
            if !state.is_empty() {
                out.push(Span::raw(" "));
                out.extend(state);
            }
        }
        out
    }

    /// The panes into the conversation's place: those without the keys first, each drawn with its
    /// state in the open chat's place for a moment, then the one with them, so the cursor and what
    /// the keys work on are its.
    pub(super) fn draw_panes(&mut self, f: &mut Frame, area: Rect) {
        let Some(tree) = self.panes.tree.clone() else { return };
        let mut drawn = vec![];
        lay(&tree, area, &mut drawn);
        self.panes.drawn = drawn.clone();
        let focused = self.panes.focused;
        for (n, (p, place)) in drawn.iter().enumerate().filter(|(_, (p, _))| *p != focused) {
            self.with_parked(*p, |v| {
                let clickable = v.areas.message_rows.len();
                v.panes.drawing = true;
                v.draw_pane(f, *place, n + 1, false);
                v.panes.drawing = false;
                v.areas.message_rows.truncate(clickable);
            });
        }
        if let Some(n) = drawn.iter().position(|(p, _)| *p == focused) {
            self.draw_pane(f, drawn[n].1, n + 1, true);
        }
    }

    /// One pane: its number and chat in its border, which is blue for the pane with the keys, its
    /// messages, and its box unless the box is along the bottom; in a pane without the keys, the
    /// box shows what's typed in it, quieter.
    fn draw_pane(&mut self, f: &mut Frame, area: Rect, number: usize, keys: bool) {
        if keys && self.library.is_some() {
            return self.draw_library(f, area);
        }
        let t = self.theme;
        let number_style = if keys {
            Style::new().bg(t.focus).fg(t.bg).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(t.comment)
        };
        let mut title = vec![Span::styled(format!(" {number} "), number_style)];
        if let Some(chat) = self.open.as_deref().and_then(|id| self.chat_known(id)) {
            let name = if keys { format!(" {} ", chat.name) } else { format!("{} ", chat.name) };
            title.push(Span::styled(name, Style::new().fg(if keys { t.fg } else { t.text })));
            if !chat.typing.is_empty() {
                title.push(Span::styled(format!("· {} ", self.who_types(chat)), Style::new().fg(t.online)));
            }
        }
        let border = if keys && self.settings_panel.is_none() { t.focus } else { t.gutter };
        let block = self.panel(border).title(Line::from(title)).padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);
        if self.open.is_none() || inner.height == 0 {
            return;
        }
        if self.experiments.composer {
            return self.draw_messages(f, inner);
        }
        let box_h = if keys && self.recorder.is_some() {
            3
        } else {
            (self.composer.height(inner.width.saturating_sub(4) as usize) + 2) as u16
        };
        let [messages, below] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(box_h.min(inner.height))]).areas(inner);
        self.draw_messages(f, messages);
        if keys {
            self.draw_composer(f, below);
            self.draw_naming(f, below);
        } else {
            let block = self.panel(t.gutter).padding(Padding::horizontal(1));
            let inner = block.inner(below);
            f.render_widget(block, below);
            let (lines, _) = self.composer.view(inner.width as usize);
            let lines: Vec<Line> =
                lines.into_iter().map(|l| Line::from(Span::styled(l, Style::new().fg(t.muted)))).collect();
            f.render_widget(Paragraph::new(lines), inner);
        }
    }
}
