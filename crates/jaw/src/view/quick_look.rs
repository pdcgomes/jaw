//! The quick look: a short catch-up of the chats that need you, over the one you're in, from the
//! palette or the status line's key. It shows each chat's latest unread messages, a few chats and
//! a few lines each, from windows jaw watches only while it's open and lets go when it closes;
//! Enter goes to the chat selected, and Esc back to the one you were in.

use super::View;
use crate::data::{Message, Place};
use crate::fmt;
use jaw_client::{Messages, Update as Window};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding};

/// The most chats a quick look shows, and the most messages of each.
pub const CHATS: usize = 4;
pub const LINES: usize = 3;
/// Its widest.
const WIDTH: u16 = 84;
/// The column a sender's name takes before what they said.
const SENDER: usize = 14;

pub struct QuickLook {
    /// The chats shown, in the order they need you, each with its newest messages once jawd has
    /// sent them.
    pub chats: Vec<(String, Messages<Message>)>,
    pub selected: usize,
}

impl View {
    /// Opens the quick look on the chats that need you, as many as fit the screen, watching each
    /// one's newest messages.
    pub(super) fn open_quick_look(&mut self) {
        // A chat at most: its name, its messages, how many more, and a line before the next.
        let rows = self.areas.height.saturating_sub(6) as usize;
        let fit = if rows == 0 { CHATS } else { ((rows + 1) / (LINES + 3)).clamp(1, CHATS) };
        let ids: Vec<String> = self.needing().into_iter().take(fit).map(|(_, c)| c.id.clone()).collect();
        if ids.is_empty() {
            self.say("nothing needs you in the other chats");
            return;
        }
        self.jawd.watch_quick_look(&ids, LINES as u32);
        let chats = ids
            .into_iter()
            .map(|id| {
                let window = Messages::new(&id, Place::newest(LINES as u32));
                (id, window)
            })
            .collect();
        self.quick_look = Some(QuickLook { chats, selected: 0 });
        self.dirty = true;
    }

    /// Closes the quick look, letting its windows go.
    pub(super) fn close_quick_look(&mut self) {
        if self.quick_look.take().is_some() {
            self.jawd.unwatch_quick_look();
            self.dirty = true;
        }
    }

    pub(super) fn quick_look_came(&mut self, chat: &str, update: Window<Message>) {
        let Some(q) = self.quick_look.as_mut() else { return };
        if let Some((_, window)) = q.chats.iter_mut().find(|(id, _)| id == chat) {
            window.apply(update);
        }
    }

