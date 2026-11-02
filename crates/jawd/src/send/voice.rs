//! `voice.send`'s message: a stopped recording, sent to the chat it was recorded in, or the one
//! that chat has merged into since, with that chat's disappearing timer, as WhatsApp's voice
//! message, a message of yours, as `send.media` sends a file. jaw's copy of the recording is kept
//! in media's folder for files to send first, cloned where the disk can; then the message is
//! written, pending, with its outbox row and the copy to make of its file, in one synced commit,
//! before it's answered, so it's sent however jawd stops. Media makes the copy, which goes as it is,
//! and uploads it, and the message goes once it's uploaded, in its chat's order. A voice message
//! carries no caption, so the chat's draft stays as it is.

use std::path::PathBuf;
use std::sync::Arc;

use jaw_proto::voice::VoiceSent;
use jaw_proto::{Error, code};
use prost::Message as _;
use rusqlite::OptionalExtension;
use wa_proto::wa_web_protobufs_e2e as e2e;

use super::compose;
use super::media::row;
use super::outbox::{CHAT_TO_SEND, check_key, me, queue};
use super::{Inner, durably, internal, no_chat, now_ms, query};
use crate::feed;
use crate::media::outgoing::{making, refusal};
use crate::store::outbox::{Attached, Enqueued, Kept, Step, Written, forwarded_by_key};
use crate::store::{Change, outgoing, read};

/// A stopped recording to send: the chat it was recorded in, the ID made for its message, its
/// file, how long it is, its waveform, and the interface's key for it.
pub(crate) struct Note {
    pub chat: String,
    pub id: String,
    pub path: PathBuf,
    pub seconds: f64,
    pub waveform: Vec<u8>,
    pub key: Option<String>,
}

/// The message a key named within a day, as it is now, if it named one.
pub(super) async fn by_key(inner: &Arc<Inner>, key: &str) -> Result<Option<VoiceSent>, Error> {
    check_key(&Some(key.to_string()))?;
    let looked = key.to_string();
    let Some(ids) = query(inner, move |db| forwarded_by_key(db, &looked)).await? else { return Ok(None) };
    let Some(id) = ids.into_iter().next() else { return Ok(None) };
    Ok(Some(answer(inner, None, id).await))
}

pub(super) async fn send_voice(inner: &Arc<Inner>, note: Note) -> Result<VoiceSent, Error> {
    let Some(media) = inner.media.clone() else {
        return Err(Error::new(code::INTERNAL, "this jawd keeps no media, so it can't send voice messages"));
    };
    check_key(&note.key)?;
    if let Some(key) = note.key.as_deref()
        && let Some(sent) = by_key(inner, key).await?
    {
        return Ok(sent);
    }
    // A recording's chat is fixed as it starts, so the chat it goes to is the one that's become of
    // it, as the writer finds a message's: the same, or, merged since, the chat kept for its person.
    let recorded = note.chat.clone();
    let Some(chat) = query(inner, move |db| read::chat_named(db, &recorded)).await? else {
        return Err(no_chat(&note.chat));
    };
    let _place = super::composing(inner, &[chat.as_str()]).await;
    let named = chat.clone();
    let (found, me, kept_bytes) = query(inner, move |db| {
        let found: Option<(u32, i64, String)> = db
            .prepare_cached(CHAT_TO_SEND)?
            .query_row([&named], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        Ok((found, me(db)?, outgoing::kept(db)?.1))
    })
    .await?;
    let Some((ephemeral, _, _)) = found else { return Err(no_chat(&note.chat)) };
    let shown = note.path.display().to_string();
    let kept = media
        .keep(vec![(format!("{}.ogg", note.id), note.path.clone())], kept_bytes)
        .await
        .map_err(|(i, why)| refusal(i, &shown, why))?;
    let file = kept.first().map(|k| Kept { path: k.path.clone(), bytes: k.bytes });
    let timer = (ephemeral > 0).then(|| e2e::ContextInfo { expiration: Some(ephemeral), ..Default::default() });
    let raw = compose::voice_message(note.seconds, &note.waveform, timer);
    let written = Written::default();
    let message = match row(&chat, &note.id, &raw, now_ms(), &me) {
        Ok(mut message) => {
            message.media = None;
            message
        }
        Err(e) => {
            media.unkeep(&kept);
            return Err(e);
        }
    };
    let attached = Attached { message, kind: "voice", raw: raw.encode_to_vec(), file, hd: None, typed: false };
    let step = Step::Attach { sending: vec![attached], key: note.key.clone(), written: written.clone() };
    let committed = {
        let _order = inner.enqueue.lock().await;
        durably(inner, vec![Change::Outbox(step)], "keep the voice message, so it isn't sent").await
    };
    if let Err(e) = committed {
        media.unkeep(&kept);
        return Err(e);
    }
    let enqueued = match written.take() {
        Some(Enqueued::Many(all)) => all,
        Some(Enqueued::Full { waiting, failed }) => {
            media.unkeep(&kept);
            return Err(super::actions::full(waiting, failed));
        }
        Some(Enqueued::Making { waiting }) => {
            media.unkeep(&kept);
            return Err(making(waiting));
        }
        _ => {
            media.unkeep(&kept);
            return Err(Error::new(code::INTERNAL, "jawd kept nothing of the voice message, so it isn't sent"));
        }
    };
    let Some(Enqueued::Queued { id, chat, chat_n, outbox, .. }) = enqueued.into_iter().next() else {
        media.unkeep(&kept);
        return Err(internal("send the voice message", anyhow::anyhow!("the outbox said nothing of it")));
    };
    if let Some(outbox) = outbox {
        queue(inner, outbox, chat.clone(), false);
    }
    if id != note.id {
        // An earlier request with the same key wrote this one: the copy kept now isn't named.
        media.unkeep(&kept);
    }
    media.send_now();
    if let Some(chat_n) = chat_n {
        super::reads::want(inner, chat_n);
    }
    Ok(answer(inner, Some(&chat), id).await)
}

/// The chat message ?1 is in, by its JID.
const CHAT_OF: &str = "SELECT c.id FROM messages m JOIN chats c ON c.n = m.chat WHERE m.n = ?1";

/// The answer for message `id`, with its row as it can be read, in `chat` or wherever it is.
async fn answer(inner: &Inner, chat: Option<&str>, id: String) -> VoiceSent {
    let (chat, wanted) = (chat.map(str::to_string), id.clone());
    let row = inner
        .store
        .reads()
        .query(crate::work::Priority::Interactive, move |db| {
            let chat = match chat {
                Some(chat) => Some(chat),
                None => match outgoing::message_n(db, &wanted, None)? {
                    Some(n) => db.prepare_cached(CHAT_OF)?.query_row([n], |r| r.get(0)).optional()?,
                    None => None,
                },
            };
            let Some(chat) = chat else { return Ok(None) };
            Ok(read::message(db, &chat, &wanted)?.map(|(row, _)| feed::message_row(row)))
        })
        .await
        .ok()
        .flatten();
    VoiceSent { message: id, row }
}
