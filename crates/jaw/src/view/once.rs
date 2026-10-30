//! A view-once message opened, as the protocol's "Disappearing and view-once messages" has it:
//! Enter on someone else's asks jawd to open it, which downloads its file and answers with it; the
//! photo then shows over the screen with its caption, and how long until it goes, and Esc closes
//! it, after which jawd erases it, as it does once jaw goes or at its time. A video or a voice
//! message plays in the app the system has for it. Closed while it opens, it's closed as the answer
//! comes, so it's seen at most once.

use super::draw::put;
use super::{Effect, View};
use crate::data::{Kind, Message, Opened};
use crate::fmt;
use crate::graphics::{Look, Place, Protocol};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding};

/// The view's widest.
const WIDTH: u16 = 100;
/// The most lines the caption takes under the picture.
const CAPTION: usize = 3;

/// The message open, or being opened.
pub struct Once {
    pub chat: String,
    pub message: String,
    /// Who sent it and what it is, as its row says, for the title.
    from: String,
    kind: Kind,
    /// jawd's answer once it comes: the file to show and when jawd erases it whatever happens.
    opened: Option<Box<Opened>>,
}

impl Once {
    /// Message `m` of `chat`'s, being opened.
    pub(super) fn opening(chat: &str, m: &Message) -> Once {
        let from = m.from.as_ref().map_or_else(String::new, |f| f.name.clone());
        Once { chat: chat.to_string(), message: m.id.clone(), from, kind: m.kind, opened: None }
    }
}

#[cfg(test)]
impl Once {
    pub(super) fn opened_for_tests(self) -> Option<Box<Opened>> {
        self.opened
    }
}

/// What a view-once message of `kind` is, as its title says it.
fn what(kind: Kind) -> &'static str {
    match kind {
        Kind::Image => "photo",
        Kind::Video | Kind::Gif | Kind::VideoNote => "video",
        Kind::Voice | Kind::Audio => "voice message",
        _ => "message",
    }
}

impl View {
    /// Enter on a view-once message: someone else's opens, once; yours can't be, as on the phones.
    pub(super) fn open_once(&mut self, m: &Message) {
        if m.mine {
            self.say("a view-once message you sent can't be opened here, as on your phone");
            return;
        }
        if m.deleted {
            self.say("that message was deleted, so there's nothing to open");
            return;
        }
        let Some(chat) = self.open.clone() else { return };
        self.once = Some(Once::opening(&chat, m));
        self.jawd.view_once_open(&chat, &m.id);
    }

    /// jawd's answer to opening one: shown, or said why not. One closed while it opened, or another
    /// since, is closed now, so jawd erases it.
    pub(super) fn on_opened(&mut self, chat: String, message: String, opened: Result<Box<Opened>, String>) {
        let waiting = self.once.as_mut().filter(|o| o.message == message && o.opened.is_none());
        match (waiting, opened) {
            (Some(once), Ok(opened)) => once.opened = Some(opened),
            (Some(_), Err(why)) => {
                self.once = None;
                self.say(why);
            }
            (None, Ok(_)) => self.jawd.view_once_close(&chat, &message),
            (None, Err(_)) => {}
        }
    }

    /// Closes the view: jawd erases the message once it has it open.
    pub(super) fn close_once(&mut self) {
        let Some(once) = self.once.take() else { return };
        if once.opened.is_some() {
            self.jawd.view_once_close(&once.chat, &once.message);
        }
        self.dirty = true;
    }

    /// The view's time has come: jawd erases it now whatever the view does, so it closes.
    pub(super) fn once_due(&mut self, now: i64) {
        if self.once.as_ref().and_then(|o| o.opened.as_ref()).is_some_and(|o| o.until <= now) {
            self.close_once();
        }
    }

    /// When the countdown next moves, or the view's time comes.
    pub(super) fn once_wake(&self) -> Option<i64> {
        let until = self.once.as_ref()?.opened.as_ref()?.until;
        let now = self.now();
        Some(((now / 1_000 + 1) * 1_000).min(until))
    }

