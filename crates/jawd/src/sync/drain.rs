//! The converter: inbox rows become chats, messages and people as soon as they're committed, and
//! each row is deleted in the transaction its conversion's rows go in, so a crash between the two
//! converts it again and nothing twice. What was left from before jawd started goes in the
//! background lane; what arrives live goes in the live lane, a few dozen at a time so the inbox's
//! own commits are never held up for long. History kept raw by an earlier jawd is converted
//! the same way, in the background lane, alongside.

use std::time::Instant;

use prost::Message as _;
use tokio::sync::mpsc;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::{Avatars, Shared, fetch};
use crate::convert::{self, Me, app_state, people};
use crate::engine::pb;
use crate::store::{Change, Chat, Drained, Lane, Store};
use crate::work::Priority;

/// Inbox rows read and converted at once.
const READ: usize = 256;
/// Changes in one live transaction.
const LIVE_CHANGES: usize = 64;
/// History rows read and converted at once.
const HISTORY_READ: usize = 2_000;

/// What converting some inbox rows gives besides changes.
#[derive(Default)]
pub struct Converted {
    pub changes: Vec<Change>,
    /// History notifications, for the fetcher, whose inbox rows stay until their blobs are in.
    pub offers: Vec<fetch::Offer>,
    pub my_name: Option<String>,
    /// Chats and people whose pictures changed.
    pub pictures: Vec<String>,
}

/// One inbox row as it's read.
pub struct Row {
    pub seq: i64,
    pub kind: String,
    pub payload: Vec<u8>,
}

/// The HistorySyncNotification in a message from the phone, if it carries one.
pub fn history_notification(message: &e2e::Message) -> Option<&e2e::HistorySyncNotification> {
    let protocol = message
        .protocol_message
        .as_deref()
        .or_else(|| message.device_sent_message.as_deref()?.message.as_deref()?.protocol_message.as_deref())?;
    protocol.history_sync_notification.as_ref()
}

/// Converts inbox rows. A row whose kind this jawd doesn't know stays in the inbox for one that
/// does.
pub fn convert_rows(rows: &[Row], me: &Me) -> Converted {
    let mut out = Converted::default();
    for row in rows {
        let drained = match row.kind.as_str() {
            "message" => message(row, me, &mut out),
            "app_state" => match pb::AppState::decode(row.payload.as_slice()) {
                Ok(state) => {
                    let applied = app_state::mutation(&state.index, &state.value);
                    out.changes.extend(applied.changes);
                    out.my_name = applied.my_name.or(out.my_name.take());
                    out.changes.push(Change::AppState { index: state.index, value: state.value });
                    true
                }
                Err(e) => undecodable(row, e),
            },
            "push_name" => match pb::PushName::decode(row.payload.as_slice()) {
                Ok(name) => {
                    out.changes.extend(people::push_name(&name));
                    true
                }
                Err(e) => undecodable(row, e),
            },
            "group" => match pb::GroupInfo::decode(row.payload.as_slice()) {
                Ok(group) => {
                    out.changes.extend(people::group(&group));
                    true
                }
                Err(e) => undecodable(row, e),
            },
            "group_change" => match pb::GroupChange::decode(row.payload.as_slice()) {
                Ok(change) => {
                    out.changes.extend(people::group_change(&change));
                    true
                }
                Err(e) => undecodable(row, e),
            },
            "picture" => match pb::Picture::decode(row.payload.as_slice()) {
                Ok(picture) if picture.removed => {
                    let none = Chat {
                        id: picture.jid,
                        avatar: Some(String::new()),
                        avatar_id: Some(super::avatars::NONE.into()),
                        ..Chat::default()
                    };
                    out.changes.push(Change::Chat(none));
                    true
                }
                Ok(picture) => {
                    out.pictures.push(picture.jid);
                    true
                }
                Err(e) => undecodable(row, e),
            },
            "bench" => true,
            _ => false,
        };
        if drained {
            out.changes.push(Change::Drained(Drained::Inbox(row.seq)));
        }
    }
    out
}

/// A row that can't be decoded is kept, and said once.
fn undecodable(row: &Row, e: prost::DecodeError) -> bool {
    eprintln!("jawd: inbox row {} ({}) doesn't decode, so it's kept as it is: {e}", row.seq, row.kind);
    false
}

/// A live message's changes; a history notification goes to the fetcher instead, keeping its row.
fn message(row: &Row, me: &Me, out: &mut Converted) -> bool {
    let live = match pb::Message::decode(row.payload.as_slice()) {
        Ok(live) => live,
        Err(e) => return undecodable(row, e),
    };
    let web = match convert::web_message(&live) {
        Ok(web) => web,
        Err(e) => return undecodable(row, e),
    };
    if let Some(notification) = web.message.as_ref().and_then(history_notification) {
        out.offers.push(fetch::Offer {
            seq: row.seq,
            id: web.key.id.clone().unwrap_or_default(),
            notification: notification.encode_to_vec(),
        });
        return false;
    }
    let info = live.info.unwrap_or_default();
    if !info.sender_alt.is_empty() && convert::is_person(&info.sender) {
        out.changes.extend(people_joined(&info.sender, &info.sender_alt));
    }
    out.changes.extend(convert::message(&web, me, true));
    true
}

/// A sender's LID and phone number, joined.
fn people_joined(a: &str, b: &str) -> Option<Change> {
    people::lid_pairs(&pb::LidPairs { pairs: vec![pb::LidPair { lid: a.into(), phone: b.into() }] }).into_iter().next()
}

