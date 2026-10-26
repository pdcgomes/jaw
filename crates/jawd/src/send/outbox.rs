//! The outbox's lane. A message is committed, synced, with its outbox row before `send.text`
//! answers, and once it's committed the answer is a success; then it joins its chat's queue, which
//! sends one message at a time with the ID jawd made. As on the phone, a message waits, pending, for
//! as long as WhatsApp can't be reached; while it can, a send that fails on the way (the network,
//! the server busy) is tried again after a pause that doubles, without holding a place other chats
//! could send in. A refusal (a number not on WhatsApp, a group you've left, a message the server
//! won't take) fails it until `send.retry`, unless an earlier attempt may have got through: then it
//! waits, unconfirmed, for a receipt, and fails only if none comes within a day. A restart queues
//! what's left again, in order. An action on a message of yours that hasn't reached WhatsApp waits
//! for it, out of its chat's queue, and is queued again once it has (`release`).

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jaw_proto::send::{SendRetry, SendText, Sent};
use jaw_proto::{Error, code};
use prost::Message as _;
use rusqlite::OptionalExtension;
use serde_json::Value;
use tokio::sync::Notify;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_web::{WebMessageInfo, web_message_info};

use super::quote::{self, Mentioned, Quote};
use super::{Inner, TEXT_MOST, durably, engine, internal, no_chat, now_ms, query, retryable};
use crate::convert::{self, Me};
use crate::engine::{EngineError, pb};
use crate::feed;
use crate::store::outbox::{self, Enqueued, Outboxed, Step, Written};
use crate::store::{Applied, Change, Lane, Person, read};
use crate::work::{Next, Priority};

/// A chat to send to: its disappearing timer, the chat it is as the writer finds it (itself, or
/// the one it's being merged into), and its kind.
pub(super) const CHAT_TO_SEND: &str = "SELECT c.ephemeral, coalesce(m.keep, c.n), c.kind FROM chats c
    LEFT JOIN chat_merges m ON m.gone = c.n WHERE c.id = ?1";
pub(super) const ME: &str = "SELECT value FROM sync_state WHERE key = 'me'";

/// The longest key an interface may give a message.
const KEY_MOST: usize = 128;
/// How many attempts while WhatsApp can be reached, about a day's worth at the longest pause,
/// before a failure that might pass is taken as one that won't.
const ATTEMPTS_MOST: i64 = 1_500;
/// How long after a commit the held actions are looked at again, so a burst of commits is looked
/// at once.
const HELD_SETTLE: Duration = Duration::from_millis(50);

/// The actions waiting for their message to reach WhatsApp, by outbox row, each out of its chat's
/// queue until `release` queues it again; and what wakes `release` to look at them.
#[derive(Default)]
pub(super) struct Held {
    rows: Mutex<BTreeSet<i64>>,
    look: Arc<Notify>,
}

impl Held {
    fn hold(&self, outbox: i64) {
        self.rows.lock().expect("held actions").insert(outbox);
        self.look.notify_one();
    }

    pub(super) fn rows(&self) -> Vec<i64> {
        self.rows.lock().expect("held actions").iter().copied().collect()
    }

    fn let_go(&self, outbox: i64) {
        self.rows.lock().expect("held actions").remove(&outbox);
    }
}

