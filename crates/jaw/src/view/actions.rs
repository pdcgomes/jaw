//! M2's message actions from the selected message: r replies, e edits, d deletes, + and 1 to 6
//! react, * stars, f forwards, i shows the info, s saves its file (`save`), and Enter opens a link
//! or a file, starts a chat with whoever a contact card shares, or opens a poll's options to vote
//! with (`vote`). Each asks jawd and shows what jawd pushes back (an edited row, a deleted one, the
//! reactions, a star, a vote), never a guess of its own. @ in a group's box completes its members,
//! and what's taken goes as a real mention. A card of yours is made from a person's name and
//! number, as a vCard 3.0.

use super::{Focus, Overlay, View};
use crate::data::{Kind, Member, Message, MessageInfo, SpanKind};
use crate::fmt;
use jaw_proto::send::Card;
use jaw_proto::vcard;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding};

/// WhatsApp's quick reactions, on 1 to 6.
pub const QUICK: [&str; 6] = ["👍", "❤️", "😂", "😮", "😢", "🙏"];
/// How long WhatsApp lets you edit a message, and delete it for everyone.
pub const EDITABLE: i64 = 15 * fmt::MINUTE;
pub const DELETABLE: i64 = 60 * 60 * fmt::MINUTE;
/// The most names the completion shows.
const NAMES: usize = 5;

/// Where a name matched what's typed, as character ranges.
type Marks = Vec<(usize, usize)>;

/// The message the box replies to: its sender and its line, for the box's top border.
#[derive(Clone, Debug, PartialEq)]
pub struct Replying {
    pub id: String,
    pub from: String,
    pub colour: u8,
    pub summary: String,
}

/// The message the box is editing, and the draft it set aside, which comes back after.
#[derive(Clone, Debug, PartialEq)]
pub struct Editing {
    pub id: String,
    pub draft: String,
}

/// A name taken from the completion, and who it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Taken {
    pub name: String,
    pub person: String,
}

/// The `@` being typed: where it starts in the box, and what follows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Naming {
    pub at: usize,
    pub query: String,
}

/// A card being made of someone from their chat, whose number its members will say: the chat the
/// card goes to, the chat whose person it shares, and the name it goes by.
#[derive(Clone, Debug, PartialEq)]
pub struct Carding {
    pub to: String,
    pub of: String,
    pub name: String,
}

/// The chats a contact card offers: each card's name and each of its numbers, the WhatsApp
/// account's where the card marks one, each number once.
pub fn card_numbers(m: &Message) -> Vec<(String, String)> {
    let Some(contact) = m.contact.as_ref().filter(|_| m.kind == Kind::Contact && !m.deleted) else { return vec![] };
    let cards = contact.cards();
    let alone = cards.len() == 1;
    let mut out: Vec<(String, String)> = vec![];
    for card in &cards {
        let name = if alone { contact.name.clone().or(card.name.clone()) } else { card.name.clone() };
        let name = name.unwrap_or_else(|| "someone".into());
        for phone in &card.phones {
            if !out.iter().any(|(_, number)| number == phone.dialled()) {
                out.push((name.clone(), phone.dialled().to_string()));
            }
        }
    }
    out
}

/// What * does to a message: stars it (true), takes its star off (false), or nothing, for a line
/// about the chat or a deleted message without a star.
pub fn starring(m: &Message) -> Option<bool> {
    (!matches!(m.kind, Kind::System | Kind::Call) && (!m.deleted || m.starred)).then_some(!m.starred)
}

/// A message's text, or what it is, on one line.
pub fn one_line(m: &Message) -> String {
    let text: String = m.spans.iter().map(|s| s.s.as_str()).collect();
    match (m.kind, text.trim().is_empty()) {
        (Kind::Text, _) | (_, false) => fmt::one_line(&text),
        _ => crate::layout::summary(m),
    }
}

impl View {
    fn chat_is_group(&self) -> bool {
        self.open.as_deref().and_then(|id| self.chat_known(id)).is_some_and(|c| c.group())
    }

