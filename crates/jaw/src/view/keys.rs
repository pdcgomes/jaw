//! Keys and the mouse, without modes: typing goes to the message box, Up from an empty box steps
//! into the messages, where single keys act on the selected one, and Esc steps back. Keys for
//! what jawd can't do yet say in the footer when it comes.

use super::{Effect, Focus, HOLD, Overlay, Scroll, View};
use crate::data::{Kind, Message};
use chrono::FixedOffset;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

/// Two clicks on a message this close together open it.
const DOUBLE_CLICK: i64 = 400;

/// What Enter does on a message of this kind, and when it comes.
fn opens(kind: Kind) -> &'static str {
    match kind {
        Kind::Image | Kind::Video | Kind::Gif | Kind::VideoNote | Kind::Document | Kind::Sticker => {
            "opening photos, videos and files comes with M3"
        }
        Kind::Audio => "playing audio files comes later",
        Kind::Location => "its map link opens from your terminal",
        Kind::Event => "its links open from your terminal",
        Kind::LiveLocation | Kind::Poll => "opening these comes with M2",
        Kind::Contact => "there's no number on this card to start a chat with",
        _ => "there's nothing to open in this one",
    }
}

/// A message's text as it would be pasted: a contact card's lines as they're drawn.
fn plain(m: &Message, tz: FixedOffset, t: &crate::theme::Theme) -> String {
    if let Some(contact) = m.contact.as_ref().filter(|_| m.kind == Kind::Contact && !m.deleted) {
        return crate::layout::card(contact, t).into_iter().map(|(s, _)| s).collect();
    }
    let text: String = m.spans.iter().map(|s| s.s.as_str()).collect();
    if m.kind == Kind::Text {
        return text;
    }
    if let Some(place) = &m.location {
        let place = place.lines().join("\n");
        return if text.is_empty() { place } else { format!("{place}\n{text}") };
    }
    if let Some(event) = &m.event {
        let lines: Vec<String> =
            event.lines(tz).into_iter().map(|parts| parts.into_iter().map(|(said, _)| said).collect()).collect();
        let event = lines.join("\n");
        return if text.is_empty() { event } else { format!("{event}\n{text}") };
    }
    let summary = crate::layout::summary(m);
    if text.is_empty() { summary } else { format!("{summary}\n{text}") }
}

