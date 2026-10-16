//! Drawing: the header in a top border of its own, the chat list and the conversation in rounded
//! panels with their titles in the top border, the message box inside the conversation, and the
//! keys that matter now in the last line. The panel with the keys' attention is outlined in blue,
//! and orange is kept for what needs you.

use super::{Focus, LIST, NARROW, Overlay, Placed, Reach, Scroll, View};
use crate::art::{self, Look};
use crate::data::{Chat, Message, Tick};
use crate::fmt;
use crate::layout;
use jaw_client::Messages;
use jaw_proto::{ConnectionState, LinkStep};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

/// A message on screen with what comes before it: a day's separator or a blank line.
struct Placing {
    id: String,
    sort: String,
    top: i32,
    day: Option<String>,
    blank: bool,
    lines: Rc<crate::layout::Laidout>,
    detail: Option<Line<'static>>,
}

impl Placing {
    fn lead(&self) -> i32 {
        i32::from(self.blank) + i32::from(self.day.is_some())
    }

    fn height(&self) -> i32 {
        self.lead() + self.lines.lines.len() as i32 + i32::from(self.detail.is_some())
    }

    fn bottom(&self) -> i32 {
        self.top + self.height()
    }
}

/// `left` and `right` on one line `width` wide, `right` against the end.
fn fill(left: Vec<Span<'static>>, right: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let used: usize = left.iter().chain(&right).map(|s| s.width()).sum();
    let mut spans = left;
    spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
    spans.extend(right);
    Line::from(spans)
}

/// A line drawn into the buffer as `set_line` would, but leaving cells that only a plain space
/// would cover as the panel filled them, which spares a frame most of its width, and honouring a
/// centred line's alignment.
fn put(buf: &mut Buffer, x: u16, y: u16, line: &Line, width: u16) {
    let end = x + width;
    let mut at = x;
    if line.alignment == Some(ratatui::layout::Alignment::Center) {
        at += width.saturating_sub(line.width() as u16) / 2;
    }
    for span in &line.spans {
        if at >= end {
            break;
        }
        let style = line.style.patch(span.style);
        if style == Style::default() && span.content.bytes().all(|b| b == b' ') {
            at = at.saturating_add(span.content.len() as u16);
            continue;
        }
        // Plain ASCII is a cell a byte, so it goes straight in; the rest to ratatui, which knows
        // graphemes and wide characters.
        let text: &str = &span.content;
        let ascii = text.bytes().position(|b| !b.is_ascii_graphic() && b != b' ').unwrap_or(text.len());
        for c in text[..ascii].chars() {
            if at >= end {
                return;
            }
            buf[(at, y)].set_char(c).set_style(style);
            at += 1;
        }
        if ascii < text.len() && at < end {
            at = buf.set_stringn(at, y, &text[ascii..], (end - at) as usize, style).0;
        }
    }
}

