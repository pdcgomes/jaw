//! The made-up account `jawd demo` serves: thousands of chats with a million messages between them,
//! sized as `jawd bench store` sizes them (three big chats and a long tail), between people with
//! names, in one-to-one chats and groups with members; the newest chat a minute old and the oldest
//! a year. Its phone offers the whole history once linked, every chat's newest messages first and
//! then the rest, newest first, in blobs made only when the engine is asked for one, so none of it
//! waits in memory: each message is made from its chat and place alone, the same each time.

use std::collections::HashMap;

use prost::Message as _;
use prost::encoding::{WireType, encode_key, encode_varint};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_history_sync::history_sync::HistorySyncType;
use wa_proto::wa_web_protobufs_history_sync::{Conversation, HistorySync, HistorySyncMsg};
use wa_proto::wa_web_protobufs_web::{WebMessageInfo, web_message_info};

use crate::engine::demo::{self, Blob, ME};
use crate::engine::pb;
use crate::find::bench::{FIRST, GROUPS, LAST};
use crate::store::bench::{BIG_CHATS, DAY_MS, Rng, SPAN_MS, chat_id, lid, phone, sizes, text};

/// How many of each chat's newest messages come in the history's first pass.
const RECENT: usize = 20;
/// Chats with messages you haven't read, among the newest.
const UNREAD_CHATS: usize = 30;
/// What names a demo history blob, with its number after.
const BLOB: &str = "/demo/history/";
const YEAR_MS: i64 = 365 * DAY_MS;

/// How big the account is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Shape {
    pub chats: usize,
    pub people: usize,
    pub messages: usize,
    /// Messages in each history blob.
    pub chunk: usize,
}

pub struct Account {
    pub shape: Shape,
    /// When the history ends.
    now: i64,
    chats: Vec<Spec>,
    by_id: HashMap<String, usize>,
    /// Where each stretch of the history starts, in the order the phone sends them (every chat's
    /// newest RECENT, then the rest of each), and one past the last.
    starts: Vec<usize>,
}

/// A chat, as its messages are made.
struct Spec {
    id: String,
    name: Option<String>,
    size: usize,
    /// Its oldest message's number in the account, which its messages' IDs count from.
    first: u64,
    begins: i64,
    ends: i64,
    unread: u32,
    /// For a group, its members but you, by person.
    members: Vec<usize>,
}

impl Spec {
    fn group(&self) -> bool {
        self.id.ends_with("@g.us")
    }

    fn recent(&self) -> usize {
        self.size.min(RECENT)
    }
}

/// A well-mixed number from `x`, never 0, so each message's generator starts from its own place.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (x ^ (x >> 31)) | 1
}

/// Person `n`'s name: a first name and a surname.
pub fn name(n: usize) -> String {
    let mut rng = Rng(mix(n as u64 ^ 0x4E41_4D45));
    let first = FIRST[rng.below(FIRST.len() as u64) as usize];
    let last = LAST[rng.below(LAST.len() as u64) as usize];
    format!("{first} {last}")
}

fn first_name(n: usize) -> String {
    name(n).split(' ').next().unwrap_or_default().to_string()
}

/// The person whose LID `jid` is, if it's one of the account's.
fn person_of_lid(jid: &str, people: usize) -> Option<usize> {
    let number = |jid: &str| jid.strip_suffix("@lid")?.parse::<u64>().ok();
    let n = number(jid)?.checked_sub(number(&lid(0))?)? as usize;
    (n < people).then_some(n)
}

