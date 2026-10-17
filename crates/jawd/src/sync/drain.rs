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

use super::{Avatars, Merges, Shared, fetch};
use crate::convert::{self, Me, app_state, people};
use crate::engine::pb;
use crate::store::{Amend, Amendment, Change, Chat, Drained, Lane, Packed, Read, Store};
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
            "receipt" => match pb::Receipt::decode(row.payload.as_slice()) {
                Ok(receipt) => {
                    out.changes.extend(receipt_changes(&receipt));
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

/// What a receipt changes. Reading on another of your devices reads the chat here; a recipient's
/// delivered, read or played marks your message, or waits in `pending` for it. The rest (retries,
/// server errors and the like) are whatsmeow's business and change nothing jaw keeps.
pub fn receipt_changes(receipt: &pb::Receipt) -> Vec<Change> {
    let chat = convert::user_jid(&receipt.chat);
    if chat.is_empty() {
        return Vec::new();
    }
    let status = match receipt.r#type.as_str() {
        "read-self" | "played-self" => {
            return vec![Change::Read(Read {
                chat,
                at: receipt.timestamp_ms,
                ids: receipt.ids.clone(),
                phone: None,
                here: false,
            })];
        }
        "" | "delivered" => "delivered",
        "read" => "read",
        "played" => "played",
        _ => return Vec::new(),
    };
    if receipt.from_me || receipt.sender.is_empty() {
        return Vec::new();
    }
    receipt
        .ids
        .iter()
        .map(|id| {
            Change::Amend(Amend {
                chat: chat.clone(),
                target: id.clone(),
                sender: Some(convert::user_jid(&receipt.sender)),
                mine: false,
                at: receipt.timestamp_ms,
                what: Amendment::Status { status: status.into() },
                raw: None,
            })
        })
        .collect()
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
    let mut changes = convert::message(&web, me, true);
    // A poll vote or an encrypted reaction is readable only through what the engine decrypted,
    // which the inbox row holds and a message's raw protobuf doesn't, so the row stays for the
    // jawd that tallies them.
    if changes.iter().any(|c| matches!(c, Change::Amend(Amend { what: Amendment::Keep { kind }, .. }) if kind != "pin"))
    {
        return false;
    }
    // The row keeps the engine's whole event, which says more than the WebMessageInfo made from it.
    let id = web.key.id.as_deref().unwrap_or_default();
    let envelope = Packed::new(&row.payload);
    for change in &mut changes {
        if let Change::Message(m) = change
            && m.id == id
        {
            m.envelope = Some(envelope.clone());
        }
    }
    out.changes.extend(changes);
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
pub async fn inbox(
    store: Store,
    live: Shared,
    offered: mpsc::UnboundedSender<fetch::Work>,
    avatars: Avatars,
    merges: Merges,
) {
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
    let mut swept = false;
    let mut pause = RETRY_FIRST;
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
            if !swept {
                swept = true;
                let _ = offered.send(fetch::Work::Offered);
            }
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
        let from_before = last <= left;
        let count = rows.len();
        let me = live.lock().expect("live state").me.clone();
        let Ok(converted) = tokio::task::spawn_blocking(move || convert_rows(&rows, &me)).await else { continue };
        let applied = if from_before {
            store.apply(Lane::Background, converted.changes).await.map(|applied| applied.quarantined)
        } else {
            apply_live(&store, converted.changes).await
        };
        // Rows that couldn't be stored at all (a full disk, an I/O error) are read again after a
        // pause; a row whose own conversion failed is in quarantine and stays in the inbox.
        match applied {
            Ok(quarantined) => {
                after = last;
                pause = RETRY_FIRST;
                merges.poke();
                if from_before {
                    leftovers += count;
                }
                if quarantined > 0 {
                    eprintln!("jawd: {quarantined} inbox rows didn't convert and are kept in quarantine");
                }
            }
            Err(e) => {
                eprintln!("jawd: can't store what the inbox held, so it's tried again in {pause:?}: {e:#}");
                tokio::time::sleep(pause).await;
                pause = (pause * 2).min(RETRY_MOST);
                continue;
            }
        }
        for offer in converted.offers {
            let _ = offered.send(fetch::Work::Offer(offer));
        }
        if let Some(name) = converted.my_name
            && let Some(me) = live.lock().expect("live state").account.me.as_mut()
        {
            me.name = name;
        }
        for jid in converted.pictures {
            avatars.want(&jid);
        }
    }
}

/// How long the converter waits before reading rows again that it couldn't store, at first and
/// at most.
const RETRY_FIRST: std::time::Duration = std::time::Duration::from_millis(10);
const RETRY_MOST: std::time::Duration = std::time::Duration::from_secs(1);

/// Makes changes in the live lane, LIVE_CHANGES or so to a transaction, never splitting one inbox
/// row's changes from the removal of its row; says how many rows went to quarantine.
async fn apply_live(store: &Store, mut changes: Vec<Change>) -> anyhow::Result<usize> {
    let mut quarantined = 0;
    while !changes.is_empty() {
        let mut cut = changes.len().min(LIVE_CHANGES);
        while cut < changes.len() && !matches!(changes[cut - 1], Change::Drained(_)) {
            cut += 1;
        }
        let rest = changes.split_off(cut);
        quarantined += store.apply(Lane::Live, changes).await?.quarantined;
        changes = rest;
    }
    Ok(quarantined)
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
        let last = rows.last().map(|(chat, id, _)| (chat.clone(), id.clone()));
        let me = live.lock().expect("live state").me.clone();
        let Ok(changes) = tokio::task::spawn_blocking(move || history_rows(rows, &me)).await else { return };
        match store.apply(Lane::Background, changes).await {
            Ok(applied) => {
                if applied.quarantined > 0 {
                    eprintln!(
                        "jawd: {} messages of history didn't convert and are kept in quarantine",
                        applied.quarantined
                    );
                }
                after = last.unwrap_or_default();
                converted += count;
            }
            Err(e) => {
                eprintln!("jawd: can't store the history kept raw, so it's tried again in {RETRY_MOST:?}: {e:#}");
                tokio::time::sleep(RETRY_MOST).await;
            }
        }
    }
    if converted > 0 {
        let seconds = started.elapsed().as_secs_f64();
        eprintln!(
            "jawd: converted {converted} messages of history kept raw in {seconds:.1} s, {:.0} a second",
            converted as f64 / seconds.max(1e-6)
        );
    }
}

/// History rows as changes, each followed by its row's removal. A row that becomes a message keeps
/// its raw there; any other (an edit, a live location's last place, a status, an empty shell) is
/// kept whole in history_kept, since it can't be fetched again. A row that doesn't decode stays.
pub fn history_rows(rows: Vec<(String, String, Vec<u8>)>, me: &Me) -> Vec<Change> {
    let mut changes = Vec::new();
    for (chat, id, raw) in rows {
        match WebMessageInfo::decode(raw.as_slice()) {
            Ok(mut info) => {
                if info.key.remote_jid.as_deref() != Some(chat.as_str()) {
                    info.key.remote_jid = Some(chat.clone());
                }
                let converted = convert::message(&info, me, false);
                let its_row =
                    converted.iter().any(|c| matches!(c, Change::Message(m) if m.id == id && m.raw.is_some()));
                changes.extend(converted);
                if !its_row {
                    changes
                        .push(Change::HistoryKept { id: format!("history_raw {chat} {id}"), raw: Packed::new(&raw) });
                }
                changes.push(Change::Drained(Drained::History { chat, id }));
            }
            Err(e) => eprintln!("jawd: a message of history in {chat} doesn't decode, so it's kept raw: {e}"),
        }
    }
    changes
}
