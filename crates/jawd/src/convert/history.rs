//! A history blob from the phone: each conversation's chat as the phone has it (its name, pin,
//! archive, mute, where it was read and community) and its messages, then the push names and the
//! LIDs of phone numbers the blob carries. A one-to-one conversation the phone keeps by LID goes under
//! its phone number's JID when the blob gives it, as live messages do.
//!
//! A blob is walked as it's encoded, a message at a time, and never decoded whole: prost lays out
//! WhatsApp's optional message parts inline, so a decoded message takes 16 KB whatever it holds,
//! and a blob holds tens of thousands. What a blob costs is then its encoded size and the changes
//! on their way to the writer.

use prost::DecodeError;
use prost::Message as _;
use prost::encoding::{DecodeContext, WireType, decode_key, decode_varint, encode_key, encode_varint, skip_field};
use wa_proto::wa_web_protobufs_history_sync::{Conversation, HistorySync, HistorySyncMsg};
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::{Me, is_person, message, user_jid};
use crate::store::{Change, Chat, Person, Read};

/// HistorySync's conversations and its statuses, each a message the walk takes one at a time.
const CONVERSATIONS: u32 = 2;
const STATUSES: u32 = 3;
/// Conversation's messages.
const MESSAGES: u32 = 2;

/// What walking a blob found besides the changes it handed over.
#[derive(Debug)]
pub struct Walked {
    pub sync_type: i32,
    pub chunk_order: Option<u32>,
    pub progress: Option<u32>,
    pub conversations: usize,
    /// The message rows among the changes.
    pub messages: usize,
    /// The blob as the phone sent it, less the messages that became rows, whose raws are kept with
    /// them: every conversation's settings and the messages that didn't (edits, reactions and the
    /// like), the push names, statuses, past participants and everything jaw doesn't read yet, the
    /// fields this jawd's protobufs don't know among them.
    pub rest: Vec<u8>,
}

/// Why a walk ended before the blob did.
#[derive(Debug)]
pub enum Unwalked {
    /// The blob doesn't decode, and nothing of it was handed over.
    Undecodable(DecodeError),
    /// What took its changes wanted no more.
    Stopped,
}

impl From<DecodeError> for Unwalked {
    fn from(e: DecodeError) -> Unwalked {
        Unwalked::Undecodable(e)
    }
}

/// Converts the blob encoded in `data`, handing its changes to `take` in order, about `chunk` at a
/// time: each conversation's chat, where it was read and its messages' changes, then the people
/// the blob names. The whole blob is checked before anything is handed over, so one that doesn't
/// decode changes nothing. `take` saying no stops the walk.
pub fn walk(data: &[u8], me: &Me, chunk: usize, mut take: impl FnMut(Vec<Change>) -> bool) -> Result<Walked, Unwalked> {
    let head = check(data)?;
    let mut walked = Walked {
        sync_type: head.sync_type,
        chunk_order: head.chunk_order,
        progress: head.progress,
        conversations: 0,
        messages: 0,
        rest: Vec::new(),
    };
    let mut changes = Vec::new();
    for field in Fields(data) {
        let field = field?;
        let Some(encoded) = field.holds(CONVERSATIONS) else {
            walked.rest.extend_from_slice(field.whole);
            continue;
        };
        let conversation = conversation_head(encoded)?;
        let (id, chat) = chat(&conversation);
        // Where it was read comes first and depends on all its messages, so they're converted
        // once to find it and again as they're handed over, rather than held.
        let mut reading = Reading::new(&conversation);
        for field in Fields(encoded) {
            if let Some(message) = field?.holds(MESSAGES) {
                reading.add(&entry(message, &id, me)?.0);
            }
        }
        changes.extend(chat);
        changes.push(Change::Read(reading.read(&conversation, &id)));
        hand(&mut changes, chunk, &mut take)?;
        let mut kept = Vec::new();
        for field in Fields(encoded) {
            let field = field?;
            let Some(message) = field.holds(MESSAGES) else {
                kept.extend_from_slice(field.whole);
                continue;
            };
            let (converted, row) = entry(message, &id, me)?;
            if !row {
                kept.extend_from_slice(field.whole);
            }
            walked.messages += converted.iter().filter(|c| matches!(c, Change::Message(_))).count();
            changes.extend(converted);
            hand(&mut changes, chunk, &mut take)?;
        }
        encode_key(CONVERSATIONS, WireType::LengthDelimited, &mut walked.rest);
        encode_varint(kept.len() as u64, &mut walked.rest);
        walked.rest.extend_from_slice(&kept);
        walked.conversations += 1;
    }
    changes.extend(people(&head));
    if !changes.is_empty() && !take(changes) {
        return Err(Unwalked::Stopped);
    }
    Ok(walked)
}

