//! The made-up account's answers to M2's message actions, as jawd would push them: a reply's
//! quote and its mentions in the row it sends, one reaction of yours per message, edits for 15
//! minutes and deletes for everyone for 60 hours, forwards as new messages of yours, each
//! recipient's receipts, and a chat's members.

use super::{Account, Later, ME, markup};
use crate::data::{
    ChatKind, Kind, Member, Message, MessageInfo, Quote, Reaction, Recipient, Span, SpanKind, Tick, Update,
};
use crate::fmt::MINUTE;
use jaw_client::Update as Window;

/// How long WhatsApp lets you edit your message, and delete it for everyone.
const EDITABLE: i64 = 15 * MINUTE;
const DELETABLE: i64 = 60 * 60 * MINUTE;

/// Members who haven't written in the made-up chats, by chat.
const QUIET: &[(&str, &str, &str)] = &[("familia", "person:8", "Inês Duarte"), ("jantar", "person:9", "Marta Reis")];

/// An emoji as it's compared: with or without its variation selector, the same.
fn bare(emoji: &str) -> String {
    emoji.replace('\u{fe0f}', "")
}

/// A made-up phone number for a person, the same each time.
fn phone(person: &str) -> String {
    let n =
        person.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |n, b| (n ^ b as u64).wrapping_mul(0x100_0000_01b3)) % 10_000;
    let k = person.trim_start_matches("person:").parse::<u64>().unwrap_or(0) % 100;
    format!("+351 91{} {:03} {:04}", k % 10, 100 + k * 7 % 900, n)
}

/// The text's `@name`s for `people` as mentions; the rest as `markup` makes it.
fn mentioned(text: &str, people: &[(String, String)]) -> Vec<Span> {
    let mut out = vec![];
    let mut rest = text;
    while !rest.is_empty() {
        let next = people
            .iter()
            .filter_map(|(id, name)| rest.find(&format!("@{name}")).map(|at| (at, id, name)))
            .min_by_key(|(at, _, _)| *at);
        let Some((at, id, name)) = next else {
            out.extend(markup(rest));
            break;
        };
        out.extend(markup(&rest[..at]));
        let said = format!("@{name}");
        out.push(Span { t: SpanKind::Mention, s: said.clone(), person: Some(id.clone()), url: None });
        rest = &rest[at + said.len()..];
    }
    out
}

impl Account {
    /// Where a message is: its thread and place in it.
    pub(super) fn locate(&self, id: &str) -> Option<(usize, usize)> {
        self.threads.iter().enumerate().find_map(|(t, th)| th.messages.iter().position(|m| m.id == id).map(|i| (t, i)))
    }

    /// A message changed: a new version, upserted into the open window if it shows it.
    pub(super) fn changed(&mut self, t: usize, i: usize) {
        let m = &mut self.threads[t].messages[i];
        m.v += 1;
        let m = m.clone();
        let chat = self.threads[t].chat.id.clone();
        if self.open.as_ref().is_some_and(|o| o.chat == chat && (o.lo..o.hi).contains(&i)) {
            self.send(Update::Messages { chat, update: Window { upsert: vec![m], ..Window::default() } });
        }
    }

    pub(super) fn members(&self, chat: &str) -> Vec<Member> {
        let Some(&t) = self.index.get(chat) else { return vec![] };
        let thread = &self.threads[t];
        let mut out: Vec<Member> = vec![];
        for m in &thread.messages {
            if let Some(f) = m.from.as_ref().filter(|f| !out.iter().any(|o| o.id == f.id)) {
                out.push(Member { id: f.id.clone(), name: f.name.clone(), phone: Some(phone(&f.id)), admin: false });
            }
        }
        for (_, id, name) in QUIET.iter().filter(|(c, _, _)| *c == chat) {
            out.push(Member { id: id.to_string(), name: name.to_string(), phone: Some(phone(id)), admin: false });
        }
        if thread.chat.kind == ChatKind::Group
            && let Some(first) = out.first_mut()
        {
            first.admin = true;
        }
        out
    }

