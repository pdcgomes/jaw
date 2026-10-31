//! The status line, as BitchX and the Claude command line keep one: a line above the message box,
//! always on screen, with the chat you're in and how it is (typing, online, muted, catching up),
//! and what needs you in the others, by name where it matters, such as "Ana · 2" or
//! "@ Família Gomes", and counted otherwise. It reads the chat list's window and the roster jaw
//! already watches, and draws from `&self`, so it never asks jawd for anything.

use super::View;
use crate::data::{Chat, ChatKind};
use crate::fmt;
use jaw_proto::ConnectionState;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// How a chat that isn't open needs you, the first the most.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Need {
    /// It mentions you, muted or not: named.
    Mention,
    /// Someone writing to you alone: named.
    Person,
    /// A group with unread messages: counted.
    Group,
}

/// The longest a chat's name runs on the status line.
const NAME: usize = 24;

fn width(spans: &[Span]) -> usize {
    spans.iter().map(Span::width).sum()
}

impl View {
    /// Whether the status line is on screen: switched on, or while one chat has the screen, when
    /// it's the only glimpse of the rest.
    pub(super) fn status_line_shown(&self) -> bool {
        self.experiments.status_line || self.focused()
    }

    /// The chats no pane shows that need you, mentions first, then people writing to you, then
    /// groups, each in the chat list's order; a muted chat only when it mentions you.
    pub(super) fn needing(&self) -> Vec<(Need, &Chat)> {
        let now = self.now();
        let mut out: Vec<(Need, &Chat)> = self
            .chats
            .rows()
            .filter(|c| c.kind != ChatKind::Community && !self.on_screen(&c.id))
            .filter_map(|c| match (c.mentions > 0, c.unread > 0 || c.marked_unread, c.muted(now), c.group()) {
                (true, ..) => Some((Need::Mention, c)),
                (false, true, false, false) => Some((Need::Person, c)),
                (false, true, false, true) => Some((Need::Group, c)),
                _ => None,
            })
            .collect();
        out.sort_by_key(|(need, _)| *need);
        out
    }