    /// Why a message can't be edited, if it can't.
    pub(super) fn not_editable(&self, m: &Message) -> Option<&'static str> {
        if !m.mine || m.deleted {
            Some("only your own messages can be edited")
        } else if m.kind != Kind::Text {
            Some("only text can be edited")
        } else if self.now() - m.at > EDITABLE {
            Some("WhatsApp lets a message be edited for 15 minutes after it's sent")
        } else {
            None
        }
    }

    /// Why a message can't be deleted for everyone, if it can't.
    pub(super) fn not_for_everyone(&self, m: &Message) -> Option<&'static str> {
        if !m.mine {
            Some("only your own")
        } else if m.deleted {
            Some("it's deleted already")
        } else if self.now() - m.at > DELETABLE {
            Some("only for 60 hours after sending")
        } else {
            None
        }
    }

    /// r: the box replies to the message, its line in the box's top border.
    pub(super) fn reply_to(&mut self, m: &Message) {
        if m.deleted || matches!(m.kind, Kind::System | Kind::Call) {
            self.say("there's nothing to reply to in this one");
            return;
        }
        let (from, colour) = match (&m.from, m.mine) {
            (Some(f), false) => (f.name.clone(), f.colour),
            _ => ("You".to_string(), 0),
        };
        self.editing_stop();
        self.replying = Some(Replying { id: m.id.clone(), from, colour, summary: one_line(m) });
        self.focus = Focus::Typing;
    }

    /// e: the message's text into the box to change, the draft set aside until it's saved.
    pub(super) fn edit(&mut self, m: &Message) {
        if let Some(why) = self.not_editable(m) {
            self.say(why);
            return;
        }
        if self.editing.is_none() {
            self.editing = Some(Editing { id: m.id.clone(), draft: self.composer.text().to_string() });
        } else if let Some(e) = self.editing.as_mut() {
            e.id = m.id.clone();
        }
        self.replying = None;
        let text: String = m.spans.iter().map(|s| s.s.as_str()).collect();
        self.composer.set(&text);
        self.focus = Focus::Typing;
    }

    /// The box stops editing, its draft back.
    pub(super) fn editing_stop(&mut self) {
        if let Some(e) = self.editing.take() {
            self.composer.set(&e.draft);
        }
    }

    /// 1 to 6: a quick reaction, or yours taken away when it's the one you'd given.
    pub(super) fn react_quick(&mut self, m: &Message, i: usize) {
        if let Some(emoji) = QUICK.get(i) {
            self.react(m, emoji);
        }
    }

    pub(super) fn react(&mut self, m: &Message, emoji: &str) {
        if m.deleted || matches!(m.kind, Kind::System | Kind::Call) {
            self.say("there's nothing to react to in this one");
            return;
        }
        let Some(chat) = self.open.clone() else { return };
        let bare = |e: &str| e.replace('\u{fe0f}', "");
        let mine = m.reactions.iter().any(|r| r.mine && bare(&r.emoji) == bare(emoji));
        self.jawd.react(&chat, &m.id, if mine { "" } else { emoji });
    }

    /// *: the message starred, or its star taken off, which its row shows once jawd has it. A line
    /// about the chat can't be starred, nor a deleted message, though a star can always come off.
    pub(super) fn star(&mut self, m: &Message) {
        if matches!(m.kind, Kind::System | Kind::Call) {
            self.say("a line about the chat can't be starred");
            return;
        }
        if m.deleted && !m.starred {
            self.say("a deleted message can't be starred");
            return;
        }
        let Some(chat) = self.open.clone() else { return };
        self.jawd.star(&chat, &m.id, !m.starred);
        self.say(if m.starred { "unstarred" } else { "starred" });
    }

    /// i: the sheet of who has it and who's read it, asked of jawd.
    pub(super) fn info(&mut self, m: &Message) {
        let Some(chat) = self.open.clone() else { return };
        self.info = Some((m.id.clone(), None));
        self.overlay = Some(Overlay::Info);
        self.jawd.message_info(&chat, &m.id);
    }

    pub(super) fn on_info(&mut self, message: String, info: Result<MessageInfo, String>) {
        if let Some((id, slot)) = self.info.as_mut()
            && *id == message
        {
            *slot = Some(info);
        }
    }

    /// Enter on a message with a link or a file: the link in the browser, the file in its app.
    pub(super) fn open_in(&mut self, m: &Message) -> bool {
        let link = m.spans.iter().find(|s| s.t == SpanKind::Link).map(|s| s.url.clone().unwrap_or_else(|| s.s.clone()));
        if let Some(url) = link {
            self.effects.push(super::Effect::Open(url));
            self.say("opened in the browser");
            return true;
        }
        let path = m.media.as_ref().and_then(|media| media.path.clone().or(media.previews.large.clone()));
        if let Some(path) = path {
            let path = self.images.path(&path);
            self.effects.push(super::Effect::Open(path.display().to_string()));
            self.say("opened");
            return true;
        }
        false
    }

    /// Enter on a contact card: a chat with whoever it shares, by their number, or with several
    /// numbers on it, the palette's list of them to choose from. Says whether it had a number.
    pub(super) fn chat_from_card(&mut self, m: &Message) -> bool {
        match card_numbers(m).as_slice() {
            [] => return false,
            [(name, number)] => {
                self.starting = Some(number.clone());
                self.jawd.start_chat_by_phone(number);
                self.say(format!("a chat with {name} begins"));
            }
            numbers => self.palette_card(m, numbers.to_vec()),
        }
        true
    }

    /// Sends `name`'s card to `chat`, made as a vCard 3.0 from their number, which a card needs.
    pub(super) fn send_card(&mut self, chat: &str, name: &str, phone: Option<&str>) {
        let Some(phone) = phone.filter(|phone| phone.chars().any(|c| c.is_ascii_digit())) else {
            self.say(format!("jawd knows no number for {name}, which a card needs"));
            return;
        };
        let card = Card { name: name.to_string(), vcard: vcard::write(name, &[phone]) };
        self.jawd.send_contact(chat, &[card]);
        self.say(format!("sending {name}'s card"));
    }

    pub(super) fn on_members(&mut self, chat: String, members: Vec<Member>) {
        if let Some(carding) = self.carding.take_if(|c| c.of == chat) {
            let phone = members.iter().find(|m| !self.me.contains(&m.id)).and_then(|m| m.phone.clone());
            self.send_card(&carding.to, &carding.name, phone.as_deref());
        }
        self.members.insert(chat, members);
    }

    /// The `@` being typed in a group's box, if the box has one before its cursor.
    pub(super) fn naming(&self) -> Option<Naming> {
        if !self.chat_is_group() || self.focus != Focus::Typing {
            return None;
        }
        let text = &self.composer.text()[..self.composer.cursor()];
        let at = text.rfind('@')?;
        let before_ok = text[..at].chars().next_back().is_none_or(char::is_whitespace);
        let query = &text[at + 1..];
        let fits = query.chars().count() <= 24 && !query.chars().any(char::is_whitespace);
        (before_ok && fits && self.dismissed != Some(at)).then(|| Naming { at, query: query.to_string() })
    }

    /// Who an `@` could name: the chat's members, or until jawd says them, the people seen in it.
    pub(super) fn candidates(&mut self, query: &str) -> Vec<(Member, Marks)> {
        let Some(chat) = self.open.clone() else { return vec![] };
        if !self.members.contains_key(&chat) && self.members_asked.insert(chat.clone()) {
            self.jawd.chat_info(&chat);
        }
        let mut people: Vec<Member> = self.members.get(&chat).cloned().unwrap_or_default();
        if people.is_empty() {
            let mut at = self.messages.as_ref().and_then(|w| w.oldest()).cloned();
            while let Some(m) = at {
                if let Some(f) = m.from.as_ref().filter(|f| !m.mine && !people.iter().any(|p| p.id == f.id)) {
                    people.push(Member { id: f.id.clone(), name: f.name.clone(), phone: None, admin: false });
                }
                at = self.messages.as_ref().and_then(|w| w.after(&m.sort)).cloned();
            }
        }
        people.retain(|p| !self.me.contains(&p.id));
        let mut found: Vec<(i64, Member, Marks)> = people
            .into_iter()
            .filter_map(|p| {
                if query.is_empty() {
                    return Some((0, p, vec![]));
                }
                let (score, marks) = crate::find::name(query, &p.name)?;
                Some((score, p, marks))
            })
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        found.into_iter().take(NAMES).map(|(_, p, marks)| (p, marks)).collect()
    }

    /// Keys while names are offered: says whether it took the key.
    pub(super) fn on_naming_key(&mut self, k: KeyEvent) -> bool {
        let Some(naming) = self.naming() else { return false };
        let found = self.candidates(&naming.query);
        if found.is_empty() {
            return false;
        }
        match k.code {
            KeyCode::Tab | KeyCode::Enter => {
                let (p, _) = &found[self.naming_selected.min(found.len() - 1)];
                self.composer.replace_back_to(naming.at, &format!("@{} ", p.name));
                self.taken.push(Taken { name: p.name.clone(), person: p.id.clone() });
                self.naming_selected = 0;
                self.typed();
            }
            KeyCode::Up => self.naming_selected = self.naming_selected.saturating_sub(1),
            KeyCode::Down => self.naming_selected = (self.naming_selected + 1).min(found.len() - 1),
            KeyCode::Esc => self.dismissed = Some(naming.at),
            _ => return false,
        }
        true
    }

    /// The people mentioned in `text`: those taken whose `@name` is still in it.
    pub(super) fn mentions_in(&self, text: &str) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for t in self.taken.iter().filter(|t| text.contains(&format!("@{}", t.name))) {
            if !out.contains(&t.person) {
                out.push(t.person.clone());
            }
        }
        out
    }

    /// Forgets the box's reply, edit and names, and the votes asked, as a chat is left.
    pub(super) fn drop_actions(&mut self) {
        self.editing_stop();
        self.replying = None;
        self.votes.clear();
        self.taken.clear();
        self.dismissed = None;
        self.naming_selected = 0;
    }

    /// The names offered above the box, as the wireframes have them: dashed, the match bold, and
    /// a number's last digits where two share a first name.
    pub(super) fn draw_naming(&mut self, f: &mut Frame, composer: Rect) {
        let Some(naming) = self.naming() else { return };
        let found = self.candidates(&naming.query);
        if found.is_empty() {
            return;
        }
        let t = self.theme;
        let first = |n: &str| n.split_whitespace().next().unwrap_or(n).to_string();
        let w = 40.min(composer.width);
        let h = found.len() as u16 + 2;
        if composer.y < h + 1 {
            return;
        }
        let area = Rect::new(composer.x, composer.y - h - 1, w, h);
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
            .title(Span::styled(" members ", Style::new().fg(t.comment)))
            .title_bottom(Line::from(Span::styled(" tab takes it ", Style::new().fg(t.comment))).right_aligned())
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let selected = self.naming_selected.min(found.len() - 1);
        let buf = f.buffer_mut();
        for (i, (p, marks)) in found.iter().enumerate() {
            let shared = found.iter().filter(|(o, _)| first(&o.name) == first(&p.name)).count() > 1;
            let digits = p.phone.as_deref().filter(|_| shared).map(|ph| {
                let d: String = ph.chars().filter(char::is_ascii_digit).collect();
                format!("··· {}", &d[d.len().saturating_sub(4)..])
            });
            let mut spans = vec![];
            let chars: Vec<char> = p.name.chars().collect();
            let mut at = 0;
            let plain = Style::new().fg(t.fg);
            for &(a, b) in marks {
                spans.push(Span::styled(chars[at..a.min(chars.len())].iter().collect::<String>(), plain));
                spans.push(Span::styled(
                    chars[a.min(chars.len())..b.min(chars.len())].iter().collect::<String>(),
                    plain.add_modifier(Modifier::BOLD),
                ));
                at = b.min(chars.len());
            }
            spans.push(Span::styled(chars[at..].iter().collect::<String>(), plain));
            let used: usize = spans.iter().map(|s| s.width()).sum();
            if let Some(d) = digits {
                let room = (inner.width as usize).saturating_sub(used + d.chars().count());
                spans.push(Span::raw(" ".repeat(room)));
                spans.push(Span::styled(d, Style::new().fg(t.comment)));
            }
            let y = inner.y + i as u16;
            buf.set_line(inner.x, y, &Line::from(spans), inner.width);
            if i == selected {
                buf.set_style(Rect::new(area.x + 1, y, area.width - 2, 1), Style::new().bg(t.selection));
            }
        }
    }

    /// The info sheet: when it was sent, and for each recipient when it was delivered, read and
    /// played.
    pub(super) fn draw_info(&self, f: &mut Frame) {
        let t = &self.theme;
        let Some((id, info)) = &self.info else { return };
        let m = self.messages.as_ref().and_then(|w| w.get(id));
        let mut lines = vec![];
        if let Some(m) = m {
            lines.push(Line::from(Span::styled(fmt::fit(&one_line(m), 60), Style::new().fg(t.text))));
            lines.push(Line::default());
        }
        let when = |at: Option<i64>| at.map_or("—".to_string(), |at| fmt::when(at, self.now(), self.tz));
        match info {
            None => lines.push(Line::from(Span::styled("asking jawd…", Style::new().fg(t.comment)))),
            Some(Err(why)) => lines.push(Line::from(Span::styled(why.clone(), Style::new().fg(t.comment)))),
            Some(Ok(info)) => {
                let sent = info.sent.map_or(String::new(), |at| fmt::full(at, self.tz));
                let mine = m.is_some_and(|m| m.mine);
                let said = if mine { "sent" } else { "sent to you" };
                lines.push(Line::from(vec![
                    Span::styled(format!("{said:<13}"), Style::new().fg(t.muted)),
                    Span::styled(sent, Style::new().fg(t.fg)),
                ]));
                if !mine {
                    lines.push(Line::default());
                    let said = "Receipts are only for your own messages.";
                    lines.push(Line::from(Span::styled(said, Style::new().fg(t.comment))));
                } else {
                    let played = m.is_some_and(|m| m.kind == Kind::Voice);
                    lines.push(Line::default());
                    let mut head = format!("{:<20}{:<13}{:<13}", "", "delivered", "read");
                    if played {
                        head.push_str("played");
                    }
                    lines.push(Line::from(Span::styled(head, Style::new().fg(t.muted))));
                    for r in &info.recipients {
                        let mut spans = vec![
                            Span::styled(format!("{:<20}", fmt::fit(&r.name, 19)), Style::new().fg(t.fg)),
                            Span::styled(format!("{:<13}", when(r.delivered)), Style::new().fg(t.unseen)),
                            Span::styled(format!("{:<13}", when(r.read)), Style::new().fg(t.read)),
                        ];
                        if played {
                            spans.push(Span::styled(when(r.played), Style::new().fg(t.read)));
                        }
                        lines.push(Line::from(spans));
                    }
                    if info.recipients.is_empty() {
                        lines.push(Line::from(Span::styled("No one has it yet.", Style::new().fg(t.comment))));
                    }
                }
            }
        }
        let screen = f.area();
        let w = 66.min(screen.width.saturating_sub(2));
        let h = (lines.len() as u16 + 2).min(screen.height.saturating_sub(2));
        let area = Rect::new((screen.width - w) / 2, (screen.height - h) / 2, w, h);
        f.render_widget(Clear, area);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(t.focus))
            .style(Style::new().bg(t.surface).fg(t.fg))
            .title(Span::styled(" info ", Style::new().fg(t.focus).add_modifier(Modifier::BOLD)))
            .title_bottom(Line::from(Span::styled(" any key closes ", Style::new().fg(t.muted))))
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(ratatui::widgets::Paragraph::new(lines), inner);
    }
}
