//! View-once messages, opened once (see "Disappearing and view-once messages" in the protocol).
//! `view_once.open` has media download someone else's view-once message's file, ahead of what
//! downloads by itself, and once it's in place one synced commit sets the message to go at `until`,
//! a minute past its length, and puts the played receipt that tells WhatsApp it was opened in the
//! outbox; only then is the file named, in the answer. `view_once.close`, or the connection that
//! opened it ending, erases it at once, and a start erases those a stop left open, so it's seen
//! once however the interface stops. Windows never get its file (`store::read`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use jaw_proto::disappearing::{Opened, ViewOnce};
use jaw_proto::{Error, code};
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::{Inner, durably, internal, now_ms, outbox, query};
use crate::convert;
use crate::engine::pb;
use crate::store::expiry::{OPEN_MOST, Once};
use crate::store::{Change, Lane, unpack};

/// How long past its length an opened message waits for its interface to close it before it goes.
const LEFT_OPEN_MS: i64 = 60_000;

/// The view-once messages open now: by n, the connection that opened each, its ID and its answer.
#[derive(Default)]
pub(super) struct Open {
    open: Mutex<HashMap<i64, (u64, String, Value)>>,
}

impl Open {
    pub(super) fn count(&self) -> u64 {
        self.open.lock().expect("view-once messages open").len() as u64
    }

    /// Ends the openings of the messages erased since, by n and ID, as at their time or opened
    /// elsewhere: one of a copy folded into another since has its ID.
    pub(super) fn forget(&self, closed: Vec<(i64, String)>) {
        if closed.is_empty() {
            return;
        }
        self.open
            .lock()
            .expect("view-once messages open")
            .retain(|n, (_, id, _)| !closed.iter().any(|(gone, gone_id)| gone == n || gone_id == id));
    }

    /// Where message `n`, `id`, is open, as (its key, its connection): under its n, or, for one
    /// opened on `connection` before its copy was folded into it, under the copy's.
    fn opening(&self, n: i64, id: &str, connection: u64) -> Option<(i64, u64)> {
        let open = self.open.lock().expect("view-once messages open");
        open.get(&n).map(|(on, _, _)| (n, *on)).or_else(|| {
            open.iter()
                .find(|(_, (on, opened, _))| *on == connection && opened == id)
                .map(|(key, (on, _, _))| (*key, *on))
        })
    }

    /// Ends the openings of message `id` on `connection`, which is gone.
    fn gone(&self, connection: u64, id: &str) {
        self.open
            .lock()
            .expect("view-once messages open")
            .retain(|_, (on, opened, _)| *on != connection || opened != id);
    }
}

/// A message as opening it finds it.
struct Found {
    n: i64,
    chat: String,
    sender: Option<String>,
    mine: bool,
    view_once: bool,
    deleted: bool,
    kind: String,
    raw: Option<Vec<u8>>,
    detail: Option<String>,
    copy: bool,
}

const FOUND: &str = "SELECT m.n, c.id, j.jid, m.mine, m.view_once, m.deleted, m.kind, m.raw, m.detail,
        EXISTS (SELECT 1 FROM media md WHERE md.message = m.n AND md.variant = 'sd' AND md.sha256 IS NOT NULL)";
const IN_CHAT: &str = "FROM chats c JOIN messages m ON m.chat = c.n AND m.id = ?2
    LEFT JOIN person_jids j ON j.n = m.sender WHERE c.id = ?1";
const ANYWHERE: &str = "FROM chats c CROSS JOIN messages m ON m.chat = c.n AND m.id = ?1
    LEFT JOIN person_jids j ON j.n = m.sender LIMIT 2";
const PLAYED_ROW: &str = "SELECT n FROM outbox WHERE kind = 'played' AND target = ?1";

fn found(r: &rusqlite::Row) -> rusqlite::Result<Found> {
    Ok(Found {
        n: r.get(0)?,
        chat: r.get(1)?,
        sender: r.get(2)?,
        mine: r.get(3)?,
        view_once: r.get(4)?,
        deleted: r.get(5)?,
        kind: r.get(6)?,
        raw: r.get(7)?,
        detail: r.get(8)?,
        copy: r.get(9)?,
    })
}