/// Hands `changes` over once there are `chunk` of them.
fn hand(changes: &mut Vec<Change>, chunk: usize, take: &mut impl FnMut(Vec<Change>) -> bool) -> Result<(), Unwalked> {
    if changes.len() >= chunk && !take(std::mem::take(changes)) {
        return Err(Unwalked::Stopped);
    }
    Ok(())
}

/// Checks that the whole blob decodes as prost would decode it, a message at a time, and says
/// what it holds besides its conversations and statuses.
fn check(data: &[u8]) -> Result<HistorySync, DecodeError> {
    let mut head = Vec::new();
    for field in Fields(data) {
        let field = field?;
        if let Some(encoded) = field.holds(CONVERSATIONS) {
            let mut conversation = Vec::new();
            for field in Fields(encoded) {
                let field = field?;
                match field.holds(MESSAGES) {
                    Some(message) => {
                        HistorySyncMsg::decode(message)?;
                    }
                    None => conversation.extend_from_slice(field.whole),
                }
            }
            Conversation::decode(conversation.as_slice())?;
        } else if let Some(status) = field.holds(STATUSES) {
            WebMessageInfo::decode(status)?;
        } else {
            head.extend_from_slice(field.whole);
        }
    }
    HistorySync::decode(head.as_slice())
}

/// A conversation's fields but its messages.
fn conversation_head(encoded: &[u8]) -> Result<Conversation, DecodeError> {
    let mut head = Vec::new();
    for field in Fields(encoded) {
        let field = field?;
        if field.holds(MESSAGES).is_none() {
            head.extend_from_slice(field.whole);
        }
    }
    Conversation::decode(head.as_slice())
}

/// A field of an encoded message: its number, and the field as encoded, its key included.
struct Field<'a> {
    number: u32,
    whole: &'a [u8],
    /// What a length-delimited field holds.
    value: Option<&'a [u8]>,
}

impl<'a> Field<'a> {
    /// The message this field holds if it's field `number`. One of that number with a wire type a
    /// message can't have is left among the fields decoded whole, where prost refuses it.
    fn holds(&self, number: u32) -> Option<&'a [u8]> {
        self.value.filter(|_| self.number == number)
    }
}

/// The fields of an encoded message in order, read as prost reads them; one it would refuse ends
/// them with its error.
struct Fields<'a>(&'a [u8]);

impl<'a> Fields<'a> {
    fn read(&mut self) -> Result<Field<'a>, DecodeError> {
        let start = self.0;
        let (number, wire_type) = decode_key(&mut self.0)?;
        let mut value = self.0;
        skip_field(wire_type, number, &mut self.0, DecodeContext::default())?;
        let value = match wire_type {
            WireType::LengthDelimited => {
                decode_varint(&mut value)?;
                Some(&value[..value.len() - self.0.len()])
            }
            _ => None,
        };
        Ok(Field { number, whole: &start[..start.len() - self.0.len()], value })
    }
}

impl<'a> Iterator for Fields<'a> {
    type Item = Result<Field<'a>, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            return None;
        }
        let field = self.read();
        if field.is_err() {
            self.0 = &[];
        }
        Some(field)
    }
}

/// The people a blob names besides its conversations: the LIDs of phone numbers, then push names.
fn people(history: &HistorySync) -> Vec<Change> {
    let mut people: Vec<Change> = history
        .phone_number_to_lid_mappings
        .iter()
        .filter_map(|m| {
            let jids: Vec<String> = [m.pn_jid.as_deref(), m.lid_jid.as_deref()]
                .into_iter()
                .flatten()
                .filter(|j| is_person(j))
                .map(user_jid)
                .collect();
            (jids.len() == 2).then(|| Change::Person(Person { jids, ..Person::default() }))
        })
        .collect();
    people.extend(history.pushnames.iter().filter_map(|p| {
        let jid = p.id.as_deref().filter(|j| is_person(j))?;
        let name = p.pushname.clone().filter(|n| !n.is_empty() && n != "-")?;
        Some(Change::Person(Person { jids: vec![user_jid(jid)], push_name: Some(name), ..Person::default() }))
    }));
    people
}