    pub(super) fn on_quick_look_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let Some(q) = self.quick_look.as_mut() else { return };
        let last = q.chats.len().saturating_sub(1);
        match k.code {
            KeyCode::Up | KeyCode::BackTab => q.selected = q.selected.saturating_sub(1),
            KeyCode::Char('k') if !ctrl => q.selected = q.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => q.selected = (q.selected + 1).min(last),
            KeyCode::Char('j') if !ctrl => q.selected = (q.selected + 1).min(last),
            KeyCode::Enter => {
                let Some(id) = q.chats.get(q.selected).map(|(id, _)| id.clone()) else { return };
                self.close_quick_look();
                self.close_library();
                self.sidebar.keys = None;
                self.open_chat(&id);
                self.jawd.opened(&id);
            }
            KeyCode::Esc => self.close_quick_look(),
            KeyCode::Char('g') if ctrl => self.close_quick_look(),
            _ => {}
        }
    }

    pub(super) fn quick_look_hints(&self) -> Vec<(&'static str, &'static str, bool)> {
        vec![("↑↓", "moves", true), ("⏎", "goes to it", true), ("esc", "back", true)]
    }

    /// A chat's lines in the quick look: its name and how much is unread, then its latest unread
    /// messages, or what its row last said until they come.
    fn looked_at(&self, id: &str, window: &Messages<Message>, w: usize) -> Vec<Line<'static>> {
        let t = &self.theme;
        let Some(chat) = self.chat_known(id) else { return vec![] };
        let mention = chat.mentions > 0;
        let mut left = vec![];
        if mention {
            left.push(Span::styled("@ ", Style::new().fg(t.mention).add_modifier(Modifier::BOLD)));
        }
        let mut parts = vec![];
        if mention {
            let said = format!("@{}", chat.mentions);
            parts.push(Span::styled(said, Style::new().fg(t.mention).add_modifier(Modifier::BOLD)));
        }
        if chat.unread > 0 {
            let said = format!("{} unread", fmt::num(chat.unread as u64));
            parts.push(Span::styled(said, Style::new().fg(t.unread).add_modifier(Modifier::BOLD)));
        }
        if let Some(last) = &chat.last {
            parts.push(Span::styled(fmt::when(last.at, self.now(), self.tz), Style::new().fg(t.comment)));
        }
        let mut right = vec![];
        for (i, part) in parts.into_iter().enumerate() {
            if i > 0 {
                right.push(Span::styled(" · ", Style::new().fg(t.comment)));
            }
            right.push(part);
        }
        let right_w: usize = right.iter().map(Span::width).sum();
        let lead_w: usize = left.iter().map(Span::width).sum();
        let name = fmt::fit(&chat.name, w.saturating_sub(lead_w + right_w + 1));
        left.push(Span::styled(name, Style::new().fg(t.fg).add_modifier(Modifier::BOLD)));
        let used: usize = left.iter().map(Span::width).sum::<usize>() + right_w;
        left.push(Span::raw(" ".repeat(w.saturating_sub(used))));
        left.extend(right);
        let mut lines = vec![Line::from(left)];
        if !window.loaded {
            let said = chat.last.as_ref().map_or_else(String::new, |l| match &l.from {
                Some(from) => format!("{from}: {}", fmt::one_line(&l.summary)),
                None => fmt::one_line(&l.summary),
            });
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(fmt::fit(&said, w.saturating_sub(2)), Style::new().fg(t.muted)),
            ]));
            return lines;
        }
        let wanted = (chat.unread as usize).clamp(1, LINES);
        let theirs: Vec<&Message> = window.rows().filter(|m| !m.mine).collect();
        let shown: Vec<&Message> = if theirs.is_empty() {
            window.rows().rev().take(1).collect()
        } else {
            theirs[theirs.len().saturating_sub(wanted)..].to_vec()
        };
        let text_w = w.saturating_sub(2 + SENDER + 11);
        let more = (chat.unread as usize).saturating_sub(shown.len());
        for m in shown {
            let (who, colour) = match (&m.from, m.mine) {
                (Some(f), false) => (f.name.clone(), t.sender(f.colour)),
                _ => ("you".to_string(), t.muted),
            };
            let who = fmt::fit(&who, SENDER - 1);
            let said = if m.deleted { "⊘ deleted".to_string() } else { super::actions::one_line(m) };
            let said = fmt::fit(&said, text_w);
            let when = fmt::when(m.at, self.now(), self.tz);
            let pad = SENDER.saturating_sub(fmt::width(&who));
            let gap = (text_w + 11).saturating_sub(fmt::width(&said) + fmt::width(&when));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(who, Style::new().fg(colour).add_modifier(Modifier::BOLD)),
                Span::raw(" ".repeat(pad)),
                Span::styled(said, Style::new().fg(t.text)),
                Span::raw(" ".repeat(gap)),
                Span::styled(when, Style::new().fg(t.comment)),
            ]));
        }
        if more > 0 {
            let said = format!("  ⋯ {} more", fmt::num(more as u64));
            lines.push(Line::from(Span::styled(said, Style::new().fg(t.comment))));
        }
        lines
    }

    /// The quick look over the screen, the screen behind it quieter.
    pub(super) fn draw_quick_look(&mut self, f: &mut Frame) {
        let t = self.theme;
        let screen = f.area();
        let Some(q) = self.quick_look.as_ref() else { return };
        let w = WIDTH.min(screen.width.saturating_sub(4));
        if w < 30 || screen.height < 8 {
            return;
        }
        let iw = w.saturating_sub(4) as usize;
        let mut lines: Vec<(Line<'static>, bool)> = vec![];
        for (i, (id, window)) in q.chats.iter().enumerate() {
            if i > 0 {
                lines.push((Line::default(), false));
            }
            for (k, line) in self.looked_at(id, window, iw).into_iter().enumerate() {
                lines.push((line, k == 0 && i == q.selected));
            }
        }
        let h = (lines.len() as u16 + 2).min(screen.height.saturating_sub(4));
        let area = Rect::new(screen.x + (screen.width - w) / 2, screen.y + 2, w, h);
        for y in screen.y..screen.y + screen.height {
            for x in screen.x..screen.x + screen.width {
                if !(x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height) {
                    let cell = &mut f.buffer_mut()[(x, y)];
                    cell.set_fg(t.gutter);
                    cell.modifier = Modifier::empty();
                }
            }
        }
        f.render_widget(Clear, area);
        let (shown, needing) = (q.chats.len(), self.needing().len().max(q.chats.len()));
        let said = match (shown, needing) {
            (1, 1) => "· 1 chat needs you ".to_string(),
            (s, n) if s == n => format!("· {n} chats need you "),
            (s, n) => format!("· {s} of the {n} chats that need you "),
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(t.focus))
            .style(Style::new().bg(t.surface).fg(t.fg))
            .title(Line::from(vec![
                Span::styled(" quick look ", Style::new().fg(t.text)),
                Span::styled(said, Style::new().fg(t.comment)),
            ]))
            .title_bottom(Line::from(Span::styled(" ↑↓ moves · ⏎ goes to it · esc back ", Style::new().fg(t.muted))))
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let buf = f.buffer_mut();
        for (i, (line, selected)) in lines.into_iter().take(inner.height as usize).enumerate() {
            let y = inner.y + i as u16;
            buf.set_line(inner.x, y, &line, inner.width);
            if selected {
                buf.set_style(
                    Rect::new(area.x + 1, y, area.width - 2, 1),
                    Style::new().bg(t.selection).add_modifier(Modifier::BOLD),
                );
            }
        }
    }
}