impl Account {
    pub fn new(shape: Shape, now: i64) -> Account {
        let chats = shape.chats.max(BIG_CHATS + 1);
        let people = shape.people.max(chats);
        let shape = Shape { chats, people, messages: shape.messages.max(chats), chunk: shape.chunk.max(1) };
        let mut first = 0u64;
        let specs: Vec<Spec> = sizes(shape.messages, chats)
            .into_iter()
            .enumerate()
            .map(|(n, size)| {
                let mut rng = Rng(mix(n as u64 ^ 0x4348_4154));
                let id = chat_id(n);
                let group = id.ends_with("@g.us");
                // A few chats this hour, a few hundred this week, the last a year ago.
                let age = (n as f64 / chats as f64).powi(2) * YEAR_MS as f64;
                let ends = now - 60_000 - age as i64 - rng.below(60_000) as i64;
                let span = if n < BIG_CHATS { 3 * YEAR_MS } else { rng.below(SPAN_MS as u64) as i64 + DAY_MS };
                let mut members: Vec<usize> = if group {
                    (0..3 + rng.below(30)).map(|_| rng.below(people as u64) as usize).collect()
                } else {
                    Vec::new()
                };
                members.sort_unstable();
                members.dedup();
                let spec = Spec {
                    name: group.then(|| format!("{} {}", GROUPS[n % GROUPS.len()], n / GROUPS.len())),
                    id,
                    size,
                    first,
                    begins: ends - span,
                    ends,
                    unread: if n < UNREAD_CHATS && n % 3 != 1 { 1 + rng.below(8) as u32 } else { 0 },
                    members,
                };
                first += size as u64;
                spec
            })
            .collect();
        let mut starts = vec![0];
        for spec in &specs {
            starts.push(starts.last().copied().unwrap_or_default() + spec.recent());
        }
        for spec in &specs {
            starts.push(starts.last().copied().unwrap_or_default() + spec.size - spec.recent());
        }
        let by_id = specs.iter().enumerate().map(|(n, spec)| (spec.id.clone(), n)).collect();
        let messages = starts.last().copied().unwrap_or_default();
        Account { shape: Shape { messages, ..shape }, now, chats: specs, by_id, starts }
    }

    pub fn chats(&self) -> usize {
        self.chats.len()
    }

    pub fn chat(&self, c: usize) -> &str {
        &self.chats[c].id
    }

    /// How many messages chat `c` has in the history.
    pub fn size(&self, c: usize) -> usize {
        self.chats[c].size
    }

    /// The ID of chat `c`'s message `i`, counting from its oldest.
    pub fn message_id(&self, c: usize, i: usize) -> String {
        format!("3EB0{:016X}", self.chats[c].first + i as u64)
    }

    /// How many blobs the phone offers.
    pub fn blobs(&self) -> usize {
        self.shape.messages.div_ceil(self.shape.chunk)
    }

    /// The phone's history notifications, one for each blob, as messages from your own phone.
    pub fn notifications(&self) -> Vec<pb::Event> {
        let recent = self.starts[self.chats.len()];
        (0..self.blobs())
            .map(|k| {
                let kind = if (k + 1) * self.shape.chunk <= recent {
                    HistorySyncType::InitialBootstrap
                } else {
                    HistorySyncType::Full
                };
                let notification = e2e::HistorySyncNotification {
                    sync_type: Some(kind as i32),
                    chunk_order: Some(k as u32),
                    progress: Some(self.progress(k)),
                    direct_path: Some(format!("{BLOB}{k}")),
                    ..Default::default()
                };
                let protocol = e2e::ProtocolMessage {
                    r#type: Some(e2e::protocol_message::Type::HistorySyncNotification as i32),
                    history_sync_notification: Some(notification),
                    ..Default::default()
                };
                let raw = e2e::Message { protocol_message: Some(Box::new(protocol)), ..Default::default() };
                pb::Event {
                    seq: 0,
                    kind: Some(pb::event::Kind::Message(pb::Message {
                        info: Some(pb::MessageInfo {
                            chat: ME.into(),
                            sender: ME.into(),
                            from_me: true,
                            id: format!("3EB0DEMOHISTORY{k:05}"),
                            r#type: "protocol".into(),
                            timestamp_ms: self.now,
                            ..Default::default()
                        }),
                        raw: raw.encode_to_vec(),
                        ..Default::default()
                    })),
                }
            })
            .collect()
    }

    fn progress(&self, k: usize) -> u32 {
        ((k + 1) * 100 / self.blobs().max(1)) as u32
    }