/// Seconds or milliseconds since the epoch, as milliseconds: the phone's fields use both.
fn ms(time: u64) -> i64 {
    let time = time.min(i64::MAX as u64 / 1000) as i64;
    if time < 100_000_000_000 { time * 1000 } else { time }
}

/// A conversation's chat's JID, and its chat as the phone has it, after the person it joins when
/// the phone gives both their phone number and LID.
fn chat(c: &Conversation) -> (String, Vec<Change>) {
    let mut id = c.id.clone();
    let pn = c.pn_jid.as_deref().filter(|j| j.ends_with("@s.whatsapp.net")).map(user_jid);
    let lid = c.lid_jid.as_deref().filter(|j| j.ends_with("@lid")).map(user_jid);
    if id.ends_with("@lid")
        && let Some(pn) = &pn
    {
        id = pn.clone();
    }
    let mut changes = Vec::new();
    if let (Some(pn), Some(lid)) = (&pn, &lid) {
        changes.push(Change::Person(Person { jids: vec![pn.clone(), lid.clone()], ..Person::default() }));
    }
    let kind = if c.is_parent_group.unwrap_or(false) {
        Some("community")
    } else if c.is_default_subgroup.unwrap_or(false) {
        Some("announcement")
    } else {
        None
    };
    let name = c.name.clone().or_else(|| c.display_name.clone()).filter(|n| !n.is_empty() && !is_person(&id));
    changes.push(Change::Chat(Chat {
        id: id.clone(),
        kind,
        name,
        pinned: c.pinned.map(|at| if at > 0 { ms(u64::from(at)) } else { 0 }),
        archived: c.archived,
        muted_until: c.mute_end_time.map(ms),
        marked_unread: c.marked_as_unread,
        community: c.parent_group_id.clone(),
        ephemeral: c.ephemeral_expiration,
        ..Chat::default()
    }));
    (id, changes)
}

/// One message of a conversation in chat `id`, as encoded: its changes, and whether it became a
/// row, which then keeps its raw.
fn entry(encoded: &[u8], id: &str, me: &Me) -> Result<(Vec<Change>, bool), DecodeError> {
    let entry = HistorySyncMsg::decode(encoded)?;
    let Some(mut info) = entry.message else { return Ok((Vec::new(), false)) };
    if info.key.remote_jid.as_deref() != Some(id) {
        info.key.remote_jid = Some(id.to_string());
    }
    let changes = message(&info, me, false);
    let key = info.key.id.as_deref();
    let row = changes.iter().any(|c| matches!(c, Change::Message(m) if Some(m.id.as_str()) == key && m.raw.is_some()));
    Ok((changes, row))
}

/// Where the phone had a conversation read up to, worked out as its messages are converted: just
/// before the oldest of its unread messages from others among those the blob carries, or, when it
/// carries fewer than the phone counts, as of the conversation's newest moment with the phone's own
/// counts standing in.
struct Reading {
    newest: i64,
    theirs: Vec<i64>,
}

impl Reading {
    fn new(c: &Conversation) -> Reading {
        let newest =
            c.conversation_timestamp.map(ms).unwrap_or_default().max(c.last_msg_timestamp.map(ms).unwrap_or_default());
        Reading { newest, theirs: Vec::new() }
    }

    fn add(&mut self, changes: &[Change]) {
        let rows = changes.iter().filter_map(|change| match change {
            Change::Message(m) => Some(m),
            Change::Edited { shown, .. } => Some(shown.as_ref()),
            _ => None,
        });
        for m in rows {
            self.newest = self.newest.max(m.at);
            if !m.mine && !matches!(m.kind, "system" | "call") {
                self.theirs.push(m.at);
            }
        }
    }

    fn read(mut self, c: &Conversation, chat: &str) -> Read {
        self.theirs.sort_unstable_by(|a, b| b.cmp(a));
        let unread = c.unread_count.unwrap_or(0) as usize;
        let read = Read { chat: chat.to_string(), at: self.newest, ..Read::default() };
        match unread {
            0 => read,
            n if n <= self.theirs.len() => Read { at: self.theirs[n - 1] - 1, ..read },
            n => Read { phone: Some((n as u32, c.unread_mention_count.unwrap_or(0))), ..read },
        }
    }
}

#[cfg(test)]
mod tests;