/// Message `id`, in `chat` or wherever it is, as the actions on a message name one.
fn find(db: &Connection, id: &str, chat: Option<&str>) -> anyhow::Result<Result<Found, Error>> {
    let found: Vec<Found> = match chat {
        Some(chat) => db
            .prepare_cached(&format!("{FOUND} {IN_CHAT}"))?
            .query_row(params![chat, id], found)
            .optional()?
            .into_iter()
            .collect(),
        None => db
            .prepare_cached(&format!("{FOUND} {ANYWHERE}"))?
            .query_map([id], found)?
            .collect::<rusqlite::Result<_>>()?,
    };
    let mut found = found.into_iter();
    Ok(match (found.next(), found.next()) {
        (None, _) => Err(Error::new(code::NOT_FOUND, format!("jawd has no message {id}; one opened once is gone"))),
        (Some(_), Some(_)) => {
            let mut error =
                Error::new(code::INVALID_PARAMS, format!("message {id} is in more than one chat; say which"));
            error.data = Some(json!({ "param": "chat" }));
            Err(error)
        }
        (Some(found), None) => Ok(found),
    })
}

/// The file's length in seconds, as its row's media says, for how long it may stay open.
fn seconds(detail: Option<&str>) -> f64 {
    detail
        .and_then(|detail| serde_json::from_str::<Value>(detail).ok())
        .and_then(|detail| detail["media"]["seconds"].as_f64())
        .unwrap_or(0.0)
}

/// When a message opened at `now`, with its row's `detail`, goes if nothing closes it first: a
/// minute past its length.
fn until(now: i64, detail: Option<&str>) -> i64 {
    now + (seconds(detail) * 1000.0) as i64 + LEFT_OPEN_MS
}

/// `view_once.open`, for the interface on `connection`.
pub(super) async fn open(inner: &Arc<Inner>, asked: ViewOnce, connection: u64) -> Result<Value, Error> {
    let (id, chat) = (asked.message.clone(), asked.chat.clone());
    let found = query(inner, move |db| find(db, &id, chat.as_deref())).await??;
    if !found.view_once {
        return Err(Error::new(code::NOT_ALLOWED, "that message isn't one to open once; it shows as it is"));
    }
    if found.mine {
        return Err(Error::new(
            code::NOT_ALLOWED,
            "a view-once message you sent can't be opened here, as on your phone",
        ));
    }
    if found.deleted {
        return Err(Error::new(code::NOT_ALLOWED, "that message was deleted, so there's nothing to open"));
    }
    if !found.copy {
        return Err(Error::new(code::NOT_ALLOWED, "that message says no file to open"));
    }
    inner.open.forget(inner.store.take_closed());
    {
        let open = inner.open.open.lock().expect("view-once messages open");
        match open.get(&found.n) {
            Some((on, _, answer)) if *on == connection => return Ok(answer.clone()),
            Some(_) => return Err(Error::new(code::NOT_ALLOWED, "that message is open in another interface")),
            None if open.len() as i64 >= OPEN_MOST => {
                let mut error = Error::new(code::LIMIT, format!("{OPEN_MOST} view-once messages are open already"));
                error.data = Some(json!({ "limit": "open", "most": OPEN_MOST }));
                return Err(error);
            }
            None => {}
        }
    }
    let media = inner.media.as_ref().ok_or_else(|| Error::new(code::INTERNAL, "this jawd keeps no media"))?;
    let pictured = matches!(found.kind.as_str(), "image" | "video" | "gif" | "video_note");
    let file = media.open_file(found.n, pictured).await?;
    let message = found
        .raw
        .as_deref()
        .and_then(|packed| unpack(packed).ok())
        .and_then(|raw| WebMessageInfo::decode(raw.as_slice()).ok())
        .and_then(|info| info.message);
    let spans = message.as_ref().map_or_else(|| json!([]), convert::once_caption);
    let until = until(now_ms(), found.detail.as_deref());
    let group = !convert::is_person(&found.chat);
    let receipt = pb::MarkRead {
        chat: found.chat.clone(),
        sender: if group { found.sender.clone().unwrap_or_default() } else { String::new() },
        ids: vec![asked.message.clone()],
        at_ms: now_ms(),
        r#type: "played".into(),
    };
    let opened = Once::Opened {
        n: found.n,
        until,
        jid: found.chat.clone(),
        id: asked.message.clone(),
        receipt: receipt.encode_to_vec(),
    };
    let applied = durably(inner, vec![Change::Once(opened)], "keep that it was opened").await?;
    if applied.changed == 0 {
        return Err(Error::new(code::NOT_ALLOWED, "that message was opened already"));
    }
    let n = found.n;
    if let Some(row) =
        query(inner, move |db| Ok(db.prepare_cached(PLAYED_ROW)?.query_row([n], |r| r.get(0)).optional()?)).await?
    {
        outbox::queue(inner, row, found.chat.clone(), false);
    }
    let mut detail: Value = found.detail.as_deref().and_then(|d| serde_json::from_str(d).ok()).unwrap_or_default();
    let mut shown = detail["media"].take();
    if !shown.is_object() {
        shown = json!({});
    }
    if let (Some(small), Some(large)) = (&file.small, &file.large) {
        shown["previews"] = json!({ "256": small, "768": large });
    }
    let id = asked.message.clone();
    let answer = Opened {
        message: asked.message,
        kind: found.kind,
        path: file.path,
        spans,
        media: shown.as_object().cloned().unwrap_or_default(),
        until,
    };
    let answer = serde_json::to_value(answer).map_err(|e| internal("answer", e.into()))?;
    inner.open.open.lock().expect("view-once messages open").insert(n, (connection, id, answer.clone()));
    Ok(answer)
}