    /// Blob `k`: the messages in its stretch of the history, a conversation for each chat they're
    /// in, with each chat's name, and its unread count and time where its newest message is. It's
    /// encoded a message at a time, since a decoded message takes 16 KB whatever it holds, and the
    /// demo engine's blob would otherwise count against jawd's memory.
    pub fn blob(&self, k: usize) -> Option<Blob> {
        let (from, to) = (k * self.shape.chunk, ((k + 1) * self.shape.chunk).min(self.shape.messages));
        if from >= to {
            return None;
        }
        let chats = self.chats.len();
        // Each conversation's fields but its messages, and its messages encoded as field 2.
        let mut conversations: Vec<(Conversation, Vec<u8>)> = Vec::new();
        let mut placed: HashMap<usize, usize> = HashMap::new();
        let mut segment = self.starts.partition_point(|&start| start <= from) - 1;
        let mut at = from;
        while at < to {
            let (start, end) = (self.starts[segment], self.starts[segment + 1]);
            if start == end {
                segment += 1;
                continue;
            }
            let (c, newest) = if segment < chats {
                (segment, self.chats[segment].size - 1)
            } else {
                (segment - chats, self.chats[segment - chats].size - self.chats[segment - chats].recent() - 1)
            };
            let spec = &self.chats[c];
            let place = *placed.entry(c).or_insert_with(|| {
                let head = Conversation { id: spec.id.clone(), name: spec.name.clone(), ..Default::default() };
                conversations.push((head, Vec::new()));
                conversations.len() - 1
            });
            let (head, messages) = &mut conversations[place];
            for slot in at..end.min(to) {
                let i = newest - (slot - start);
                if i == spec.size - 1 {
                    head.unread_count = Some(spec.unread);
                    head.conversation_timestamp = Some((spec.ends / 1000) as u64);
                }
                let entry =
                    HistorySyncMsg { message: Some(self.message(c, i)), msg_order_id: Some(spec.first + i as u64) };
                encode_key(2, WireType::LengthDelimited, messages);
                entry.encode_length_delimited(messages).expect("a Vec grows to fit");
            }
            at = end.min(to);
            segment += 1;
        }
        let kind = if to <= self.starts[chats] { HistorySyncType::InitialBootstrap } else { HistorySyncType::Full };
        let progress = self.progress(k);
        let head = HistorySync {
            sync_type: kind as i32,
            chunk_order: Some(k as u32),
            progress: Some(progress),
            ..Default::default()
        };
        let mut data = head.encode_to_vec();
        for (head, messages) in &conversations {
            let head = head.encode_to_vec();
            encode_key(2, WireType::LengthDelimited, &mut data);
            encode_varint((head.len() + messages.len()) as u64, &mut data);
            data.extend_from_slice(&head);
            data.extend_from_slice(messages);
        }
        Some(Blob {
            data,
            kind: kind as i32,
            chunk_order: k as u32,
            progress,
            conversations: conversations.len() as u32,
        })
    }