    pub(super) fn on_once_key(&mut self, k: KeyEvent) {
        let Some(once) = self.once.as_ref() else { return };
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => self.close_once(),
            KeyCode::Enter if once.kind != Kind::Image => {
                if let Some(opened) = &once.opened {
                    let path = self.images.path(&opened.path).display().to_string();
                    self.effects.push(Effect::Open(path));
                    self.say(format!("playing the {} in the app your Mac has for it", what(once.kind)));
                }
            }
            _ => {}
        }
    }

    pub(super) fn once_hints(&self) -> Vec<(&'static str, &'static str, bool)> {
        let mut hints = vec![("esc", "closes it, and it's gone", true)];
        if let Some(once) = self.once.as_ref().filter(|o| o.kind != Kind::Image) {
            hints.insert(0, ("⏎", "plays it", once.opened.is_some()));
        }
        hints
    }

    /// The view over the screen, the screen behind it quieter: the picture as large as fits, its
    /// caption under it, and how long until it goes.
    pub(super) fn draw_once(&mut self, f: &mut Frame) {
        let t = self.theme;
        let screen = f.area();
        let Some(once) = self.once.as_ref() else { return };
        let (kind, from, opened) = (once.kind, once.from.clone(), once.opened.clone());
        let w = WIDTH.min(screen.width.saturating_sub(4));
        let h = screen.height.saturating_sub(4);
        if w < 30 || h < 8 {
            return;
        }
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
        let from = if from.is_empty() { String::new() } else { format!(" from {from}") };
        let mut block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(t.focus))
            .style(Style::new().bg(t.surface).fg(t.fg))
            .title(Line::from(vec![
                Span::styled(" ① view once ", Style::new().fg(t.text).add_modifier(Modifier::BOLD)),
                Span::styled(format!("· {}{from} ", what(kind)), Style::new().fg(t.comment)),
            ]))
            .title_bottom(Line::from(Span::styled(" esc closes it, and it's gone ", Style::new().fg(t.muted))))
            .padding(Padding::horizontal(1));
        if let Some(opened) = &opened {
            let left = ((opened.until - self.now()) as f64 / 1_000.0).ceil();
            let said = format!(" goes in {} ", fmt::length(left));
            block = block.title(Line::from(Span::styled(said, Style::new().fg(t.warn))).right_aligned());
        }
        let inner = block.inner(area);
        f.render_widget(block, area);
        let Some(opened) = opened else {
            let line = Line::from(Span::styled(
                format!("opening the {}… jawd is fetching it from WhatsApp", what(kind)),
                Style::new().fg(t.comment),
            ))
            .centered();
            put(f.buffer_mut(), inner.x, inner.y + inner.height / 2, &line, inner.width);
            return;
        };
        let width = inner.width as usize;
        let caption = crate::layout::wrap(&crate::layout::styled(&opened.spans, &t, &self.me), width, width);
        let caption: Vec<Line<'static>> = caption
            .into_iter()
            .take(CAPTION)
            .map(|line| Line::from(line.into_iter().map(|(s, st)| Span::styled(s, st)).collect::<Vec<_>>()).centered())
            .collect();
        let room = inner.height.saturating_sub(caption.len() as u16 + u16::from(!caption.is_empty()));
        let picture = Rect::new(inner.x, inner.y, inner.width, room);
        let drawn = self.draw_once_picture(f, &opened, picture);
        if !drawn {
            let said = match kind {
                Kind::Image => "▣ the photo comes once it's drawn".to_string(),
                kind => format!("▶ the {} · ⏎ plays it", what(kind)),
            };
            let line = Line::from(Span::styled(said, Style::new().fg(t.comment))).centered();
            put(f.buffer_mut(), picture.x, picture.y + picture.height / 2, &line, picture.width);
        }
        for (i, line) in caption.iter().enumerate() {
            let y = inner.y + room + 1 + i as u16;
            if y < inner.y + inner.height {
                put(f.buffer_mut(), inner.x, y, line, inner.width);
            }
        }
    }

    /// The opened file's picture in `area`, as large as fits, from its previews or the photo itself;
    /// says whether it was drawn.
    fn draw_once_picture(&mut self, f: &mut Frame, opened: &Opened, area: Rect) -> bool {
        let t = self.theme;
        if self.images.protocol == Protocol::None || self.no_color || area.height == 0 {
            return false;
        }
        let Some(media) = opened.media.as_deref() else { return false };
        let mut sources: Vec<std::path::PathBuf> =
            [&media.previews.large, &media.previews.small].into_iter().flatten().map(|p| self.images.path(p)).collect();
        if opened.kind == Kind::Image {
            sources.push(self.images.path(&opened.path));
        }
        if sources.is_empty() || media.w == 0 || media.h == 0 {
            return false;
        }
        let (cols, rows) = crate::graphics::fit(media.w, media.h, area.width, area.height, self.images.cell);
        let place = Place { x: area.x + (area.width - cols) / 2, top: area.y as i32, cols, rows };
        let look = Look { bg: t.surface, truecolor: t.truecolor, over: false };
        self.images.draw(f.buffer_mut(), place, area, &sources, look)
    }
}