async fn read_inbox(store: &Store, after: i64) -> anyhow::Result<Vec<Row>> {
    store
        .reads()
        .query(Priority::Interactive, move |db| {
            let rows = db
                .prepare_cached("SELECT seq, kind, payload FROM inbox WHERE seq > ?1 ORDER BY seq LIMIT ?2")?
                .query_map([after, READ as i64], |r| Ok(Row { seq: r.get(0)?, kind: r.get(1)?, payload: r.get(2)? }))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        })
        .await
}

/// Converts the inbox as rows land, until jawd stops.
pub async fn inbox(store: Store, live: Shared, offered: mpsc::UnboundedSender<fetch::Offer>, avatars: Avatars) {
    let inboxed = store.inboxed();
    let left: i64 = store
        .reads()
        .query(Priority::Interactive, |db| {
            Ok(db.query_row("SELECT coalesce(max(seq), 0) FROM inbox", [], |r| r.get(0))?)
        })
        .await
        .unwrap_or_default();
    let mut after = 0;
    let started = Instant::now();
    let mut leftovers = 0usize;
    loop {
        let rows = match read_inbox(&store, after).await {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("jawd: can't read the inbox: {e:#}");
                inboxed.notified().await;
                continue;
            }
        };
        let Some(last) = rows.last().map(|row| row.seq) else {
            if leftovers > 0 {
                eprintln!(
                    "jawd: converted {leftovers} inbox rows left from before in {:.1} s",
                    started.elapsed().as_secs_f64()
                );
                leftovers = 0;
            }
            inboxed.notified().await;
            continue;
        };
        after = last;
        let from_before = last <= left;
        if from_before {
            leftovers += rows.len();
        }
        let me = live.lock().expect("live state").me.clone();
        let Ok(converted) = tokio::task::spawn_blocking(move || convert_rows(&rows, &me)).await else { continue };
        for offer in converted.offers {
            let _ = offered.send(offer);
        }
        if let Some(name) = converted.my_name
            && let Some(me) = live.lock().expect("live state").account.me.as_mut()
        {
            me.name = name;
        }
        for jid in converted.pictures {
            avatars.want(&jid);
        }
        let applied = if from_before {
            store.apply(Lane::Background, converted.changes).await.map(|_| ())
        } else {
            apply_live(&store, converted.changes).await
        };
        if let Err(e) = applied {
            eprintln!("jawd: can't store what the inbox held: {e:#}");
        }
    }
}

/// Makes changes in the live lane, LIVE_CHANGES to a transaction.
async fn apply_live(store: &Store, mut changes: Vec<Change>) -> anyhow::Result<()> {
    while !changes.is_empty() {
        let rest = changes.split_off(changes.len().min(LIVE_CHANGES));
        store.apply(Lane::Live, changes).await?;
        changes = rest;
    }
    Ok(())
}

/// Converts the history an earlier jawd kept raw, HISTORY_READ rows at a time, deleting each row
/// in the transaction its message goes in.
pub async fn history_raw(store: Store, live: Shared) {
    let started = Instant::now();
    let mut converted = 0usize;
    let mut after = (String::new(), String::new());
    loop {
        let from = after.clone();
        let rows: anyhow::Result<Vec<(String, String, Vec<u8>)>> = store
            .reads()
            .query(Priority::Background, move |db| {
                let rows = db
                    .prepare_cached(
                        "SELECT chat, id, raw FROM history_raw WHERE (chat, id) > (?1, ?2) ORDER BY chat, id LIMIT ?3",
                    )?
                    .query_map(rusqlite::params![from.0, from.1, HISTORY_READ as i64], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                    })?
                    .collect::<rusqlite::Result<_>>()?;
                Ok(rows)
            })
            .await;
        let rows = match rows {
            Ok(rows) if rows.is_empty() => break,
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("jawd: can't read the history kept raw: {e:#}");
                return;
            }
        };
        let count = rows.len();
        if let Some((chat, id, _)) = rows.last() {
            after = (chat.clone(), id.clone());
        }
        let me = live.lock().expect("live state").me.clone();
        let Ok(changes) = tokio::task::spawn_blocking(move || history_rows(rows, &me)).await else { return };
        if let Err(e) = store.apply(Lane::Background, changes).await {
            eprintln!("jawd: can't store the history kept raw: {e:#}");
            return;
        }
        converted += count;
    }
    if converted > 0 {
        let seconds = started.elapsed().as_secs_f64();
        eprintln!(
            "jawd: converted {converted} messages of history kept raw in {seconds:.1} s, {:.0} a second",
            converted as f64 / seconds.max(1e-6)
        );
    }
}

/// History rows as changes, each followed by its row's removal. A row that doesn't decode stays.
pub fn history_rows(rows: Vec<(String, String, Vec<u8>)>, me: &Me) -> Vec<Change> {
    let mut changes = Vec::new();
    for (chat, id, raw) in rows {
        match WebMessageInfo::decode(raw.as_slice()) {
            Ok(mut info) => {
                if info.key.remote_jid.as_deref() != Some(chat.as_str()) {
                    info.key.remote_jid = Some(chat.clone());
                }
                changes.extend(convert::message(&info, me, false));
                changes.push(Change::Drained(Drained::History { chat, id }));
            }
            Err(e) => eprintln!("jawd: a message of history in {chat} doesn't decode, so it's kept raw: {e}"),
        }
    }
    changes
}