    /// Chat `c`'s message `i`, counting from its oldest, as the phone keeps it.
    pub fn message(&self, c: usize, i: usize) -> WebMessageInfo {
        let spec = &self.chats[c];
        let number = spec.first + i as u64;
        let mut rng = Rng(mix(number));
        let step = ((spec.ends - spec.begins) / spec.size as i64).max(1);
        let at = spec.begins + step * i as i64 + rng.below(step as u64) as i64;
        let group = spec.group();
        let mine = rng.below(if group { 10 } else { 2 }) == 0;
        let from = (!mine).then(|| if group { spec.members[rng.below(spec.members.len() as u64) as usize] } else { c });
        let sender = from.map(|n| if group { lid(n) } else { spec.id.clone() });
        let words = text(&mut rng);
        let secret: Vec<u8> = (0..4).flat_map(|_| rng.next().to_le_bytes()).collect();
        let participant = sender.clone().filter(|_| group);
        WebMessageInfo {
            key: MessageKey {
                remote_jid: Some(spec.id.clone()),
                from_me: Some(mine),
                id: Some(self.message_id(c, i)),
                participant: participant.clone(),
            },
            message: Some(e2e::Message {
                conversation: Some(words),
                message_context_info: Some(e2e::MessageContextInfo {
                    message_secret: Some(secret),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            message_timestamp: Some((at / 1000) as u64),
            participant,
            push_name: from.map(first_name),
            status: mine.then_some(web_message_info::Status::Read as i32),
            ..Default::default()
        }
    }

    /// A message someone sends in chat `c` at `at`, as WhatsApp delivers one live.
    pub fn live(&self, c: usize, id: String, at: i64) -> pb::Event {
        let spec = &self.chats[c];
        let mut rng = Rng(mix(id.bytes().fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)))));
        let group = spec.group();
        let from = if group { spec.members[rng.below(spec.members.len() as u64) as usize] } else { c };
        let raw = e2e::Message { conversation: Some(text(&mut rng)), ..Default::default() };
        pb::Event {
            seq: 0,
            kind: Some(pb::event::Kind::Message(pb::Message {
                info: Some(pb::MessageInfo {
                    chat: spec.id.clone(),
                    sender: if group { lid(from) } else { spec.id.clone() },
                    sender_alt: if group { phone(from) } else { String::new() },
                    group,
                    id,
                    r#type: "text".into(),
                    push_name: first_name(from),
                    timestamp_ms: at,
                    ..Default::default()
                }),
                raw: raw.encode_to_vec(),
                ..Default::default()
            })),
        }
    }

    /// The other side of chat `c` reading the newest of your messages in it, at `at`, if one of
    /// its last few is yours.
    pub fn read(&self, c: usize, at: i64) -> Option<pb::Event> {
        let spec = &self.chats[c];
        let i = (spec.size.saturating_sub(RECENT)..spec.size)
            .rev()
            .find(|&i| self.message(c, i).key.from_me == Some(true))?;
        let reader = if spec.group() { lid(*spec.members.first()?) } else { spec.id.clone() };
        Some(pb::Event {
            seq: 0,
            kind: Some(pb::event::Kind::Receipt(pb::Receipt {
                chat: spec.id.clone(),
                sender: reader,
                group: spec.group(),
                ids: vec![self.message_id(c, i)],
                timestamp_ms: at,
                r#type: "read".into(),
                ..Default::default()
            })),
        })
    }
}

impl demo::Account for Account {
    fn contacts(&self) -> pb::ContactList {
        let contacts = (0..self.shape.people)
            .map(|n| {
                // One in seven is known only by the name they gave themselves.
                let full_name = if n % 7 == 3 { String::new() } else { name(n) };
                pb::Contact {
                    jid: phone(n),
                    lid: lid(n),
                    first_name: full_name.split(' ').next().unwrap_or_default().to_string(),
                    full_name,
                    push_name: first_name(n),
                    ..Default::default()
                }
            })
            .collect();
        pb::ContactList { contacts }
    }

    fn groups(&self) -> pb::GroupList {
        let groups = self
            .chats
            .iter()
            .filter(|spec| spec.group())
            .map(|spec| {
                let mut participants: Vec<pb::Participant> = spec
                    .members
                    .iter()
                    .map(|&m| pb::Participant { jid: lid(m), phone: phone(m), lid: lid(m), ..Default::default() })
                    .collect();
                participants.push(pb::Participant {
                    jid: ME.into(),
                    phone: ME.into(),
                    admin: true,
                    ..Default::default()
                });
                pb::GroupInfo {
                    jid: spec.id.clone(),
                    name: spec.name.clone().unwrap_or_default(),
                    created_ms: spec.begins,
                    participants,
                    ..Default::default()
                }
            })
            .collect();
        pb::GroupList { groups }
    }

    fn members(&self, group: &str) -> Vec<String> {
        let Some(&c) = self.by_id.get(group) else { return Vec::new() };
        let mut members: Vec<String> = self.chats[c].members.iter().map(|&m| lid(m)).collect();
        members.dedup();
        members
    }

    fn lid_pairs(&self, lids: &[String]) -> pb::LidPairs {
        let pairs = lids
            .iter()
            .filter_map(|jid| person_of_lid(jid, self.shape.people).map(|n| (jid, n)))
            .map(|(jid, n)| pb::LidPair { lid: jid.clone(), phone: phone(n) })
            .collect();
        pb::LidPairs { pairs }
    }