    /// The chat you're in, and how it is: who's typing, or whether they're online or when they
    /// were seen; muted; catching up. Its name only where nothing else on the line would say it:
    /// the box along the bottom, right below, says "to" it.
    fn status_here(&self) -> Vec<Span<'static>> {
        let t = &self.theme;
        let Some(chat) = self.open.as_deref().and_then(|id| self.chat_known(id)) else { return vec![] };
        let pill = Style::new().bg(t.focus).fg(t.bg).add_modifier(Modifier::BOLD);
        let mut said: Vec<Span<'static>> = vec![];
        if let Some(who) = self.who_types(chat) {
            said.push(Span::styled(who, Style::new().fg(t.online)));
        } else if let Some((presence, style)) = self.presence_said(&chat.id) {
            said.push(Span::styled(presence, style));
        }
        if chat.muted(self.now()) {
            said.push(Span::styled("muted", Style::new().fg(t.comment)));
        }
        if self.connection.state == ConnectionState::CatchingUp {
            said.push(Span::styled("catching up", Style::new().fg(t.comment)));
        }
        let named = !self.composer_below();
        let mut state = vec![];
        for (i, s) in said.into_iter().enumerate() {
            if i > 0 {
                state.push(Span::styled(" · ", Style::new().fg(t.comment)));
            }
            state.push(s);
        }
        let mut out = vec![Span::raw(" ")];
        if self.split() {
            out.extend(self.pane_tabs(named, state));
            return out;
        }
        if named {
            out.push(Span::styled(format!(" {} ", fmt::fit(&chat.name, NAME)), pill));
        }
        if !state.is_empty() {
            out.push(Span::styled(if named { "  " } else { " " }, Style::new().fg(t.comment)));
            out.extend(state);
        }
        out
    }

    /// One chat that needs you, by name: a mention as "@ Família Gomes", someone as "Ana · 2",
    /// with a dot while they type.
    fn named(&self, need: Need, chat: &Chat) -> Vec<Span<'static>> {
        let t = &self.theme;
        match need {
            Need::Mention => vec![Span::styled(
                format!("@ {}", fmt::fit(&chat.name, NAME)),
                Style::new().fg(t.mention).add_modifier(Modifier::BOLD),
            )],
            _ => {
                let first = chat.name.split_whitespace().next().unwrap_or(&chat.name);
                let mut out = vec![];
                if !self.typists(&chat.id).is_empty() {
                    out.push(Span::styled("● ", Style::new().fg(t.online)));
                }
                out.push(Span::styled(fmt::fit(first, NAME), Style::new().fg(t.fg)));
                out.push(Span::styled(" · ", Style::new().fg(t.comment)));
                let n = if chat.unread > 0 { fmt::num(chat.unread as u64) } else { "1".into() };
                out.push(Span::styled(n, Style::new().fg(t.unread).add_modifier(Modifier::BOLD)));
                out
            }
        }
    }

    /// The chats not named, counted, at length `0` ("3 more chats · 26 unread"), `1` ("3 more
    /// chats") or `2` ("+3").
    fn counted(&self, chats: &[&Chat], more: bool, length: u8) -> Vec<Span<'static>> {
        let t = &self.theme;
        if chats.is_empty() {
            return vec![];
        }
        let n = chats.len();
        let what = match (length, more, n) {
            (2, true, n) => format!("+{n}"),
            (_, true, 1) => "1 more chat".to_string(),
            (_, true, n) => format!("{n} more chats"),
            (_, false, 1) => "1 chat".to_string(),
            (_, false, n) => format!("{n} chats"),
        };
        let mut out = vec![Span::styled(what, Style::new().fg(t.muted))];
        if length == 0 {
            let unread: u64 = chats.iter().map(|c| u64::from(c.unread)).sum();
            out.push(Span::styled(" · ", Style::new().fg(t.comment)));
            out.push(Span::styled(format!("{} unread", fmt::num(unread)), Style::new().fg(t.muted)));
        }
        out
    }

    /// What needs you elsewhere in at most `room` columns: by name while the names fit beside the
    /// rest counted at its shortest, then the count as long as it fits.
    fn status_elsewhere(&self, room: usize) -> Vec<Span<'static>> {
        let t = &self.theme;
        let needing = self.needing();
        if needing.is_empty() {
            let said = "nothing new elsewhere";
            return if said.len() <= room { vec![Span::styled(said, Style::new().fg(t.comment))] } else { vec![] };
        }
        const GAP: &str = "   ";
        let worth_naming = needing.iter().take_while(|(need, _)| *need != Need::Group).count();
        let rest = |from: usize| -> Vec<&Chat> { needing[from..].iter().map(|(_, c)| *c).collect() };
        let mut out: Vec<Span<'static>> = vec![];
        let mut shown = 0;
        for (i, (need, chat)) in needing.iter().enumerate().take(worth_naming) {
            let item = self.named(*need, chat);
            let tail = self.counted(&rest(i + 1), true, 2);
            let gap = if out.is_empty() { 0 } else { GAP.len() };
            let tail_w = if tail.is_empty() { 0 } else { GAP.len() + width(&tail) };
            if width(&out) + gap + width(&item) + tail_w > room {
                break;
            }
            if gap > 0 {
                out.push(Span::raw(GAP));
            }
            out.extend(item);
            shown += 1;
        }
        let gap = if out.is_empty() { 0 } else { GAP.len() };
        let tail = (0..3)
            .map(|length| self.counted(&rest(shown), shown > 0, length))
            .find(|tail| width(&out) + gap + width(tail) <= room)
            .unwrap_or_default();
        if !tail.is_empty() {
            if gap > 0 {
                out.push(Span::raw(GAP));
            }
            out.extend(tail);
        }
        out
    }

    /// The status line into its row: the chat you're in on the left, what needs you elsewhere on
    /// the right, and the quick look's key at the end.
    pub(super) fn draw_status_line(&self, buf: &mut Buffer, area: Rect) {
        let t = &self.theme;
        buf.set_style(area, Style::new().bg(t.surface).fg(t.text));
        let w = area.width as usize;
        let mut left = self.status_here();
        let mut key = vec![];
        if self.experiments.quick_look {
            key.push(Span::raw("   "));
            key.push(Span::styled("ctrl-g", Style::new().fg(t.focus).add_modifier(Modifier::BOLD)));
            key.push(Span::styled(" look ", Style::new().fg(t.muted)));
        } else {
            key.push(Span::raw(" "));
        }
        if width(&left) + width(&key) > w {
            left.truncate(2);
        }
        let room = w.saturating_sub(width(&left) + width(&key) + 3);
        let right = self.status_elsewhere(room);
        let used = width(&left) + width(&right) + width(&key);
        let mut spans = left;
        spans.push(Span::raw(" ".repeat(w.saturating_sub(used))));
        spans.extend(right);
        spans.extend(key);
        buf.set_line(area.x, area.y, &Line::from(spans), area.width);
    }
}