/// `view_once.close`, for the interface on `connection`.
pub(super) async fn close(inner: &Arc<Inner>, asked: ViewOnce, connection: u64) -> Result<Value, Error> {
    inner.open.forget(inner.store.take_closed());
    let (id, chat) = (asked.message.clone(), asked.chat.clone());
    let n = match query(inner, move |db| find(db, &id, chat.as_deref())).await? {
        Ok(found) => found.n,
        Err(e) if e.code == code::NOT_FOUND => {
            // Erased already, as at its time: its opening here ends with it.
            inner.open.gone(connection, &asked.message);
            return Ok(json!({}));
        }
        Err(e) => return Err(e),
    };
    let key = match inner.open.opening(n, &asked.message, connection) {
        Some((_, on)) if on != connection => {
            return Err(Error::new(code::NOT_ALLOWED, "that message is open in another interface"));
        }
        Some((key, _)) => key,
        None => return Ok(json!({})),
    };
    durably(inner, vec![Change::Once(Once::Closed { n })], "erase it").await?;
    inner.open.open.lock().expect("view-once messages open").remove(&key);
    Ok(json!({}))
}

/// The connection `connection` ended: what it opened is erased.
pub(super) async fn ended(inner: &Arc<Inner>, connection: u64) {
    let left: Vec<i64> = inner
        .open
        .open
        .lock()
        .expect("view-once messages open")
        .iter()
        .filter(|(_, (on, _, _))| *on == connection)
        .map(|(n, _)| *n)
        .collect();
    if left.is_empty() {
        return;
    }
    let closed = left.iter().map(|&n| Change::Once(Once::Closed { n })).collect();
    match inner.store.apply_durable(closed).await {
        Ok(_) => {
            let mut open = inner.open.open.lock().expect("view-once messages open");
            for n in left {
                open.remove(&n);
            }
        }
        // Each goes at its `until` instead.
        Err(e) => eprintln!("jawd: can't erase the view-once messages a closed connection had open: {e:#}"),
    }
}

/// A start erases what a stop left open, its connections gone.
pub(super) async fn restarted(inner: &Arc<Inner>) -> anyhow::Result<()> {
    inner.store.apply(Lane::Live, vec![Change::Once(Once::Restarted)]).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_opened_message_goes_a_minute_past_its_length_if_nothing_closes_it() {
        let video = r#"{"media":{"seconds":42.5,"w":640,"h":480}}"#;
        assert_eq!(until(1_000, Some(video)), 1_000 + 42_500 + 60_000);
        assert_eq!(until(1_000, Some(r#"{"media":{"w":640,"h":480}}"#)), 61_000, "a photo has no length");
        assert_eq!(until(1_000, None), 61_000);
    }
}