/// Queues each held action again, oldest first, once its message has reached WhatsApp, or can't be
/// sent again. A message's outbox row goes by its answer, a receipt, a merge folding it or a
/// deletion for everyone cancelling it, each a commit, so it looks after each commit while any
/// action is held, and when sending says a send's outcome is kept, which may not write an event.
pub(super) fn release(inner: &Arc<Inner>) {
    let weak = Arc::downgrade(inner);
    let (mut committed, look) = (inner.store.committed(), inner.held.look.clone());
    tokio::spawn(async move {
        loop {
            let Some(inner) = weak.upgrade() else { return };
            let held = inner.held.rows();
            if !held.is_empty() {
                let rows = inner
                    .store
                    .reads()
                    .query(Priority::Background, move |db| {
                        held.into_iter()
                            .map(|outbox| Ok((outbox, outbox::outboxed_by_n(db, outbox)?)))
                            .collect::<anyhow::Result<Vec<_>>>()
                    })
                    .await;
                match rows {
                    Ok(rows) => {
                        for (outbox, row) in rows {
                            match row.filter(|row| row.state == "queued") {
                                Some(row) if row.held() => {}
                                Some(row) => {
                                    inner.held.let_go(outbox);
                                    queue(&inner, outbox, row.chat, false);
                                }
                                None => inner.held.let_go(outbox),
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("jawd: can't read the actions waiting for their messages, so it's tried again: {e:#}")
                    }
                }
            }
            let holding = !inner.held.rows().is_empty();
            drop(inner);
            if !holding {
                look.notified().await;
                continue;
            }
            tokio::select! {
                changed = committed.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    tokio::time::sleep(HELD_SETTLE).await;
                }
                () = look.notified() => {}
            }
        }
    });
}

/// A new message ID in whatsmeow's format for messages from this kind of device: `3EB0` and 18
/// upper-case hex digits, from 72 random bits.
pub(super) fn message_id() -> std::io::Result<String> {
    let mut bytes = [0u8; 9];
    // SAFETY: getentropy fills the buffer it's given, which may be up to 256 bytes.
    if unsafe { libc::getentropy(bytes.as_mut_ptr().cast(), bytes.len()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(bytes.iter().fold(String::from("3EB0"), |mut id, byte| {
        id.push_str(&format!("{byte:02X}"));
        id
    }))
}

/// The account's own JIDs as sync keeps them.
pub(super) fn me(db: &rusqlite::Connection) -> anyhow::Result<Me> {
    let me: Option<String> = db.prepare_cached(ME)?.query_row([], |r| r.get(0)).optional()?;
    Ok(Me::new(me.unwrap_or_default().split_whitespace().map(str::to_string)))
}

/// Whether a chat of `kind` has members to mention.
fn has_members(kind: &str) -> bool {
    matches!(kind, "group" | "community" | "announcement")
}

/// What `send.text` reads before it writes: the chat, the interface's key if it named a message
/// already, who you are, and the message replied to and the people mentioned, as they're sent.
struct Composed {
    chat: Option<(u32, i64, String)>,
    keyed: Option<(String, String)>,
    me: Me,
    quote: Option<Quote>,
    /// The people mentioned, as they're sent, or why one of them can't be.
    mentioned: Result<Mentioned, String>,
}

/// The answer for message `id` in `chat`, with its row if it can be read; a message that's committed
/// is answered as sent on its way whatever the read does.
async fn answer(inner: &Inner, chat: String, id: String) -> Sent {
    let message = id.clone();
    let row = inner
        .store
        .reads()
        .query(Priority::Interactive, move |db| {
            Ok(read::message(db, &chat, &message)?.map(|(row, _)| feed::message_row(row)))
        })
        .await;
    match row {
        Ok(row) => Sent { message: id, row },
        Err(e) => {
            eprintln!("jawd: {id} is on its way, but its row couldn't be read for the answer: {e:#}");
            Sent { message: id, row: None }
        }
    }
}

/// The protocol's error for a parameter it can't use, naming it.
pub(super) fn bad_param(param: &str, message: impl Into<String>) -> Error {
    let mut error = Error::new(code::INVALID_PARAMS, message);
    error.data = Some(serde_json::json!({ "param": param }));
    error
}

/// Refuses a key that isn't 1 to KEY_MOST bytes.
pub(super) fn check_key(key: &Option<String>) -> Result<(), Error> {
    match key {
        Some(key) if key.is_empty() || key.len() > KEY_MOST => {
            Err(Error::new(code::INVALID_PARAMS, format!("a key is 1 to {KEY_MOST} bytes")))
        }
        _ => Ok(()),
    }
}

pub(super) async fn send_text(inner: &Arc<Inner>, send: SendText) -> Result<Sent, Error> {
    if send.text.trim().is_empty() {
        return Err(Error::new(code::INVALID_PARAMS, "there's nothing to send"));
    }
    if send.text.chars().count() > TEXT_MOST {
        return Err(Error::new(code::LIMIT, format!("a message can be {TEXT_MOST} characters at most")));
    }
    check_key(&send.key)?;
    let (chat, key, reply, people, text) =
        (send.chat.clone(), send.key.clone(), send.reply.clone(), send.mentions.clone(), send.text.clone());
    let composed = query(inner, move |db| {
        let chat: Option<(u32, i64, String)> = db
            .prepare_cached(CHAT_TO_SEND)?
            .query_row([&chat], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        let keyed = key.as_deref().map(|key| outbox::keyed(db, key)).transpose()?.flatten();
        let me = me(db)?;
        let (mut quote, mut mentioned) = (None, Ok(Mentioned { jids: Vec::new(), text: text.clone() }));
        if let Some((_, n, kind)) = &chat {
            quote = reply.map(|id| quote::quoted(db, *n, &id, &me)).transpose()?;
            // As WhatsApp's phones do, mentions outside a group are dropped.
            if has_members(kind) && !people.is_empty() {
                mentioned = match quote::mentioned(db, *n, &people, &text)? {
                    Ok(mentioned) => Ok(mentioned),
                    Err(person) => Err(quote::not_a_member(db, *n, &person)?),
                };
            }
        }
        Ok(Composed { chat, keyed, me, quote, mentioned })
    })
    .await?;
    if let Some((chat, id)) = composed.keyed {
        return Ok(answer(inner, chat, id).await);
    }
    let Some((ephemeral, _, _)) = composed.chat else { return Err(no_chat(&send.chat)) };
    let quoted = match composed.quote {
        None => None,
        Some(Quote::Found(quoted)) => Some(*quoted),
        Some(Quote::Missing) => {
            let reply = send.reply.unwrap_or_default();
            return Err(Error::new(
                code::NOT_FOUND,
                format!("jawd has no message {reply} in {} to reply to", send.chat),
            ));
        }
        Some(Quote::Deleted) => return Err(Error::new(code::NOT_ALLOWED, "a deleted message can't be replied to")),
    };
    let mentioned = composed.mentioned.map_err(|why| bad_param("mentions", why))?;
    let id = message_id().map_err(|e| internal("make a message ID", e.into()))?;
    let at = now_ms();
    let raw = quote::text_message(&mentioned.text, ephemeral, quoted.as_ref(), &mentioned.jids);
    let web = WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(send.chat.clone()),
            from_me: Some(true),
            id: Some(id.clone()),
            participant: None,
        },
        message: Some(raw.clone()),
        message_timestamp: Some((at / 1000).max(0) as u64),
        status: Some(web_message_info::Status::Pending as i32),
        ..Default::default()
    };
    let Some(Change::Message(mut message)) =
        convert::message(&web, &composed.me, true).into_iter().find(|c| matches!(c, Change::Message(_)))
    else {
        return Err(Error::new(code::INTERNAL, "jawd couldn't make a message of that text"));
    };
    message.at = at;
    message.expires_at = None;
    let written = Written::default();
    let step = Step::Enqueue {
        message: Box::new(message),
        kind: "text",
        raw: raw.encode_to_vec(),
        key: send.key.clone(),
        written: written.clone(),
    };
    let _order = inner.enqueue.lock().await;
    durably(inner, vec![Change::Outbox(step)], "keep the message, so it isn't sent").await?;
    match written.take() {
        Some(Enqueued::Queued { id, chat, chat_n, outbox, .. }) => {
            let sent = answer(inner, chat.clone(), id).await;
            if let Some(outbox) = outbox {
                queue(inner, outbox, chat, false);
            }
            // Sending here reads the chat, as replying does, so its receipts are owed.
            if let Some(chat_n) = chat_n {
                super::reads::want(inner, chat_n);
            }
            Ok(sent)
        }
        Some(Enqueued::Full { waiting, failed }) => {
            let mut error = Error::new(
                code::LIMIT,
                format!(
                    "{waiting} messages of yours are waiting to send, {failed} of them failed; \
                    jawd takes more once some have gone or been retried"
                ),
            );
            error.data = Some(serde_json::json!({ "limit": "outbox", "waiting": waiting, "failed": failed }));
            Err(error)
        }
        Some(Enqueued::Gone | Enqueued::Many(_)) | None => {
            Err(Error::new(code::INTERNAL, "jawd kept nothing of the message, so it isn't sent"))
        }
    }
}

/// Sends outbox row `outbox` after `chat`'s earlier sends, `chat` being the JID jaw keeps the chat
/// under, so sends named by a number and by a LID share one queue. `resumed` is for a row left from
/// before jawd started, which may have gone already.
pub(super) fn queue(inner: &Arc<Inner>, outbox: i64, chat: String, resumed: bool) {
    let inner = inner.clone();
    inner.sends.clone().push_again(chat, move || attempt(inner.clone(), outbox, resumed));
}

/// Queues what the outbox held when jawd stopped, oldest first, so each chat's go in order, and
/// gives what waits for a receipt the rest of its time.
pub(super) async fn resume(inner: &Arc<Inner>) -> anyhow::Result<()> {
    let queued = inner.store.reads().query(Priority::Interactive, outbox::queued).await?;
    for (outbox, chat) in queued {
        queue(inner, outbox, chat, true);
    }
    let now = now_ms();
    for (outbox, until) in inner.store.reads().query(Priority::Interactive, outbox::unconfirmed).await? {
        let left = Duration::from_millis(until.saturating_sub(now).max(0) as u64).min(inner.timing.unconfirmed_for);
        unconfirmed(inner, outbox, left);
    }
    Ok(())
}

/// Fails outbox row `outbox` after `left`, if it's still waiting for a receipt by then; an action
/// failed so is said to every interface, as one refused at once is.
fn unconfirmed(inner: &Arc<Inner>, outbox: i64, left: Duration) {
    let inner = inner.clone();
    tokio::spawn(async move {
        tokio::time::sleep(left).await;
        let row = loop {
            match inner.store.reads().query(Priority::Interactive, move |db| outbox::outboxed_by_n(db, outbox)).await {
                Ok(row) => break row,
                Err(e) => {
                    eprintln!("jawd: can't read outbox row {outbox}, so it's read again: {e:#}");
                    tokio::time::sleep(inner.timing.retry_most).await;
                }
            }
        };
        let Some(row) = row.filter(|row| row.state == "unconfirmed") else { return };
        let error = "WhatsApp refused it when it may already have gone, and no receipt came".to_string();
        let applied = settle(&inner, vec![Change::Outbox(Step::NoReceipt { outbox, error })]).await;
        if applied.changed > 0 {
            super::actions::refused(&inner, &row.kind, "no receipt came to say it had gone the first time");
        }
    });
}

/// Whether a message has got at least as far as the server.
fn reached_server(status: Option<&str>) -> bool {
    matches!(status, Some("sent" | "delivered" | "read" | "played"))
}

/// The pause after `attempts` failed attempts.
fn pause(inner: &Inner, attempts: i64) -> Duration {
    let doubled = inner.timing.retry_first.saturating_mul(1 << attempts.clamp(0, 16));
    doubled.min(inner.timing.retry_most)
}

/// Tries to send outbox row `outbox` once. It reads the row again first, so a message sent, failed
/// or deleted meanwhile isn't sent, and nothing is tried while WhatsApp can't be reached. What it
/// waits for before the next try, it waits for without holding a place in the sends' lane.
async fn attempt(inner: Arc<Inner>, outbox: i64, resumed: bool) -> Next {
    let mut connected = inner.connected.subscribe();
    if !*connected.borrow_and_update() {
        return Next::After(Box::pin(async move {
            let _ = connected.wait_for(|connected| *connected).await;
        }));
    }
    let row = match inner.store.reads().query(Priority::Interactive, move |db| outbox::outboxed_by_n(db, outbox)).await
    {
        Ok(Some(row)) => row,
        Ok(None) => return Next::Done,
        Err(e) => {
            eprintln!("jawd: can't read outbox row {outbox}, so it's tried again: {e:#}");
            return Next::After(Box::pin(tokio::time::sleep(inner.timing.retry_most)));
        }
    };
    if row.state != "queued" {
        return Next::Done;
    }
    if row.withdrawn() {
        settle(&inner, vec![Change::Outbox(Step::Withdrawn { outbox })]).await;
        super::actions::withdrawn(&inner, &row.kind);
        return Next::Done;
    }
    if row.held() {
        inner.held.hold(outbox);
        return Next::Done;
    }
    if reached_server(row.status.as_deref()) {
        settle(&inner, vec![Change::Outbox(Step::Sent { outbox })]).await;
        return Next::Done;
    }
    // App state is the same however often it's sent, and no receipt ever says it arrived.
    let app_state = row.kind == "app_state";
    let may_have_gone = (resumed || row.attempts > 0) && !app_state;
    let command = if app_state {
        match pb::AppStatePatch::decode(row.raw.as_slice()) {
            Ok(patch) => pb::command::Kind::AppStatePatch(patch),
            Err(e) => {
                let error = format!("its app state doesn't decode: {e}");
                settle(&inner, vec![Change::Outbox(Step::Failed { outbox, error, next_at: None })]).await;
                return Next::Done;
            }
        }
    } else {
        pb::command::Kind::Send(pb::Send { chat: row.chat.clone(), id: row.id.clone(), raw: row.raw.clone() })
    };
    let failure = match engine(&inner, command).await {
        Ok(pb::reply::Kind::Sent(sent)) => {
            settle(&inner, sent_changes(&row, &sent)).await;
            return Next::Done;
        }
        Ok(pb::reply::Kind::Done(_)) if app_state => {
            settle(&inner, vec![Change::Outbox(Step::Sent { outbox })]).await;
            return Next::Done;
        }
        Ok(other) => {
            EngineError::Failed { message: format!("the engine answered a send with {other:?}"), retry: false }
        }
        Err(e) => e,
    };
    if may_have_gone {
        eprintln!("jawd: WhatsApp answered {} in {} sent again with: {failure}", row.id, row.chat);
    }
    let gives_up = !retryable(&failure) || row.attempts + 1 >= ATTEMPTS_MOST;
    let step = match (gives_up, may_have_gone) {
        (false, _) => {
            let next_at = now_ms() + pause(&inner, row.attempts + 1).as_millis() as i64;
            Step::Failed { outbox, error: failure.to_string(), next_at: Some(next_at) }
        }
        (true, true) => Step::Unconfirmed {
            outbox,
            error: failure.to_string(),
            until: now_ms() + inner.timing.unconfirmed_for.as_millis() as i64,
        },
        (true, false) => Step::Failed { outbox, error: failure.to_string(), next_at: None },
    };
    let unconfirming = matches!(step, Step::Unconfirmed { .. });
    let refused = matches!(step, Step::Failed { next_at: None, .. });
    settle(&inner, vec![Change::Outbox(step)]).await;
    if unconfirming {
        unconfirmed(&inner, outbox, inner.timing.unconfirmed_for);
    }
    if gives_up {
        eprintln!("jawd: WhatsApp refused {} in {}: {failure}", row.id, row.chat);
        if refused {
            super::actions::refused(&inner, &row.kind, &failure.to_string());
        }
        return Next::Done;
    }
    let pause = pause(&inner, row.attempts + 1);
    Next::After(Box::pin(async move {
        tokio::select! {
            _ = tokio::time::sleep(pause) => {}
            _ = connected.changed() => {}
        }
    }))
}

/// What the server's answer changes: the message is sent; and when WhatsApp delivered it to another
/// of the person's JIDs, such as their LID for a phone number, the two are joined, so receipts
/// addressed to either find the chat.
fn sent_changes(row: &Outboxed, sent: &pb::Sent) -> Vec<Change> {
    let mut changes = vec![Change::Outbox(Step::Sent { outbox: row.outbox })];
    let to = convert::user_jid(&sent.chat);
    if convert::is_person(&to) && convert::is_person(&row.chat) && to != row.chat {
        changes.push(Change::Person(Person { jids: vec![row.chat.clone(), to], ..Person::default() }));
    }
    changes
}

/// Keeps what a send came to, trying again until it's kept: a message the server has mustn't be
/// sent again for want of a commit, and one that failed must be counted as tried, or a later
/// refusal of what may have gone would fail it outright. The actions held for it are looked at
/// again.
async fn settle(inner: &Inner, changes: Vec<Change>) -> Applied {
    let mut pause = inner.timing.retry_first;
    loop {
        match inner.store.apply(Lane::Live, changes.clone()).await {
            Ok(applied) => {
                inner.held.look.notify_one();
                return applied;
            }
            Err(e) => {
                eprintln!("jawd: can't keep what became of a send, so it's tried again in {pause:?}: {e:#}");
                tokio::time::sleep(pause).await;
                pause = (pause * 2).min(inner.timing.retry_most);
            }
        }
    }
}

/// Queues a failed message of yours again; one still on its way or waiting for a receipt has
/// nothing to retry, and one deleted for everyone, or being, isn't sent again.
pub(super) async fn retry(inner: &Arc<Inner>, retry: SendRetry) -> Result<Value, Error> {
    let id = retry.message.clone();
    let found = query(inner, move |db| {
        let Some(row) = outbox::outboxed_by_id(db, &id)? else { return Ok(None) };
        let retryable = outbox::retryable(db, row.outbox)?;
        Ok(Some((row, retryable)))
    })
    .await?;
    let Some((row, retryable)) = found.filter(|(row, _)| !outbox::is_action(&row.kind)) else {
        return Err(Error::new(
            code::NOT_FOUND,
            format!("jawd has nothing of yours waiting to send as {}", retry.message),
        ));
    };
    let done = Value::Object(Default::default());
    if row.state != "failed" {
        return Ok(done);
    }
    if reached_server(row.status.as_deref()) {
        durably(inner, vec![Change::Outbox(Step::Sent { outbox: row.outbox })], "keep that it was sent").await?;
        return Ok(done);
    }
    if retryable == Some(false) {
        return Err(Error::new(code::NOT_ALLOWED, "it's being deleted for everyone, so it isn't sent again"));
    }
    let applied =
        durably(inner, vec![Change::Outbox(Step::Retry { outbox: row.outbox })], "queue the message again").await?;
    if applied.changed > 0 {
        queue(inner, row.outbox, row.chat, false);
    }
    Ok(done)
}