fn inside(r: Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

impl View {
    pub fn on_input(&mut self, e: Event) {
        match e {
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                self.dirty = true;
                self.on_key(k);
            }
            Event::Mouse(m) => self.on_mouse(m),
            Event::Paste(s) => {
                self.dirty = true;
                self.on_paste(&s);
            }
            Event::FocusGained => self.jawd.focus(true),
            Event::FocusLost => self.jawd.focus(false),
            Event::Resize(..) => self.dirty = true,
            _ => {}
        }
    }

    pub fn on_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        if ctrl && k.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.overlay.take().is_some() {
            // Any key closes the keys; one that types goes into the box, so a "?" isn't lost.
            if let KeyCode::Char(c) = k.code
                && !ctrl
                && !alt
                && self.focus == Focus::Typing
                && self.reach == super::Reach::Up
                && !self.linking()
                && !(self.narrow() && self.picking)
            {
                self.composer.insert(c.encode_utf8(&mut [0; 4]));
                self.typed();
            }
            return;
        }
        if self.palette.is_some() {
            self.on_palette_key(k);
            return;
        }
        if self.settings_panel.is_some() {
            self.on_settings_key(k);
            return;
        }
        if self.quick_look.is_some() {
            self.on_quick_look_key(k);
            return;
        }
        if self.panes.prefix != super::panes::Prefix::Off && self.on_pane_key(k) {
            return;
        }
        if self.recorder.is_some() && self.reach == super::Reach::Up {
            self.on_recorder_key(k, ctrl);
            return;
        }
        if ctrl && k.code == KeyCode::Char('k') && self.reach == super::Reach::Up && !self.linking() {
            self.palette_open();
            return;
        }
        if ctrl && k.code == KeyCode::Char(',') && self.reach == super::Reach::Up && !self.linking() {
            self.open_settings(false);
            return;
        }
        if k.code == KeyCode::F(1) {
            self.overlay = Some(Overlay::Keys);
            return;
        }
        if self.reach != super::Reach::Up {
            return;
        }
        if self.linking() {
            self.on_linking_key(k);
            return;
        }
        if self.on_experiment_key(k) {
            return;
        }
        if ctrl && k.code == KeyCode::Char('b') {
            self.toggle_sidebar();
            return;
        }
        if self.library.is_some() {
            self.on_library_key(k);
            return;
        }
        if ctrl && k.code == KeyCode::Char('l') {
            self.library_key();
            return;
        }
        if alt && k.modifiers.contains(KeyModifiers::SHIFT) && matches!(k.code, KeyCode::Up | KeyCode::Down) {
            self.step_folder(if k.code == KeyCode::Up { -1 } else { 1 });
            return;
        }
        if alt && matches!(k.code, KeyCode::Up | KeyCode::Down) {
            self.step_chat(if k.code == KeyCode::Up { -1 } else { 1 });
            return;
        }
        if let KeyCode::Char(digit @ '1'..='9') = k.code
            && alt
        {
            self.go_favourite(digit as u8 - b'0');
            return;
        }
        if self.sidebar.keys.is_some() {
            self.on_sidebar_key(k);
            return;
        }
        if self.narrow() && self.picking {
            self.on_list_key(k);
            return;
        }
        match self.focus.clone() {
            Focus::Typing => self.on_typing_key(k, ctrl, alt),
            Focus::Message(id) => self.on_message_key(k, &id, ctrl, alt),
        }
    }

    fn on_typing_key(&mut self, k: KeyEvent, ctrl: bool, alt: bool) {
        if ctrl && k.code == KeyCode::Char('r') {
            self.record();
            return;
        }
        if !ctrl && !alt && self.on_naming_key(k) {
            return;
        }
        let before = self.composer.text().to_string();
        let c = &mut self.composer;
        match k.code {
            KeyCode::Enter if alt || k.modifiers.contains(KeyModifiers::SHIFT) => c.insert("\n"),
            KeyCode::Enter => self.send(),
            KeyCode::Char('j') if ctrl => c.insert("\n"),
            KeyCode::Char(ch) if ctrl => match ch {
                'a' => c.home(),
                'e' => c.end(),
                'u' => c.kill_line_back(),
                'w' => c.kill_to_space(),
                'y' => c.yank(),
                'd' => c.delete(),
                'h' => c.backspace(),
                _ => {}
            },
            KeyCode::Char(ch) if alt => match ch {
                'b' => c.word_left(),
                'f' => c.word_right(),
                'd' => c.kill_word_forward(),
                _ => {}
            },
            KeyCode::Char('?') if c.is_empty() => self.overlay = Some(Overlay::Keys),
            KeyCode::Char(ch) => c.insert(ch.encode_utf8(&mut [0; 4])),
            KeyCode::Backspace if alt || ctrl => c.kill_word_back(),
            KeyCode::Backspace => c.backspace(),
            KeyCode::Delete => c.delete(),
            KeyCode::Left if alt || ctrl => c.word_left(),
            KeyCode::Right if alt || ctrl => c.word_right(),
            KeyCode::Left => c.left(),
            KeyCode::Right => c.right(),
            KeyCode::Home => c.home(),
            KeyCode::End => c.end(),
            KeyCode::Up if c.is_empty() => self.step_into_messages(),
            KeyCode::Up => {
                let w = self.areas.composer_width;
                self.composer.vertical(-1, w);
            }
            KeyCode::Down => {
                let w = self.areas.composer_width;
                self.composer.vertical(1, w);
            }
            KeyCode::PageUp => self.scroll_rows(self.page()),
            KeyCode::PageDown => self.scroll_rows(-self.page()),
            KeyCode::Esc if self.editing.is_some() => self.editing_stop(),
            KeyCode::Esc if self.replying.is_some() => self.replying = None,
            KeyCode::Esc if self.narrow() => {
                self.picking = true;
                self.selected = self.open.clone();
            }
            _ => {}
        }
        if self.composer.text() != before {
            self.typed();
        }
    }

    fn page(&self) -> i32 {
        (self.areas.messages.height as i32 - 2).max(1)
    }

    fn step_into_messages(&mut self) {
        let Some(newest) = self.messages.as_ref().and_then(|w| w.newest()).cloned() else { return };
        self.select(&newest);
    }

    /// Moving up past the oldest message jaw has in the chat asks the phone for older ones, once
    /// for each oldest message; they come into the window as the phone sends them.
    fn ask_for_older(&mut self) {
        let Some(w) = self.messages.as_ref().filter(|w| w.loaded && !w.older) else { return };
        let (Some(chat), Some(oldest)) = (self.open.clone(), w.oldest().map(|m| m.id.clone())) else { return };
        if self.asked_older.as_ref().is_some_and(|(c, o)| *c == chat && *o == oldest) {
            return;
        }
        self.asked_older = Some((chat.clone(), oldest));
        self.jawd.history_more(&chat);
        self.say("asking your phone for older messages");
    }

    /// Selects a message, moving the conversation only as far as it takes to show it whole.
    fn select(&mut self, m: &Message) {
        self.focus = Focus::Message(m.id.clone());
        let h = self.areas.messages.height as i32;
        let anchor = |top: i32| Scroll::Anchor { id: m.id.clone(), sort: m.sort.clone(), top };
        match self.placed.get(&m.id).copied() {
            Some(p) if p.top < 0 => self.scroll = anchor(0),
            Some(p) if p.top + p.height + 1 > h => self.scroll = anchor((h - p.height - 1).max(0)),
            Some(_) => {}
            None => {
                let above = self.placed.values().map(|p| p.top).min().unwrap_or(0) >= 0;
                let newest = self.messages.as_ref().and_then(|w| w.newest()).is_some_and(|n| n.id == m.id);
                self.scroll = if newest {
                    Scroll::Bottom
                } else if above {
                    anchor(0)
                } else {
                    anchor(h / 2)
                };
            }
        }
    }

    pub(super) fn on_message_key(&mut self, k: KeyEvent, id: &str, ctrl: bool, alt: bool) {
        let Some(m) = self.messages.as_ref().and_then(|w| w.get(id)).cloned() else {
            self.focus = Focus::Typing;
            return;
        };
        let step = |v: &View, by: i32| -> Option<Message> {
            let w = v.messages.as_ref()?;
            let mut at = m.clone();
            for _ in 0..by.unsigned_abs() {
                let next = if by < 0 { w.before(&at.sort) } else { w.after(&at.sort) };
                match next {
                    Some(n) => at = n.clone(),
                    None => break,
                }
            }
            Some(at)
        };
        if m.kind == Kind::Voice && !ctrl && !alt {
            match k.code {
                KeyCode::Enter => return self.toggle_voice(&m),
                KeyCode::Left if self.seek_voice(&m, -1.0) => return,
                KeyCode::Right if self.seek_voice(&m, 1.0) => return,
                KeyCode::Char('x') => return self.next_speed(&m),
                _ => {}
            }
        }
        match k.code {
            KeyCode::Char('r') if ctrl => {
                self.focus = Focus::Typing;
                self.record();
            }
            KeyCode::Esc => self.focus = Focus::Typing,
            KeyCode::Up | KeyCode::Char('k') if !ctrl && !alt => {
                if let Some(prev) = step(self, -1) {
                    if prev.id == m.id {
                        self.ask_for_older();
                    }
                    self.select(&prev);
                }
            }
            KeyCode::Down | KeyCode::Char('j') if !ctrl && !alt => {
                let newest = self.messages.as_ref().and_then(|w| w.newest()).is_some_and(|n| n.id == m.id);
                match step(self, 1) {
                    Some(next) if !newest => self.select(&next),
                    _ => self.focus = Focus::Typing,
                }
            }
            KeyCode::PageUp => {
                if let Some(prev) = step(self, -10) {
                    if prev.id == m.id {
                        self.ask_for_older();
                    }
                    self.select(&prev);
                }
            }
            KeyCode::PageDown => {
                if let Some(next) = step(self, 10) {
                    self.select(&next);
                }
            }
            KeyCode::Home => {
                if let Some(oldest) = self.messages.as_ref().and_then(|w| w.oldest()).cloned() {
                    self.select(&oldest);
                }
            }
            KeyCode::End => self.step_into_messages(),
            KeyCode::Enter if m.mine && m.status == Some(crate::data::Tick::Failed) => {
                self.jawd.retry(&m.id);
                self.say("sending it again");
            }
            KeyCode::Enter => {
                let folded = !self.unfolded.contains(&m.id)
                    && self.laid.get(&m.id).is_some_and(|l| l.lines.lines.len() > crate::layout::FOLD);
                if folded {
                    self.unfolded.insert(m.id.clone());
                } else if !self.chat_from_card(&m) && !self.open_in(&m) {
                    self.say(opens(m.kind));
                }
            }
            KeyCode::Char('y') => {
                self.effects.push(Effect::Copy(plain(&m, self.tz, &self.theme)));
                self.say("copied");
            }
            KeyCode::Char('r') => self.reply_to(&m),
            KeyCode::Char('e') => self.edit(&m),
            KeyCode::Char('d') => self.palette_delete(&m),
            KeyCode::Char('+') => self.palette_react(&m),
            KeyCode::Char(c @ '1'..='6') => self.react_quick(&m, c as usize - '1' as usize),
            KeyCode::Char('f') => self.palette_forward(&m),
            KeyCode::Char('i') => self.info(&m),
            KeyCode::Char('s') => self.say("saving files comes with M3"),
            KeyCode::Char('m') => self.file_this(),
            KeyCode::Char('?') => self.overlay = Some(Overlay::Keys),
            KeyCode::Char(_) | KeyCode::Backspace if !ctrl && !alt => {
                self.focus = Focus::Typing;
                self.on_typing_key(k, ctrl, alt);
            }
            _ => {}
        }
    }

    fn on_list_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-(self.list_rows.max(1) as isize)),
            KeyCode::PageDown => self.move_selection(self.list_rows.max(1) as isize),
            KeyCode::Home => self.move_selection(isize::MIN / 2),
            KeyCode::End => self.move_selection(isize::MAX / 2),
            KeyCode::Enter | KeyCode::Right => {
                if let Some(id) = self.selected.clone()
                    && !self.toggle_community(&id)
                {
                    self.open_chat(&id);
                    self.jawd.opened(&id);
                }
            }
            KeyCode::Char('m') => self.file_this(),
            KeyCode::Char('?') => self.overlay = Some(Overlay::Keys),
            KeyCode::Char('q') => self.quit = true,
            _ => {}
        }
    }

    /// The list's selection `by` rows on; with none of its chats selected, as after a folder that
    /// hasn't the open chat is picked, down lands on the first.
    pub(super) fn move_selection(&mut self, by: isize) {
        let order = self.list_order();
        if order.is_empty() {
            return;
        }
        let i = self.selected_index().map_or(if by > 0 { -1 } else { 0 }, |i| i as isize);
        let next = (i.saturating_add(by)).clamp(0, order.len() as isize - 1) as usize;
        self.selected = Some(order[next].clone());
        self.reveal_selected_chat();
    }

    /// Alt-Up and Alt-Down: the chat before or after, the list's order held while moving.
    fn step_chat(&mut self, by: isize) {
        self.chats.hold();
        self.hold_until = Some(self.now() + HOLD);
        self.move_selection(by);
        self.step_past_panes(by);
        if !(self.narrow() && self.picking)
            && let Some(id) = self.selected.clone()
            && !self.toggle_community(&id)
        {
            self.open_chat(&id);
        }
    }

    fn on_linking_key(&mut self, k: KeyEvent) {
        if let Some(number) = self.phone.as_mut() {
            match k.code {
                KeyCode::Char(c) if c.is_ascii_digit() => number.push(c),
                KeyCode::Backspace => {
                    number.pop();
                }
                KeyCode::Enter if number.len() >= 7 => {
                    let number = self.phone.take();
                    self.link = None;
                    self.jawd.link_start(number);
                }
                KeyCode::Enter => self.say("the number with its country code, such as 351912345678"),
                KeyCode::Esc => {
                    self.phone = None;
                    self.link = None;
                    self.jawd.link_start(None);
                }
                _ => {}
            }
            return;
        }
        let logged_out = self.logged_out.is_some() && !self.link_asked;
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Enter if logged_out => self.start_linking(),
            KeyCode::Char('x') if logged_out => self.say("deleting the messages jaw keeps comes later"),
            KeyCode::Char('p') if !logged_out => self.phone = Some(String::new()),
            KeyCode::Esc if matches!(self.link, Some(jaw_proto::LinkStep::Code { .. })) => {
                self.link = None;
                self.jawd.link_start(None);
            }
            _ => {}
        }
    }

    fn on_paste(&mut self, s: &str) {
        if self.palette.is_some() {
            self.palette_paste(s);
            return;
        }
        // The box gives up its cursor while the settings panel has the keys.
        if self.settings_panel.is_some() {
            return;
        }
        if let Some(number) = self.phone.as_mut() {
            number.extend(s.chars().filter(char::is_ascii_digit));
            return;
        }
        if self.reach != super::Reach::Up || self.linking() || (self.narrow() && self.picking) {
            return;
        }
        self.focus = Focus::Typing;
        self.composer.insert(s);
        self.typed();
    }

    /// Scrolls the conversation `by` rows, up for positive, anchored on what's at the top.
    fn scroll_rows(&mut self, by: i32) {
        let Some(w) = self.messages.as_ref() else { return };
        // Scrolling up with the oldest message's first line already on screen goes past it.
        if by > 0 && w.oldest().and_then(|m| self.placed.get(&m.id)).is_some_and(|p| p.top >= 0) {
            self.ask_for_older();
        }
        let Some(w) = self.messages.as_ref() else { return };
        let (id, top) = match &self.scroll {
            Scroll::Anchor { id, top, .. } => (id.clone(), *top),
            Scroll::Bottom if by > 0 => {
                let Some((id, p)) = self.placed.iter().min_by_key(|(_, p)| p.top) else { return };
                (id.clone(), p.top)
            }
            Scroll::Bottom => return,
        };
        let Some(sort) = w.get(&id).map(|m| m.sort.clone()) else { return };
        self.scroll = Scroll::Anchor { id, sort, top: top + by };
        self.dirty = true;
    }

    pub fn on_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column, m.row);
        if self.palette.is_some() {
            match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    self.dirty = true;
                    self.palette_click(y, Rect::new(0, 0, self.areas.width, self.areas.height));
                }
                MouseEventKind::ScrollUp => self.palette_move(-1),
                MouseEventKind::ScrollDown => self.palette_move(1),
                _ => return,
            }
            self.dirty = true;
            return;
        }
        if self.quick_look.is_some() {
            if matches!(m.kind, MouseEventKind::Down(_)) {
                self.close_quick_look();
            }
            return;
        }
        // The settings panel takes no clicks, and while it has the keys neither does the chat list
        // it covers nor the sidebar or the conversation beside it.
        if self.settings_panel.is_some() {
            return;
        }
        if inside(self.areas.sidebar, x, y) {
            self.on_sidebar_mouse(m);
            return;
        }
        if self.pane_click(&m) {
            return;
        }
        match m.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let up = m.kind == MouseEventKind::ScrollUp;
                if inside(self.areas.messages, x, y) {
                    self.scroll_rows(if up { 3 } else { -3 });
                } else if inside(self.areas.list, x, y) {
                    self.list_top = if up { self.list_top.saturating_sub(1) } else { self.list_top + 1 };
                    self.clamp_list();
                    self.dirty = true;
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                self.dirty = true;
                self.overlay = None;
                if inside(self.areas.list, x, y) {
                    let hit = self
                        .areas
                        .chat_rows
                        .iter()
                        .find(|(top, _)| y == *top || y == top + 1)
                        .map(|(_, id)| id.clone());
                    if let Some(id) = hit {
                        self.selected = Some(id.clone());
                        self.open_chat(&id);
                        self.jawd.opened(&id);
                    }
                } else if inside(self.areas.composer, x, y) {
                    self.focus = Focus::Typing;
                    let r = self.areas.composer;
                    self.composer.click((y - r.y) as usize, (x - r.x) as usize, r.width as usize);
                } else if inside(self.areas.messages, x, y) {
                    let hit =
                        self.areas.message_rows.iter().find(|(a, b, _)| y >= *a && y < *b).map(|(_, _, id)| id.clone());
                    let Some(id) = hit else { return };
                    let now = self.now();
                    let again =
                        self.last_click.as_ref().is_some_and(|(at, last)| *last == id && now - at < DOUBLE_CLICK);
                    self.last_click = Some((now, id.clone()));
                    let Some(msg) = self.messages.as_ref().and_then(|w| w.get(&id)).cloned() else { return };
                    if again {
                        self.on_message_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &id, false, false);
                    } else {
                        self.select(&msg);
                    }
                }
            }
            _ => {}
        }
    }
}
