//! `send.media`: files sent to a chat, a message each, in the order given, each with its caption;
//! several photos and videos as an album, as WhatsApp's phones send one; the first answering
//! `reply`; and in HD, each photo and video that has more to give than SD keeps followed by its HD
//! copy, as WhatsApp's dual upload sends it. A copy of each file is kept in jaw's folder first,
//! cloned where the disk can, so nothing depends on the file you picked staying as it was; then
//! every message is written, pending, with its outbox row and the copies to make of its file, in one
//! synced commit, before it answers, so they're sent however jawd stops. Media makes and uploads the
//! copies in its lanes, and each message goes once its file is uploaded, in its chat's order.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use jaw_media::{Kind, Probe};
use jaw_proto::media::{AttachmentKind, SendMedia, SentMedia, Variant};
use jaw_proto::{Error, code};
use prost::Message as _;
use rusqlite::OptionalExtension;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::{WebMessageInfo, web_message_info};

use super::compose;
use super::outbox::{CHAT_TO_SEND, bad_param, check_key, me, message_id, queue};
use super::quote::{self, Quote};
use super::{Inner, durably, internal, no_chat, now_ms, query};
use crate::convert;
use crate::feed;
use crate::media::outgoing::{KEPT_MOST, KeptFile, refusal};
use crate::store::outbox::{Attached, Enqueued, Kept, Step, Written, forwarded_by_key};
use crate::store::{Change, outgoing, read};

/// The most files one `send.media` takes, as the phones let you pick.
pub(super) const FILES_MOST: usize = 30;
/// The longest caption, in characters, as a message's text.
const CAPTION_MOST: usize = 1_024;

/// What a file goes as, from what was asked and what it is: a photo, a video, a GIF, an audio
/// file, or anything as a document, which goes as it is.
fn goes_as(asked: Option<AttachmentKind>, probe: &Probe) -> Option<AttachmentKind> {
    use AttachmentKind as As;
    Some(match (asked, probe.kind) {
        (None | Some(As::Image), Kind::Photo | Kind::Sticker) => As::Image,
        (None | Some(As::Video), Kind::Video) | (Some(As::Video), Kind::Gif) => As::Video,
        (None | Some(As::Gif), Kind::Gif) | (Some(As::Gif), Kind::Video) => As::Gif,
        (None | Some(As::Audio), Kind::Audio | Kind::Voice) => As::Audio,
        (Some(As::Document), _) | (None, Kind::Document | Kind::AnimatedSticker) => As::Document,
        _ => return None,
    })
}

fn kind_name(kind: AttachmentKind) -> &'static str {
    match kind {
        AttachmentKind::Image => "image",
        AttachmentKind::Video => "video",
        AttachmentKind::Gif => "gif",
        AttachmentKind::Audio => "audio",
        AttachmentKind::Document => "document",
    }
}

/// What jaw-media takes a file going as `kind` for.
fn media_kind(kind: AttachmentKind) -> Kind {
    match kind {
        AttachmentKind::Image => Kind::Photo,
        AttachmentKind::Video => Kind::Video,
        AttachmentKind::Gif => Kind::Gif,
        AttachmentKind::Audio => Kind::Audio,
        AttachmentKind::Document => Kind::Document,
    }
}

/// Where a file to send is: a whole path, or one from your home folder with `~/`.
fn path_of(path: &str, index: usize) -> Result<PathBuf, Error> {
    let found = match path.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME").map(|home| PathBuf::from(home).join(rest)),
        None => Some(PathBuf::from(path)).filter(|p| p.is_absolute()),
    };
    found.ok_or_else(|| {
        let mut error = Error::new(code::INVALID_PARAMS, format!("{path} isn't a whole path, or one starting ~/"));
        error.data = Some(serde_json::json!({ "param": "files", "file": index, "path": path }));
        error
    })
}

/// The extension jaw's copy of a file keeps, from its name, so another program opening it knows it.
fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| (1..=8).contains(&e.len()) && e.bytes().all(|b| b.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "bin".into())
}

/// The chat and what `send.media` reads before it writes.
struct Composed {
    chat: Option<(u32, i64, String)>,
    me: convert::Me,
    quote: Option<Quote>,
    kept_bytes: u64,
}