fn centred(lines: Vec<Line<'static>>, area: Rect) -> Paragraph<'static> {
    let pad = area.height.saturating_sub(lines.len() as u16) / 2;
    let mut out = vec![Line::default(); pad as usize];
    out.extend(lines.into_iter().map(|l| l.centered()));
    Paragraph::new(out)
}

impl View {
    fn panel(&self, border: Color) -> Block<'static> {
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(border))
            .style(Style::new().bg(self.theme.bg).fg(self.theme.fg))
    }

    /// A panel's title as ooze writes one: its name, then what more there is to say, dimmer.
    fn title(&self, name: &str, more: Vec<Span<'static>>) -> Line<'static> {
        let mut spans = vec![Span::styled(format!(" {name} "), Style::new().fg(self.theme.text))];
        spans.extend(more);
        Line::from(spans)
    }

    pub fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        let t = self.theme;
        self.areas = super::Areas { width: area.width, height: area.height, ..Default::default() };
        self.drawn_at = self.now() / 1_000;
        if self.drawn_size != (area.width, area.height) {
            self.drawn_size = (area.width, area.height);
            self.images.forget_drawn();
        }
        self.images.begin();
        let ground = Style::new().bg(t.bg).fg(t.fg);
        if area.height < 4 || area.width < 20 {
            f.render_widget(Paragraph::new("jaw needs a bigger window").style(ground), area);
            return;
        }
        let [top, body, foot] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]).areas(area);
        // The panels fill the body themselves; the header and the keys have only this.
        f.buffer_mut().set_style(top, ground);
        f.buffer_mut().set_style(foot, ground);
        self.draw_header(f.buffer_mut(), top);
        match &self.reach {
            Reach::Waiting => f.render_widget(self.panel(t.gutter), body),
            Reach::Starting { failed } => {
                let failed = failed.clone();
                self.draw_starting(f, body, failed);
            }
            Reach::Unreachable { .. } if self.chats.is_empty() => self.draw_asleep(f, body),
            _ if self.linking() => self.draw_linking(f, body),
            _ => self.draw_main(f, body),
        }
        self.draw_hints(f.buffer_mut(), foot);
        if self.palette.is_some() {
            self.draw_palette(f);
        }
        if self.overlay == Some(Overlay::Keys) {
            self.draw_keys(f);
        }
        self.images.end();
        if self.no_color {
            let selection = t.selection;
            for cell in f.buffer_mut().content.iter_mut() {
                if cell.bg == selection {
                    cell.modifier.insert(Modifier::REVERSED);
                }
                cell.set_fg(Color::Reset);
                cell.set_bg(Color::Reset);
            }
        }
    }

    /// Whether the header should be orange: something needs you, or will soon.
    fn troubled(&self) -> bool {
        matches!(self.reach, Reach::Unreachable { .. })
            || matches!(
                self.connection.state,
                ConnectionState::PhoneOffline
                    | ConnectionState::Offline
                    | ConnectionState::LoggedOut
                    | ConnectionState::UpdateNeeded
                    | ConnectionState::Banned
            )
    }

    fn header_state(&self, width: usize) -> Vec<Span<'static>> {
        let t = &self.theme;
        let dim = |s: String| Span::styled(s, Style::new().fg(t.comment));
        let warn = |s: String| Span::styled(s, Style::new().fg(t.warn));
        let now = self.now();
        let c = &self.connection;
        match &self.reach {
            Reach::Waiting => return vec![dim("connecting to jawd…".into())],
            Reach::Starting { failed: None } => return vec![dim("starting jawd…".into())],
            Reach::Starting { failed: Some(_) } => return vec![warn("jawd didn't start".into())],
            Reach::Unreachable { retry_at, .. } => {
                let s = ((retry_at - now).max(0) + 999) / 1000;
                return vec![warn(format!("can't reach jawd · trying again in {s} s"))];
            }
            Reach::Up => {}
        }
        if c.state == ConnectionState::LoggedOut {
            return vec![warn("logged out from the phone".into())];
        }
        if self.linked == Some(false) {
            return vec![dim("not linked".into())];
        }
        match c.state {
            ConnectionState::Connecting => vec![dim("connecting".into())],
            ConnectionState::Online => vec![Span::styled("online", Style::new().fg(t.text))],
            ConnectionState::CatchingUp => {
                let mut out = vec![Span::styled("online · ", Style::new().fg(t.text))];
                match (c.done, c.total) {
                    (Some(done), Some(total)) if total > 0 => {
                        out.push(dim(format!("catching up {} of {} ", fmt::num(done), fmt::num(total))));
                        let bar = (width / 4).clamp(8, 40);
                        let full = ((done as f64 / total as f64).clamp(0.0, 1.0) * bar as f64).round() as usize;
                        out.push(Span::styled("━".repeat(full), Style::new().fg(t.progress)));
                        out.push(Span::styled("━".repeat(bar - full), Style::new().fg(t.gutter)));
                    }
                    _ => out.push(dim("catching up".into())),
                }
                out
            }
            ConnectionState::PhoneOffline => vec![
                Span::styled("online · ", Style::new().fg(t.text)),
                warn("phone offline: photos and older history wait for it".into()),
            ],
            ConnectionState::Offline => vec![warn("offline: what you send waits until it's back".into())],
            ConnectionState::UpdateNeeded => {
                vec![warn("update needed: WhatsApp wants a newer jaw".into()), dim(" · reading still works".into())]
            }
            ConnectionState::Banned => {
                let until = c.until.map(|u| format!(" until {}", fmt::until(u, self.tz))).unwrap_or_default();
                vec![warn(format!("temporarily banned{until}"))]
            }
            ConnectionState::EngineRestarting => vec![dim("reconnecting: the engine restarted".into())],
            ConnectionState::LoggedOut => vec![],
        }
    }

    fn draw_header(&self, buf: &mut Buffer, area: Rect) {
        let t = &self.theme;
        let border = Style::new().fg(if self.troubled() { t.warn } else { t.gutter });
        let w = area.width as usize;
        let pill = Span::styled(" jaw ", Style::new().bg(t.brand).fg(t.bg).add_modifier(Modifier::BOLD));
        let mut left = vec![Span::styled("╭ ", border), pill, Span::raw(" ")];
        left.extend(self.header_state(w));
        left.push(Span::raw(" "));
        let mut right = vec![Span::raw(" ")];
        let unread: u32 = self.chats.rows().map(|c| c.unread).sum();
        if unread > 0 && self.reach == Reach::Up && !self.linking() {
            right.push(Span::styled(format!("{} unread", fmt::num(unread as u64)), Style::new().fg(t.text)));
            right.push(Span::styled(" · ", Style::new().fg(t.comment)));
        }
        right.push(Span::styled(fmt::clock(self.now(), self.tz), Style::new().fg(t.text)));
        right.push(Span::styled(" ╮", border));
        let used: usize = left.iter().chain(&right).map(|s| s.width()).sum();
        if used > w {
            left.truncate(3);
        }
        let used: usize = left.iter().chain(&right).map(|s| s.width()).sum();
        let mut spans = left;
        spans.push(Span::styled("─".repeat(w.saturating_sub(used)), border));
        spans.extend(right);
        buf.set_line(area.x, area.y, &Line::from(spans), area.width);
    }

    fn hints(&self) -> Vec<(&'static str, &'static str, bool)> {
        if self.overlay.is_some() {
            return vec![("any key", "closes", true)];
        }
        if self.palette.is_some() {
            return vec![
                ("⏎", "does it", true),
                ("tab", "narrows", true),
                ("ctrl-g", "its message", true),
                ("esc", "closes", true),
            ];
        }
        if self.linking() {
            return match (&self.phone, &self.link, self.connection.state) {
                (Some(_), _, _) => vec![("⏎", "get a code", true), ("esc", "back to the QR code", true)],
                (None, Some(LinkStep::Code { .. }), _) => vec![("esc", "back to the QR code", true)],
                (None, _, ConnectionState::LoggedOut) if !self.link_asked => {
                    vec![("⏎", "link again", true), ("x", "delete the messages", false), ("q", "quit", true)]
                }
                _ => vec![("p", "link with a phone number", true), ("q", "quit", true)],
            };
        }
        // Out of reach with the screen kept, its keys still work on what's there.
        if self.reach != Reach::Up && !(matches!(self.reach, Reach::Unreachable { .. }) && !self.chats.is_empty()) {
            return vec![("ctrl-c", "quit", true)];
        }
        if self.narrow() && self.picking {
            return vec![
                ("↑↓", "move", true),
                ("⏎", "open", true),
                ("alt-↑↓", "chats", true),
                ("?", "keys", true),
                ("ctrl-c", "quit", true),
            ];
        }
        match &self.focus {
            Focus::Message(_) => {
                vec![
                    ("r", "reply", false),
                    ("e", "edit", false),
                    ("d", "delete", false),
                    ("+", "react", false),
                    ("f", "forward", false),
                    ("y", "copy", true),
                    ("⏎", "open", true),
                    ("i", "info", false),
                    ("esc", "back to typing", true),
                ]
            }
            Focus::Typing => {
                let mut keys = vec![
                    ("⏎", "send", true),
                    ("alt-⏎", "new line", true),
                    ("↑", "messages", true),
                    ("ctrl-k", "find", true),
                    ("alt-↑↓", "chats", true),
                ];
                if self.narrow() {
                    keys.push(("esc", "chat list", true));
                }
                keys.push(("?", "keys", true));
                keys
            }
        }
    }

    fn draw_hints(&self, buf: &mut Buffer, area: Rect) {
        let t = &self.theme;
        if let Some((text, _)) = &self.notice {
            let line = Line::from(Span::styled(format!(" {text} "), Style::new().fg(t.notice)));
            buf.set_line(area.x, area.y, &line, area.width);
            return;
        }
        let mut spans = vec![Span::raw(" ")];
        for (key, what, on) in self.hints() {
            let (k, w) = if on {
                (Style::new().fg(t.focus).add_modifier(Modifier::BOLD), Style::new().fg(t.muted))
            } else {
                (Style::new().fg(t.gutter), Style::new().fg(t.gutter))
            };
            spans.push(Span::styled(key, k));
            spans.push(Span::styled(format!(" {what}  "), w));
        }
        buf.set_line(area.x, area.y, &Line::from(spans), area.width);
    }

    fn draw_main(&mut self, f: &mut Frame, body: Rect) {
        if body.width < NARROW {
            if self.picking || self.open.is_none() {
                self.draw_list(f, body, true);
            } else {
                self.draw_conversation(f, body);
            }
            return;
        }
        let [list, conversation] = Layout::horizontal([Constraint::Length(LIST), Constraint::Min(20)]).areas(body);
        self.draw_list(f, list, false);
        self.draw_conversation(f, conversation);
    }

    fn draw_list(&mut self, f: &mut Frame, area: Rect, focused: bool) {
        let t = self.theme;
        let border = if focused && self.overlay.is_none() { t.focus } else { t.gutter };
        let title = self.title("chats", vec![Span::styled("· All ", Style::new().fg(t.comment))]);
        let block = self.panel(border).title(title).padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);
        self.areas.list = inner;
        self.list_rows = (inner.height / 2) as usize;
        self.clamp_list();
        if self.chats.is_empty() {
            let line = Line::from(Span::styled("No chats yet.", Style::new().fg(t.comment)));
            f.buffer_mut().set_line(inner.x, inner.y, &line, inner.width);
            return;
        }
        let order: Vec<String> =
            self.chats.rows().skip(self.list_top).take(self.list_rows).map(|c| c.id.clone()).collect();
        let w = inner.width as usize;
        let now = self.now();
        for (k, id) in order.iter().enumerate() {
            let Some(chat) = self.chats.get(id) else { continue };
            let y = inner.y + 2 * k as u16;
            let selected = self.selected.as_deref() == Some(id.as_str());
            let open = self.open.as_deref() == Some(id.as_str());
            let key = (chat.v, open, w, now / fmt::MINUTE);
            let rows = match self.rows_laid.get(id.as_str()) {
                Some(laid) if laid.key == key => laid.rows.clone(),
                _ => {
                    let rows = Rc::new(self.chat_row(chat, open, w, now));
                    self.rows_laid.insert(id.clone(), super::ChatRows { key, rows: rows.clone() });
                    rows
                }
            };
            let (first, second) = &*rows;
            let buf = f.buffer_mut();
            put(buf, inner.x, y, first, inner.width);
            if y + 1 < inner.y + inner.height {
                put(buf, inner.x, y + 1, second, inner.width);
            }
            if selected {
                let band = Rect::new(area.x + 1, y, area.width.saturating_sub(2), 2.min(inner.y + inner.height - y));
                buf.set_style(band, Style::new().bg(t.selection).add_modifier(Modifier::BOLD));
            }
            self.areas.chat_rows.push((y, id.clone()));
        }
    }

    /// A chat's two rows in the list: its name, time and counts, then what it last said.
    fn chat_row(&self, chat: &Chat, open: bool, w: usize, now: i64) -> (Line<'static>, Line<'static>) {
        let t = self.theme;
        let muted = chat.muted(now);
        let mut right: Vec<Span<'static>> = vec![];
        if let Some(last) = &chat.last {
            right.push(Span::styled(fmt::when(last.at, now, self.tz), Style::new().fg(t.comment)));
        }
        if chat.mentions > 0 {
            right.push(Span::raw(" "));
            right.push(Span::styled(
                format!("@{}", chat.mentions),
                Style::new().fg(t.mention).add_modifier(Modifier::BOLD),
            ));
        }
        if chat.unread > 0 {
            right.push(Span::raw(" "));
            let style =
                if muted { Style::new().fg(t.comment) } else { Style::new().fg(t.unread).add_modifier(Modifier::BOLD) };
            right.push(Span::styled(chat.unread.to_string(), style));
        }
        let right_w: usize = right.iter().map(|s| s.width()).sum();
        let typing = !chat.typing.is_empty();
        let mark = if typing { Span::styled("●", Style::new().fg(t.online)) } else { Span::raw(" ") };
        let pin = if chat.pinned { " ▴" } else { "" };
        let name_w = w.saturating_sub(2 + right_w + pin.len().min(2) + 1);
        let name_style = Style::new().fg(if muted { t.muted } else { t.fg });
        let name_style = if chat.unread > 0 { name_style.add_modifier(Modifier::BOLD) } else { name_style };
        let first = fill(
            vec![
                mark,
                Span::raw(" "),
                Span::styled(fmt::fit(&chat.name, name_w), name_style),
                Span::styled(pin, Style::new().fg(t.comment)),
            ],
            right,
            w,
        );
        let preview_w = w.saturating_sub(2);
        let mut second = vec![Span::raw("  ")];
        if typing {
            let who = self.who_types(chat);
            second.push(Span::styled(fmt::fit(&who, preview_w), Style::new().fg(t.online)));
        } else if let Some(draft) = chat.draft.as_ref().filter(|d| !d.is_empty() && !open) {
            second.push(Span::styled("draft: ", Style::new().fg(t.comment).add_modifier(Modifier::ITALIC)));
            second.push(Span::styled(
                fmt::fit(&fmt::one_line(draft), preview_w.saturating_sub(7)),
                Style::new().fg(t.text),
            ));
        } else if let Some(last) = &chat.last {
            let mut room = preview_w;
            if let Some(tick) = last.status {
                let tick = layout::ticks(tick, &t);
                room = room.saturating_sub(tick.width() + 1);
                second.push(tick);
                second.push(Span::raw(" "));
            }
            let said = match &last.from {
                Some(from) if chat.group() => format!("{from}: {}", fmt::one_line(&last.summary)),
                _ => fmt::one_line(&last.summary),
            };
            second.push(Span::styled(fmt::fit(&said, room), Style::new().fg(t.muted)));
        } else {
            second.push(Span::styled("no messages yet", Style::new().fg(t.comment)));
        }
        (first, Line::from(second))
    }

    fn conversation_title(&self) -> Line<'static> {
        let t = &self.theme;
        let Some(chat) = self.open.as_deref().and_then(|id| self.chat_known(id)) else { return Line::default() };
        let mut more = vec![];
        if !chat.typing.is_empty() {
            let who = format!("· {} ", self.who_types(chat));
            more.push(Span::styled(who, Style::new().fg(t.online)));
        }
        self.title(&chat.name, more)
    }

    fn draw_conversation(&mut self, f: &mut Frame, area: Rect) {
        let t = self.theme;
        let selecting = matches!(self.focus, Focus::Message(_)) && self.overlay.is_none();
        let block = self
            .panel(if selecting { t.focus } else { t.gutter })
            .title(self.conversation_title())
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);
        if self.open.is_none() {
            let lines = self.mimic_lines(inner, Look::default(), "No chats yet. They come as your phone sends them.");
            f.render_widget(centred(lines, inner), inner);
            return;
        }
        let box_h = (self.composer.height(inner.width.saturating_sub(4) as usize) + 2) as u16;
        let [messages, composer] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(box_h.min(inner.height))]).areas(inner);
        self.draw_messages(f, messages);
        self.draw_composer(f, composer);
    }

    fn draw_composer(&mut self, f: &mut Frame, area: Rect) {
        let t = self.theme;
        let typing = self.focus == Focus::Typing && self.overlay.is_none();
        let mut block = self.panel(if typing { t.focus } else { t.gutter }).padding(Padding::horizontal(1));
        if let Some(why) = &self.unsent {
            block =
                block.title_bottom(Line::from(Span::styled(format!(" not sent: {why} "), Style::new().fg(t.comment))));
        }
        let inner = block.inner(area);
        f.render_widget(block, area);
        self.areas.composer = inner;
        self.areas.composer_width = inner.width as usize;
        let (lines, (row, col)) = self.composer.view(inner.width as usize);
        let style = Style::new().fg(if typing { t.fg } else { t.text });
        let lines: Vec<Line> = lines.into_iter().map(|l| Line::from(Span::styled(l, style))).collect();
        f.render_widget(Paragraph::new(lines), inner);
        if typing && (row as u16) < inner.height {
            f.set_cursor_position((inner.x + col as u16, inner.y + row as u16));
        }
    }

    fn mimic_lines(&self, area: Rect, look: Look, said: &str) -> Vec<Line<'static>> {
        let t = &self.theme;
        let sprite = &self.mimic;
        let mut lines = vec![];
        if sprite.w <= area.width as usize && sprite.rows() + 2 <= area.height as usize {
            lines = sprite.lines(look, t);
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(said.to_string(), Style::new().fg(t.comment))));
        lines
    }

    fn draw_empty(&mut self, f: &mut Frame, area: Rect) {
        let name = self.open.as_deref().and_then(|id| self.chat_known(id)).map(|c| (c.name.clone(), c.group()));
        let said = match name {
            Some((name, true)) => format!("No messages in {name} yet. What you type goes to everyone in it."),
            Some((name, false)) => {
                let first = name.split_whitespace().next().unwrap_or(&name).to_string();
                format!("No messages with {first} yet. What you type goes to them.")
            }
            None => String::new(),
        };
        let shut = self.chomp_until.is_some() && (self.now() / 150) % 2 == 0;
        let lines = self.mimic_lines(area, Look { asleep: false, shut }, &said);
        f.render_widget(centred(lines, area), area);
    }

    fn day_line(&self, label: &str, w: usize) -> Line<'static> {
        let t = &self.theme;
        let label = format!("  {label}  ");
        let side = w.saturating_sub(fmt::width(&label));
        Line::from(vec![
            Span::styled("─".repeat(side / 2), Style::new().fg(t.gutter)),
            Span::styled(label, Style::new().fg(t.comment)),
            Span::styled("─".repeat(side - side / 2), Style::new().fg(t.gutter)),
        ])
    }

    /// Everything about the selected message, on a line under it.
    fn detail(&self, m: &Message, w: usize) -> Line<'static> {
        let t = &self.theme;
        let mut said = vec![fmt::full(m.at, self.tz)];
        if let Some(from) = m.from.as_ref().filter(|_| !m.mine) {
            said.insert(0, from.name.clone());
        }
        if let Some(status) = m.status {
            said.push(
                match status {
                    Tick::Pending => "waiting to send",
                    Tick::Sent => "sent",
                    Tick::Delivered => "delivered",
                    Tick::Read => "read",
                    Tick::Played => "played",
                    Tick::Failed => "not sent",
                }
                .into(),
            );
        }
        if m.edited {
            said.push("edited".into());
        }
        let lead = if m.mine { layout::indent(w) } else { layout::NAME };
        let text = fmt::fit(&said.join(" · "), w.saturating_sub(lead));
        Line::from(vec![Span::raw(" ".repeat(lead)), Span::styled(text, Style::new().fg(t.muted))])
    }

    fn placing(&mut self, w: &Messages<Message>, m: &Message, width: usize, top: i32) -> Placing {
        let prev = w.before(&m.sort);
        let shape = self.shape(m, prev, width);
        let new_day = prev.is_none_or(|p| !fmt::same_day(p.at, m.at, self.tz));
        let day = new_day.then(|| fmt::day(m.at, self.now(), self.tz));
        let blank = (new_day && prev.is_some()) || (!new_day && self.gap(m, prev));
        let lines = self.lines(m, shape);
        let detail = (self.focus == Focus::Message(m.id.clone())).then(|| self.detail(m, width));
        Placing { id: m.id.clone(), sort: m.sort.clone(), top, day, blank, lines, detail }
    }

    /// Fills the screen from the anchor up and down, keeps the newest at the bottom when it's in
    /// sight, and the oldest at the top when there's nothing before it.
    fn place(&mut self, w: &Messages<Message>, width: usize, h: i32) -> (VecDeque<Placing>, bool) {
        let (anchor, top) = match &self.scroll {
            Scroll::Bottom => (w.newest(), None),
            Scroll::Anchor { id, sort, top } => (w.get(id).or_else(|| w.nearest(sort)), Some(*top)),
        };
        let Some(anchor) = anchor else { return (VecDeque::new(), true) };
        let mut blocks = VecDeque::new();
        let first = self.placing(w, anchor, width, 0);
        let start = top.unwrap_or(h - first.height());
        blocks.push_back(Placing { top: start, ..first });
        let up = |v: &mut View, blocks: &mut VecDeque<Placing>| {
            while let Some(front) = blocks.front().filter(|b| b.top > 0) {
                let Some(prev) = w.before(&front.sort) else { break };
                let top = front.top;
                let p = v.placing(w, prev, width, 0);
                let height = p.height();
                blocks.push_front(Placing { top: top - height, ..p });
            }
        };
        let down = |v: &mut View, blocks: &mut VecDeque<Placing>| {
            while let Some(back) = blocks.back().filter(|b| b.bottom() < h) {
                let Some(next) = w.after(&back.sort) else { break };
                let top = back.bottom();
                let p = v.placing(w, next, width, top);
                blocks.push_back(p);
            }
        };
        let shift = |blocks: &mut VecDeque<Placing>, by: i32| blocks.iter_mut().for_each(|b| b.top += by);
        up(self, &mut blocks);
        down(self, &mut blocks);
        let newest = |blocks: &VecDeque<Placing>| blocks.back().is_some_and(|b| w.after(&b.sort).is_none());
        if newest(&blocks)
            && let Some(bottom) = blocks.back().map(Placing::bottom).filter(|b| *b < h)
        {
            shift(&mut blocks, h - bottom);
            up(self, &mut blocks);
        }
        let oldest = blocks.front().is_some_and(|b| w.before(&b.sort).is_none());
        if oldest
            && !w.older
            && !newest(&blocks)
            && let Some(top) = blocks.front().map(|b| b.top).filter(|t| *t > 0)
        {
            shift(&mut blocks, -top);
            down(self, &mut blocks);
            if newest(&blocks)
                && let Some(bottom) = blocks.back().map(Placing::bottom).filter(|b| *b < h)
            {
                shift(&mut blocks, h - bottom);
            }
        }
        let at_bottom = newest(&blocks) && blocks.back().is_some_and(|b| b.bottom() <= h);
        (blocks, at_bottom)
    }

    fn draw_messages(&mut self, f: &mut Frame, area: Rect) {
        self.areas.messages = area;
        let Some(window) = self.messages.take() else { return };
        if !window.loaded {
            self.messages = Some(window);
            return;
        }
        if window.is_empty() {
            self.messages = Some(window);
            self.draw_empty(f, area);
            return;
        }
        let (h, width) = (area.height as i32, area.width as usize);
        let (blocks, at_bottom) = self.place(&window, width, h);
        let t = self.theme;
        let buf = f.buffer_mut();
        let mut placed = HashMap::new();
        let mut newest_seen: Option<String> = None;
        for b in &blocks {
            if b.bottom() <= 0 || b.top >= h {
                continue;
            }
            let mut y = b.top;
            let mut put = |line: &Line, y: i32| {
                if (0..h).contains(&y) {
                    put(buf, area.x, area.y + y as u16, line, area.width);
                }
            };
            if b.blank {
                y += 1;
            }
            if let Some(day) = &b.day {
                put(&self.day_line(day, width), y);
                y += 1;
            }
            let body_top = y;
            for line in b.lines.lines.iter() {
                put(line, y);
                y += 1;
            }
            if let Some(detail) = &b.detail {
                put(detail, y);
                y += 1;
            }
            let (from, to) = (body_top.max(0), y.min(h));
            if b.detail.is_some() && to > from {
                buf.set_style(
                    Rect::new(area.x, area.y + from as u16, area.width, (to - from) as u16),
                    Style::new().bg(t.selection),
                );
            }
            if to > from {
                self.areas.message_rows.push((area.y + from as u16, area.y + to as u16, b.id.clone()));
            }
            let look = crate::graphics::Look {
                bg: if b.detail.is_some() { t.selection } else { t.bg },
                truecolor: t.truecolor,
                over: self.overlay.is_some() || self.palette.is_some(),
            };
            for p in &b.lines.pictures {
                let sources: Vec<std::path::PathBuf> = p.sources.iter().map(|s| self.images.path(s)).collect();
                let place = crate::graphics::Place {
                    x: area.x + p.col,
                    top: area.y as i32 + body_top + p.line as i32,
                    cols: p.cols,
                    rows: p.rows,
                };
                if !self.images.draw(buf, place, area, &sources, look) && self.images.gone(&sources, p.cols, p.rows) {
                    self.pictureless.insert(b.id.clone());
                    self.laid.remove(&b.id);
                    self.dirty = true;
                }
            }
            placed.insert(b.id.clone(), Placed { top: b.top, height: b.height() });
            if b.bottom() <= h {
                newest_seen = Some(b.sort.clone());
            }
        }
        self.placed = placed;
        if let Some(sort) = newest_seen
            && self.seen.as_ref().is_none_or(|s| *s < sort)
        {
            self.seen = Some(sort);
        }
        self.settle(&window, &blocks, at_bottom, h);
        let new = self.seen.as_deref().map_or(0, |s| window.count_after(s)) as u32 + window.arrived() as u32;
        if !at_bottom && new > 0 {
            let label = format!("─ {new} new ↓ ");
            let line = Line::from(vec![
                Span::styled(label.clone(), Style::new().fg(t.arrived).add_modifier(Modifier::BOLD)),
                Span::styled("─".repeat(width.saturating_sub(fmt::width(&label))), Style::new().fg(t.arrived)),
            ]);
            f.buffer_mut().set_line(area.x, area.y + area.height - 1, &line, area.width);
        }
        if at_bottom && let Some(newest) = window.newest().cloned() {
            self.seen_newest(&newest);
        }
        self.messages = Some(window);
    }

    /// After a frame: where the conversation is anchored now, and whether its window should move.
    fn settle(&mut self, w: &Messages<Message>, blocks: &VecDeque<Placing>, at_bottom: bool, h: i32) {
        let selected = match &self.focus {
            Focus::Message(id) => blocks.iter().find(|b| &b.id == id && b.top >= 0 && b.bottom() <= h),
            Focus::Typing => None,
        };
        self.scroll = if at_bottom {
            Scroll::Bottom
        } else if let Some(b) = selected.or_else(|| blocks.iter().find(|b| b.top >= 0)).or(blocks.front()) {
            Scroll::Anchor { id: b.id.clone(), sort: b.sort.clone(), top: b.top }
        } else {
            Scroll::Bottom
        };
        let Some(chat) = self.open.clone() else { return };
        let (Some(front), Some(back)) = (blocks.front(), blocks.back()) else { return };
        // At the bottom the window follows the newest; it moves for older ones once you've scrolled.
        if !at_bottom && w.older && w.count_before(&front.sort, super::EDGE) < super::EDGE {
            self.watch(&chat, View::around(&front.id));
        } else if at_bottom && !w.following() {
            self.watch(&chat, View::newest());
        } else if !w.following() && !w.holds_newest() && w.count_after(&back.sort) < super::EDGE {
            self.watch(&chat, View::around(&back.id));
        }
    }

    fn draw_starting(&mut self, f: &mut Frame, area: Rect, failed: Option<String>) {
        let t = self.theme;
        let block = self.panel(t.gutter);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let mut lines = vec![Line::from("Starting jawd, which keeps your messages and talks to WhatsApp.")];
        lines.push(match failed {
            Some(why) => Line::from(Span::styled(format!("It didn't start: {why}"), Style::new().fg(t.warn))),
            None => Line::from(Span::styled("If it can't start, this says why.", Style::new().fg(t.comment))),
        });
        f.render_widget(centred(lines, inner), inner);
    }

    fn draw_asleep(&mut self, f: &mut Frame, area: Rect) {
        let t = self.theme;
        let block = self.panel(t.gutter);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let mut lines = self.mimic_lines(inner, Look { asleep: true, shut: false }, "jawd isn't answering.");
        let drawn = lines.len() > 3;
        if let Some(first) = lines.first_mut().filter(|_| drawn) {
            first.spans.push(Span::styled("   z", Style::new().fg(t.comment)));
        }
        lines.push(Line::from(Span::styled(
            "jaw tries again by itself; jaw status says more.",
            Style::new().fg(t.comment),
        )));
        f.render_widget(centred(lines, inner), inner);
    }

    fn draw_linking(&mut self, f: &mut Frame, area: Rect) {
        let t = self.theme;
        let mut area = area;
        if self.connection.state == ConnectionState::LoggedOut {
            let h = 7.min(area.height);
            let [notice, rest] = Layout::vertical([Constraint::Length(h), Constraint::Min(0)]).areas(area);
            let block = self.panel(t.warn).title(Line::from(Span::styled(" logged out ", Style::new().fg(t.warn))));
            let lines = vec![
                Line::default(),
                Line::from("    Your phone logged jaw out. jawd has deleted the session and kept your messages."),
                Line::from("    Link again to carry on where you were, or delete the messages jaw keeps."),
                Line::default(),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled("⏎", Style::new().fg(t.fg).add_modifier(Modifier::BOLD)),
                    Span::styled(" link again   ", Style::new().fg(t.comment)),
                    Span::styled("q", Style::new().fg(t.fg).add_modifier(Modifier::BOLD)),
                    Span::styled(" quit", Style::new().fg(t.comment)),
                ]),
            ];
            f.render_widget(Paragraph::new(lines).block(block), notice);
            if !self.link_asked {
                f.buffer_mut().set_style(rest, Style::new().bg(t.bg));
                return;
            }
            area = rest;
        }
        if area.height < 3 {
            return;
        }
        let phone = self.phone.is_some() || matches!(self.link, Some(LinkStep::Code { .. }));
        let title = if phone { "link with a phone number" } else { "link jaw to WhatsApp" };
        let block = self.panel(t.gutter).title(self.title(title, vec![]));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let steps = "On the phone: WhatsApp › Settings › Linked devices › Link a device";
        if phone {
            let mut lines = vec![Line::default()];
            match (&self.phone, &self.link) {
                (Some(number), _) => {
                    lines.push(Line::from(vec![
                        Span::raw("    Your number, with its country code   "),
                        Span::styled(format!(" +{number}█ "), Style::new().fg(t.fg).bg(t.selection)),
                    ]));
                    lines.push(Line::default());
                    lines.push(Line::from(Span::styled(
                        "    ⏎ asks WhatsApp for a code to type on the phone.",
                        Style::new().fg(t.comment),
                    )));
                }
                (None, Some(LinkStep::Code { code })) => {
                    lines.push(Line::from(format!("    {steps} ›")));
                    lines.push(Line::from("    Link with phone number instead, then type"));
                    lines.push(Line::default());
                    lines.push(Line::from(Span::styled(
                        format!("        {code}"),
                        Style::new().fg(t.fg).add_modifier(Modifier::BOLD),
                    )));
                }
                _ => {}
            }
            f.render_widget(Paragraph::new(lines), inner);
            return;
        }
        let buf = f.buffer_mut();
        let qr = match &self.link {
            Some(LinkStep::Qr { code, .. }) => art::qr(code),
            _ => vec![],
        };
        let mimic = &self.mimic;
        let qr_w = qr.first().map_or(0, |r| r.chars().count());
        let x = inner.x + 4;
        let fits = qr.len() < inner.height as usize && qr_w + 4 <= inner.width as usize;
        let mut y = inner.y + 1;
        if fits {
            for (i, row) in qr.iter().enumerate() {
                let line = Line::from(Span::styled(row.clone(), Style::new().fg(t.fg).bg(t.bg)));
                buf.set_line(x, inner.y + 1 + i as u16, &line, qr_w as u16);
            }
        }
        let right_x = x + if fits { qr_w as u16 + 4 } else { 0 };
        let right_w = (inner.x + inner.width).saturating_sub(right_x) as usize;
        let mut text: Vec<Line<'static>> = vec![];
        if mimic.w <= right_w && mimic.rows() + 6 <= inner.height as usize {
            text.extend(mimic.lines(Look::default(), &t));
            text.push(Line::default());
        }
        let now = self.now();
        match &self.link {
            Some(LinkStep::Qr { expires, .. }) if fits => {
                for l in
                    layout::wrap(&[(format!("{steps}, then point it at this code."), Style::new())], right_w, right_w)
                {
                    text.push(Line::from(l.into_iter().map(|(s, _)| Span::raw(s)).collect::<Vec<_>>()));
                }
                let s = ((expires - now).max(0) + 999) / 1000;
                text.push(Line::from(Span::styled(format!("It changes in {s} s."), Style::new().fg(t.comment))));
            }
            Some(LinkStep::Qr { .. }) => {
                text.push(Line::from("The QR code needs a bigger window;"));
                text.push(Line::from("p links with a code typed on the phone instead."));
            }
            Some(LinkStep::Failed { reason }) => {
                text.push(Line::from(Span::styled(format!("Linking failed: {reason}"), Style::new().fg(t.warn))));
            }
            _ => text.push(Line::from(Span::styled("Asking WhatsApp for a code…", Style::new().fg(t.comment)))),
        }
        text.push(Line::default());
        text.push(Line::from(vec![
            Span::styled("p", Style::new().fg(t.fg).add_modifier(Modifier::BOLD)),
            Span::styled(" link with a phone number instead   ", Style::new().fg(t.comment)),
            Span::styled("q", Style::new().fg(t.fg).add_modifier(Modifier::BOLD)),
            Span::styled(" quit", Style::new().fg(t.comment)),
        ]));
        for line in text {
            if y >= inner.y + inner.height {
                break;
            }
            buf.set_line(right_x, y, &line, right_w as u16);
            y += 1;
        }
    }

    fn draw_keys(&self, f: &mut Frame) {
        let t = &self.theme;
        let col = |key: &str, what: &str, on: bool| -> Vec<Span<'static>> {
            let (k, w) = if on {
                (Style::new().fg(t.focus).add_modifier(Modifier::BOLD), Style::new().fg(t.text))
            } else {
                (Style::new().fg(t.comment), Style::new().fg(t.comment))
            };
            vec![Span::styled(format!("{key:<11}"), k), Span::styled(format!("{what:<28}"), w)]
        };
        let head = |a: &str, b: &str| {
            Line::from(vec![
                Span::styled(format!("{a:<39}"), Style::new().fg(t.muted)),
                Span::styled(b.to_string(), Style::new().fg(t.muted)),
            ])
        };
        let pair = |a: (&str, &str, bool), b: Option<(&str, &str, bool)>| {
            let mut spans = col(a.0, a.1, a.2);
            if let Some(b) = b {
                spans.extend(col(b.0, b.1, b.2));
            }
            Line::from(spans)
        };
        let lines = vec![
            head("typing", "a selected message"),
            pair(("⏎", "send", true), Some(("↑ ↓", "the one before, after", true))),
            pair(("alt-⏎", "a new line", true), Some(("home end", "the oldest, the newest", true))),
            pair(("↑", "into the messages", true), Some(("y", "copy its text", true))),
            pair(("pgup pgdn", "scroll the messages", true), Some(("⏎", "open a folded one", true))),
            pair(("ctrl-a e", "start, end of the line", true), Some(("r", "reply · M2", false))),
            pair(("alt-b f", "back, on a word", true), Some(("e", "edit · M2", false))),
            pair(("ctrl-w u", "cut a word, the line", true), Some(("d", "delete · M2", false))),
            pair(("alt-d", "cut the next word", true), Some(("+  1-6", "react · M2", false))),
            pair(("ctrl-y", "put back what was cut", true), Some(("f", "forward · M2", false))),
            pair(("@  :", "names and emoji · M2", false), Some(("i", "info · M2", false))),
            pair(("ctrl-o", "attach · M4", false), Some(("s", "save · M3", false))),
            pair(("ctrl-r", "record · M5", false), Some(("esc", "back to typing", true))),
            Line::default(),
            head("moving", "anywhere"),
            pair(("ctrl-k", "the palette: anything", true), Some(("?", "these keys", true))),
            pair(("alt-↑ ↓", "the chat before, after", true), Some(("/search", "words in this chat", true))),
            pair(("esc", "the chat list, when narrow", true), Some(("the mouse", "click, scroll, open", true))),
            pair(("ctrl-b", "the sidebar · M6", false), Some(("ctrl-c", "quit", true))),
        ];
        let w = 84.min(f.area().width.saturating_sub(2));
        let h = (lines.len() as u16 + 2).min(f.area().height.saturating_sub(2));
        let area = Rect::new((f.area().width - w) / 2, (f.area().height - h) / 2, w, h);
        f.render_widget(Clear, area);
        let block = self
            .panel(t.focus)
            .style(Style::new().bg(t.surface).fg(t.fg))
            .title(Span::styled(" keys ", Style::new().fg(t.focus).add_modifier(Modifier::BOLD)))
            .title_bottom(Line::from(Span::styled(" any key closes ", Style::new().fg(t.muted))))
            .padding(Padding::horizontal(2));
        f.render_widget(Paragraph::new(lines).block(block), area);
    }
}