    fn history(&self, notification: &e2e::HistorySyncNotification) -> Option<Blob> {
        let k: usize = notification.direct_path.as_deref()?.strip_prefix(BLOB)?.parse().ok()?;
        (notification.chunk_order == Some(k as u32)).then(|| self.blob(k)).flatten()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::engine::demo::Account as _;

    fn small() -> Account {
        Account::new(Shape { chats: 40, people: 60, messages: 5_000, chunk: 700 }, 1_792_000_000_000)
    }

    #[test]
    fn every_message_of_the_history_comes_in_exactly_one_blob_each_chats_newest_first() {
        let account = small();
        let mut seen = HashSet::new();
        for k in 0..account.blobs() {
            let encoded = account.blob(k).unwrap();
            let blob = HistorySync::decode(encoded.data.as_slice()).unwrap();
            assert_eq!((blob.chunk_order, blob.conversations.len()), (Some(k as u32), encoded.conversations as usize));
            for conversation in &blob.conversations {
                let c = account.by_id[&conversation.id];
                assert!(!conversation.messages.is_empty(), "a conversation with nothing in it");
                for entry in &conversation.messages {
                    let id = entry.message.as_ref().unwrap().key.id.clone().unwrap();
                    assert!(seen.insert(id), "a message came twice");
                }
                if k == 0 {
                    let newest = conversation.messages[0].message.as_ref().unwrap();
                    assert_eq!(newest.key.id.as_deref(), Some(account.message_id(c, account.size(c) - 1).as_str()));
                    assert!(conversation.conversation_timestamp.is_some(), "its time comes with its newest message");
                }
            }
        }
        assert_eq!(seen.len(), account.shape.messages);
        assert!(account.blob(account.blobs()).is_none(), "nothing past the last blob");
        let sizes: usize = (0..account.chats()).map(|c| account.size(c)).sum();
        assert_eq!(sizes, account.shape.messages);
    }

    #[test]
    fn a_blob_is_the_same_each_time_its_asked_for() {
        let (one, two) = (small(), small());
        let notification = &account_notification(&one, 3);
        assert_eq!(one.history(notification).unwrap().data, two.history(notification).unwrap().data);
        let wrong = e2e::HistorySyncNotification { chunk_order: Some(4), ..notification.clone() };
        assert!(one.history(&wrong).is_none(), "a notification whose path and number disagree names nothing");
    }

    fn account_notification(account: &Account, k: usize) -> e2e::HistorySyncNotification {
        let Some(pb::event::Kind::Message(message)) = account.notifications().swap_remove(k).kind else {
            panic!("a notification is a message")
        };
        let raw = e2e::Message::decode(message.raw.as_slice()).unwrap();
        raw.protocol_message.unwrap().history_sync_notification.unwrap()
    }

    #[test]
    fn a_chats_messages_go_forward_in_time_and_end_where_its_newest_is() {
        let account = small();
        for c in [0, 1, 7, 39] {
            let times: Vec<u64> =
                (0..account.size(c)).map(|i| account.message(c, i).message_timestamp.unwrap()).collect();
            assert!(times.windows(2).all(|w| w[0] <= w[1]), "chat {c}'s messages go back in time");
            assert!(times.last().copied().unwrap() as i64 * 1000 <= account.chats[c].ends);
        }
        assert!(account.chats[0].ends > account.chats[39].ends, "the first chat is the newest");
    }

    #[test]
    fn group_members_write_from_their_lids_which_the_account_joins_to_their_numbers() {
        let account = small();
        let group = (0..account.chats()).find(|&c| account.chat(c).ends_with("@g.us")).unwrap();
        let members = account.members(account.chat(group));
        assert!(!members.is_empty());
        let pairs = account.lid_pairs(&members);
        assert_eq!(pairs.pairs.len(), members.len());
        assert!(pairs.pairs.iter().all(|p| p.phone.ends_with("@s.whatsapp.net")));
        assert!(account.lid_pairs(&["999@lid".to_string()]).pairs.is_empty(), "a LID that isn't the account's");
        let contacts = account.contacts();
        assert_eq!(contacts.contacts.len(), account.shape.people);
        assert!(contacts.contacts.iter().any(|c| c.full_name.is_empty() && !c.push_name.is_empty()));
    }
}