    pub(super) fn on_chat_info(&mut self, chat: &str) {
        let members = self.members(chat);
        self.send(Update::Members { chat: chat.to_string(), members });
    }

    pub(super) fn on_send_text(&mut self, chat: &str, text: &str, reply: Option<&str>, mentions: &[String]) {
        let Some(&t) = self.index.get(chat) else { return };
        let at = self.now();
        let group = self.threads[t].chat.kind == ChatKind::Group;
        let people: Vec<(String, String)> = if group {
            let members = self.members(chat);
            mentions
                .iter()
                .filter_map(|id| members.iter().find(|m| &m.id == id).map(|m| (m.id.clone(), m.name.clone())))
                .collect()
        } else {
            vec![]
        };
        let mut m = self.message(at, None, mentioned(text, &people), Some(Tick::Pending));
        m.reply = reply.and_then(|id| self.locate(id)).filter(|(rt, _)| *rt == t).map(|(rt, ri)| {
            let q = &self.threads[rt].messages[ri];
            let summary = if q.kind == Kind::Text {
                crate::fmt::one_line(&q.spans.iter().map(|s| s.s.as_str()).collect::<String>())
            } else {
                crate::layout::summary(q)
            };
            let (from, colour) = match &q.from {
                Some(f) if !q.mine => (f.name.clone(), f.colour),
                _ => ("You".to_string(), 0),
            };
            Quote { id: Some(q.id.clone()), from: Some(from), colour, summary }
        });
        let (chat, id) = (chat.to_string(), m.id.clone());
        self.arrive(t, m);
        if self.connection.state == jaw_proto::ConnectionState::Offline {
            self.send(Update::Notice("offline: it goes when WhatsApp is back".into()));
            return;
        }
        self.after(400, Later::Tick { chat: chat.clone(), id: id.clone(), tick: Tick::Sent });
        self.after(1_200, Later::Tick { chat: chat.clone(), id: id.clone(), tick: Tick::Delivered });
        self.after(3_500, Later::Tick { chat, id, tick: Tick::Read });
    }

    pub(super) fn on_react(&mut self, message: &str, emoji: &str) {
        let Some((t, i)) = self.locate(message) else { return };
        let m = &mut self.threads[t].messages[i];
        if m.deleted {
            return;
        }
        let before = m.reactions.iter().find(|r| r.mine).map(|r| bare(&r.emoji));
        for r in m.reactions.iter_mut().filter(|r| r.mine) {
            r.n = r.n.saturating_sub(1);
            r.mine = false;
        }
        m.reactions.retain(|r| r.n > 0);
        if !emoji.is_empty() && before.as_deref() != Some(&bare(emoji)) {
            match m.reactions.iter_mut().find(|r| bare(&r.emoji) == bare(emoji)) {
                Some(r) => {
                    r.n += 1;
                    r.mine = true;
                }
                None => m.reactions.push(Reaction { emoji: emoji.to_string(), n: 1, mine: true }),
            }
        }
        self.changed(t, i);
    }

    pub(super) fn on_edit(&mut self, message: &str, text: &str) {
        let Some((t, i)) = self.locate(message) else { return };
        let now = self.now();
        let m = &mut self.threads[t].messages[i];
        if !m.mine || m.deleted || m.kind != Kind::Text || now - m.at > EDITABLE {
            self.send(Update::Notice("only your own messages, for 15 minutes after sending".into()));
            return;
        }
        m.spans = markup(text);
        m.edited = true;
        self.changed(t, i);
    }