/// A file planned: what it goes as, jaw's copy of it, and its messages' IDs.
struct Planned {
    kind: AttachmentKind,
    kept: KeptFile,
    name: String,
    caption: Option<String>,
    id: String,
    sd: jaw_media::Plan,
    hd: Option<(String, jaw_media::Plan)>,
}

pub(super) async fn send_media(inner: &Arc<Inner>, send: SendMedia) -> Result<SentMedia, Error> {
    let Some(media) = inner.media.clone() else {
        return Err(Error::new(code::INTERNAL, "this jawd keeps no media, so it can't send files"));
    };
    check_key(&send.key)?;
    if send.files.is_empty() {
        return Err(bad_param("files", "there's nothing to send"));
    }
    if send.files.len() > FILES_MOST {
        let mut error = Error::new(code::LIMIT, format!("jaw sends {FILES_MOST} files at most at once"));
        error.data = Some(serde_json::json!({ "limit": "files", "most": FILES_MOST }));
        return Err(error);
    }
    if let Some(caption) =
        send.files.iter().filter_map(|f| f.caption.as_deref()).find(|c| c.chars().count() > CAPTION_MOST)
    {
        return Err(Error::new(
            code::LIMIT,
            format!("a caption can be {CAPTION_MOST} characters at most, not {}", caption.chars().count()),
        ));
    }
    if let Some(key) = send.key.clone() {
        let files = send.files.len();
        if let Some(ids) = query(inner, move |db| forwarded_by_key(db, &key)).await? {
            return Ok(answer(inner, &send.chat, ids, files).await);
        }
    }
    let mut paths = Vec::with_capacity(send.files.len());
    for (i, file) in send.files.iter().enumerate() {
        paths.push(path_of(&file.path, i)?);
    }
    let (chat, reply) = (send.chat.clone(), send.reply.clone());
    let composed = query(inner, move |db| {
        let chat: Option<(u32, i64, String)> = db
            .prepare_cached(CHAT_TO_SEND)?
            .query_row([&chat], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        let me = me(db)?;
        let quote = match (&chat, reply) {
            (Some((_, n, _)), Some(id)) => Some(quote::quoted(db, *n, &id, &me)?),
            _ => None,
        };
        Ok(Composed { chat, me, quote, kept_bytes: outgoing::kept(db)?.1 })
    })
    .await?;
    let Some((ephemeral, _, _)) = composed.chat else { return Err(no_chat(&send.chat)) };
    let quoted = match composed.quote {
        None => None,
        Some(Quote::Found(quoted)) => Some(*quoted),
        Some(Quote::Missing) => {
            let reply = send.reply.clone().unwrap_or_default();
            return Err(Error::new(
                code::NOT_FOUND,
                format!("jawd has no message {reply} in {} to reply to", send.chat),
            ));
        }
        Some(Quote::Deleted) => return Err(Error::new(code::NOT_ALLOWED, "a deleted message can't be replied to")),
    };
    let ids: Vec<String> = (0..send.files.len())
        .map(|_| message_id())
        .collect::<std::io::Result<_>>()
        .map_err(|e| internal("make a message ID", e.into()))?;
    let keeping: Vec<(String, PathBuf)> =
        ids.iter().zip(&paths).map(|(id, path)| (format!("{id}.{}", extension(path)), path.clone())).collect();
    let kept = media.keep(keeping).await.map_err(|(i, why)| refusal(i, &send.files[i].path, why))?;
    let incoming: u64 = kept.iter().map(|k| k.bytes).sum();
    if composed.kept_bytes + incoming > KEPT_MOST {
        media.unkeep(&kept);
        let mut error = Error::new(
            code::LIMIT,
            format!(
                "jaw is still making {} MB of files to send; it takes more once some have gone",
                composed.kept_bytes / 1_000_000
            ),
        );
        error.data =
            Some(serde_json::json!({ "limit": "media", "waiting_bytes": composed.kept_bytes, "most": KEPT_MOST }));
        return Err(error);
    }
    let hd = send.quality == Some(Variant::Hd);
    let all = kept.clone();
    let mut planned = Vec::with_capacity(kept.len());
    for (i, ((file, path), (kept, id))) in send.files.iter().zip(&paths).zip(kept.into_iter().zip(ids)).enumerate() {
        let Some(kind) = goes_as(file.kind, &kept.probe) else {
            media.unkeep(&all);
            let mut error = Error::new(
                code::INVALID_PARAMS,
                format!("{} can't go as {}", file.path, file.kind.map_or("anything", kind_name)),
            );
            error.data = Some(serde_json::json!({ "param": "files", "file": i, "path": file.path }));
            return Err(error);
        };
        let mut probe = kept.probe.clone();
        probe.kind = media_kind(kind);
        let plans = jaw_media::plan(&probe);
        let hd = match (hd, plans.hd, kind) {
            (true, Some(plan), AttachmentKind::Image | AttachmentKind::Video) => Some((String::new(), plan)),
            _ => None,
        };
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        planned.push(Planned { kind, kept, name, caption: file.caption.clone(), id, sd: plans.sd, hd });
    }
    let all_kept = all;
    match compose_and_write(inner, &send, planned, ephemeral, quoted, &composed.me).await {
        Ok((ids, album, written)) => {
            let enqueued = match written {
                Some(Enqueued::Many(all)) => all,
                Some(Enqueued::Full { waiting, failed }) => {
                    media.unkeep(&all_kept);
                    return Err(super::actions::full(waiting, failed));
                }
                _ => {
                    media.unkeep(&all_kept);
                    return Err(Error::new(code::INTERNAL, "jawd kept nothing of the files, so they aren't sent"));
                }
            };
            let mut chat_n = None;
            let mut answered = Vec::with_capacity(enqueued.len());
            let mut queued = Vec::new();
            for one in &enqueued {
                if let Enqueued::Queued { id, chat, chat_n: n, outbox, .. } = one {
                    if let Some(outbox) = outbox {
                        queue(inner, *outbox, chat.clone(), false);
                        queued.push((*outbox, chat.clone()));
                    }
                    chat_n = chat_n.or(*n);
                    answered.push(id.clone());
                }
            }
            // Each HD copy is queued too, after its message, which it waits to go after.
            let hd = query(inner, move |db| {
                let mut hd = Vec::new();
                for (outbox, chat) in queued {
                    hd.extend(crate::store::outbox::hd_of(db, outbox)?.into_iter().map(|n| (n, chat.clone())));
                }
                Ok(hd)
            })
            .await?;
            for (outbox, chat) in hd {
                queue(inner, outbox, chat, false);
            }
            if answered != album.iter().cloned().chain(ids.iter().cloned()).collect::<Vec<_>>() {
                // An earlier request with the same key wrote these: the files kept now aren't named.
                media.unkeep(&all_kept);
            }
            media.send_now();
            if let Some(chat_n) = chat_n {
                super::reads::want(inner, chat_n);
            }
            Ok(answer(inner, &send.chat, answered, send.files.len()).await)
        }
        Err(e) => {
            media.unkeep(&all_kept);
            Err(e)
        }
    }
}

/// The messages for `planned`, made and written in one synced commit: the album's first, when the
/// photos and videos make one, then each file's, then each HD copy's. Says the files' IDs, the
/// album's, and what the commit came to.
async fn compose_and_write(
    inner: &Arc<Inner>,
    send: &SendMedia,
    mut planned: Vec<Planned>,
    ephemeral: u32,
    quoted: Option<quote::Quoted>,
    me: &convert::Me,
) -> Result<(Vec<String>, Option<String>, Option<Enqueued>), Error> {
    let chat = send.chat.clone();
    let key = |id: &str| MessageKey {
        remote_jid: Some(chat.clone()),
        from_me: Some(true),
        id: Some(id.into()),
        participant: None,
    };
    let context = |first: bool| {
        let quote = quoted.as_ref().filter(|_| first);
        (ephemeral > 0 || quote.is_some()).then(|| e2e::ContextInfo {
            expiration: (ephemeral > 0).then_some(ephemeral),
            stanza_id: quote.map(|q| q.id.clone()),
            participant: quote.map(|q| q.participant.clone()).filter(|p| !p.is_empty()),
            quoted_message: quote.map(|q| Box::new(q.message.clone())),
            ..Default::default()
        })
    };
    let (photos, videos) = planned.iter().fold((0u32, 0u32), |(p, v), f| match f.kind {
        AttachmentKind::Image => (p + 1, v),
        AttachmentKind::Video => (p, v + 1),
        _ => (p, v),
    });
    let at = now_ms();
    let mut sending = Vec::with_capacity(planned.len() + 1);
    let album = if photos + videos >= 2 {
        let id = message_id().map_err(|e| internal("make a message ID", e.into()))?;
        let timer = (ephemeral > 0).then(|| e2e::ContextInfo { expiration: Some(ephemeral), ..Default::default() });
        let raw = compose::album_message(photos, videos, timer);
        sending.push(Attached {
            message: row(&chat, &id, &raw, at, me)?,
            kind: "album",
            raw: raw.encode_to_vec(),
            file: None,
            hd: None,
        });
        Some(id)
    } else {
        None
    };
    let album_key = album.as_deref().map(key);
    for (nth, file) in planned.iter_mut().enumerate() {
        let in_album = matches!(file.kind, AttachmentKind::Image | AttachmentKind::Video);
        let raw = compose::file_message(
            file.kind,
            &file.sd,
            &file.kept.probe,
            &file.name,
            file.caption.as_deref(),
            context(nth == 0),
            album_key.as_ref().filter(|_| in_album),
        );
        let hd = match file.hd.as_mut() {
            Some((id, plan)) => {
                *id = message_id().map_err(|e| internal("make a message ID", e.into()))?;
                let mut hd = compose::hd_message(&raw, plan, &key(&file.id));
                if let (Some(hd), Some(expiration)) = (hd.as_mut(), (ephemeral > 0).then_some(ephemeral)) {
                    hd_expires(hd, expiration);
                }
                hd.map(|hd| (id.clone(), hd.encode_to_vec()))
            }
            None => None,
        };
        let mut message = row(&chat, &file.id, &raw, at, me)?;
        message.media = None;
        sending.push(Attached {
            message,
            kind: kind_name(file.kind),
            raw: raw.encode_to_vec(),
            file: Some(Kept { path: file.kept.path.clone(), bytes: file.kept.bytes }),
            hd,
        });
    }
    let written = Written::default();
    let step = Step::Attach { sending, key: send.key.clone(), written: written.clone() };
    let _order = inner.enqueue.lock().await;
    durably(inner, vec![Change::Outbox(step)], "keep the files, so they aren't sent").await?;
    Ok((planned.into_iter().map(|p| p.id).collect(), album, written.take()))
}

/// The chat's disappearing timer on an HD copy's own message, as on its SD one.
fn hd_expires(hd: &mut e2e::Message, expiration: u32) {
    let Some(child) = hd.associated_child_message.as_deref_mut().and_then(|c| c.message.as_deref_mut()) else { return };
    let context = Some(Box::new(e2e::ContextInfo { expiration: Some(expiration), ..Default::default() }));
    if let Some(image) = child.image_message.as_deref_mut() {
        image.context_info = context;
    } else if let Some(video) = child.video_message.as_deref_mut() {
        video.context_info = context;
    }
}

/// The row a message of yours that says `raw` is, pending, as every window shows it.
fn row(chat: &str, id: &str, raw: &e2e::Message, at: i64, me: &convert::Me) -> Result<crate::store::Message, Error> {
    let web = WebMessageInfo {
        key: MessageKey { remote_jid: Some(chat.into()), from_me: Some(true), id: Some(id.into()), participant: None },
        message: Some(raw.clone()),
        message_timestamp: Some((at / 1000).max(0) as u64),
        status: Some(web_message_info::Status::Pending as i32),
        ..Default::default()
    };
    let Some(Change::Message(mut message)) =
        convert::message(&web, me, true).into_iter().find(|c| matches!(c, Change::Message(_)))
    else {
        return Err(Error::new(code::INTERNAL, "jawd couldn't make a message of that file"));
    };
    message.at = at;
    message.expires_at = None;
    Ok(message)
}

/// The answer for the messages `ids` in `chat`, the album's first when there's one more than there
/// are files, with their rows as they can be read.
async fn answer(inner: &Inner, chat: &str, ids: Vec<String>, files: usize) -> SentMedia {
    let (album, messages) = match ids.len() > files {
        true => (ids.first().cloned(), ids[1..].to_vec()),
        false => (None, ids),
    };
    let (chat, wanted) = (chat.to_string(), album.iter().chain(&messages).cloned().collect::<Vec<_>>());
    let rows = inner
        .store
        .reads()
        .query(crate::work::Priority::Interactive, move |db| {
            let mut rows = Vec::new();
            for id in &wanted {
                if let Some((row, _)) = read::message(db, &chat, id)? {
                    rows.push(feed::message_row(row));
                }
            }
            Ok(rows)
        })
        .await
        .unwrap_or_default();
    SentMedia { messages, album, rows }
}