    pub(super) fn on_delete(&mut self, message: &str, everyone: bool) {
        let Some((t, i)) = self.locate(message) else { return };
        let now = self.now();
        if everyone {
            let m = &mut self.threads[t].messages[i];
            if !m.mine || now - m.at > DELETABLE {
                self.send(Update::Notice("only your own, for 60 hours after sending".into()));
                return;
            }
            m.deleted = true;
            m.spans.clear();
            m.reactions.clear();
            m.media = None;
            m.voice = None;
            self.changed(t, i);
            return;
        }
        let gone = self.threads[t].messages.remove(i);
        let chat = self.threads[t].chat.id.clone();
        if let Some(open) = self.open.as_mut().filter(|o| o.chat == chat) {
            let shown = (open.lo..open.hi).contains(&i);
            if i < open.lo {
                open.lo -= 1;
            }
            if i < open.hi {
                open.hi -= 1;
            }
            if shown {
                let update = Window { remove: vec![gone.id], ..Window::default() };
                self.send(Update::Messages { chat, update });
            }
        }
    }

    pub(super) fn on_forward(&mut self, messages: &[String], to: &[String]) {
        let originals: Vec<Message> = messages
            .iter()
            .filter_map(|id| self.locate(id))
            .map(|(t, i)| self.threads[t].messages[i].clone())
            .collect();
        let mut sent = 0;
        for chat in to {
            let Some(&t) = self.index.get(chat) else { continue };
            for o in originals.iter().filter(|o| !o.deleted) {
                let at = self.now();
                let mut m = self.message(at, None, o.spans.clone(), Some(Tick::Pending));
                m.kind = o.kind;
                m.summary = o.summary.clone();
                m.media = o.media.clone();
                m.album = o.album.clone();
                m.voice = o.voice.clone();
                m.forwarded = o.forwarded + 1;
                for s in m.spans.iter_mut().filter(|s| s.t == SpanKind::Mention) {
                    s.t = SpanKind::Text;
                    s.person = None;
                }
                let id = m.id.clone();
                self.arrive(t, m);
                self.after(400, Later::Tick { chat: chat.clone(), id, tick: Tick::Sent });
            }
            sent += 1;
        }
        let said = if sent == 1 { "forwarded".to_string() } else { format!("forwarded to {sent} chats") };
        self.send(Update::Notice(said));
    }

    pub(super) fn on_message_info(&mut self, message: &str) {
        let Some((t, i)) = self.locate(message) else { return };
        let m = &self.threads[t].messages[i];
        let chat = self.threads[t].chat.id.clone();
        let mut recipients = vec![];
        if m.mine {
            let read = matches!(m.status, Some(Tick::Read | Tick::Played));
            let delivered = read || m.status == Some(Tick::Delivered);
            for (k, member) in self.members(&chat).iter().filter(|p| p.id != ME).enumerate() {
                let k = k as i64;
                let read_at = (read && (k % 3 != 2)).then_some(m.at + (3 + 7 * k) * MINUTE);
                recipients.push(Recipient {
                    person: member.id.clone(),
                    name: member.name.clone(),
                    delivered: delivered.then_some(m.at + 2_000 + k * 1_500),
                    read: read_at,
                    played: read_at.filter(|_| m.kind == Kind::Voice).map(|r| r + MINUTE),
                });
            }
        }
        let info = MessageInfo { message: message.to_string(), sent: Some(m.at), recipients };
        self.send(Update::Info { message: message.to_string(), info: Ok(info) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_at_name_for_someone_mentioned_is_a_mention_of_them() {
        let people = vec![("person:4".to_string(), "Inês Gomes".to_string())];
        let spans = mentioned("@Inês Gomes vens *já*?", &people);
        assert_eq!(
            spans[0],
            Span { t: SpanKind::Mention, s: "@Inês Gomes".into(), person: Some("person:4".into()), url: None }
        );
        assert!(spans.iter().any(|s| s.t == SpanKind::Bold && s.s == "já"));
        assert!(mentioned("@Inês Duarte", &people).iter().all(|s| s.t != SpanKind::Mention));
    }

    #[test]
    fn made_up_numbers_are_the_same_each_time_and_differ_between_people() {
        assert_eq!(phone("person:4"), phone("person:4"));
        assert_ne!(phone("person:4"), phone("person:8"));
    }
}
