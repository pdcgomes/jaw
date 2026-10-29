use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;

use super::*;
use crate::convert::Me;
use crate::store::write::Packed;
use crate::store::{Applied, Lane, Message, Store, schema};
use crate::work::{Priority, Report};

const ANA: &str = "351910000002@s.whatsapp.net";
const ME: &str = "351910000001@s.whatsapp.net";
const AT: i64 = 1_792_000_000_000;

/// The ID of a message whose conversion panics, as `copied` does for it in tests.
pub(super) const PANICS: &str = "3EB0PANICS";
/// The ID of a message whose conversion fails once its copy is written, as `copied` does for it in
/// tests.
pub(super) const FAILS: &str = "3EB0FAILS";
/// The ID of a message whose copy fills the disk once it's written, the first time only.
pub(super) const FILLS: &str = "3EB0FILLS";
pub(super) static FILLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A message as a jawd before media in kept it, for `before_media`.
pub(crate) struct Before {
    /// Its raw protobuf.
    pub info: WebMessageInfo,
    /// What its row shows, when that isn't what `info` converts to now: the photo inside an HD copy
    /// a jawd before media in kept as a message of its own.
    pub shown_as: Option<WebMessageInfo>,
    pub deleted: bool,
}

impl Before {
    pub(crate) fn kept(info: WebMessageInfo) -> Before {
        Before { info, shown_as: None, deleted: false }
    }
}

/// A jaw.db at `version`, from before migration 20, holding `messages` as a jawd of that version
/// stored them, you being `ME`: each a row as it converted, with its raw protobuf, and no copy of its
/// file, since no jawd before media in wrote one. Says each message's n, in order.
pub(crate) fn before_media(home: &Path, version: usize, messages: &[Before]) -> Vec<i64> {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    for migration in &schema::MIGRATIONS[..version] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", version as i64).unwrap();
    db.execute("INSERT INTO sync_state (key, value) VALUES ('me', ?1)", [ME]).unwrap();
    let me = Me::new([ME.to_string()]);
    let mut ns = Vec::new();
    for (i, before) in messages.iter().enumerate() {
        let shown = before.shown_as.as_ref().unwrap_or(&before.info);
        let id = shown.key.id.clone().unwrap();
        let row: Message = convert::message(shown, &me, false)
            .into_iter()
            .find_map(|change| match change {
                Change::Message(message) if message.id == id => Some(message),
                _ => None,
            })
            .expect("a message of its own");
        let kind = if row.chat.ends_with("@g.us") { "group" } else { "direct" };
        db.execute("INSERT INTO chats (id, kind, v) VALUES (?1, ?2, 0) ON CONFLICT (id) DO NOTHING", [&row.chat, kind])
            .unwrap();
        let chat: i64 = db.query_row("SELECT n FROM chats WHERE id = ?1", [&row.chat], |r| r.get(0)).unwrap();
        let sender = row.sender.as_deref().map(|jid| {
            let known: Option<i64> =
                db.query_row("SELECT n FROM person_jids WHERE jid = ?1", [jid], |r| r.get(0)).optional().unwrap();
            known.unwrap_or_else(|| {
                let person: i64 = db
                    .query_row("INSERT INTO people (name, v) VALUES (?1, 0) RETURNING n", [jid], |r| r.get(0))
                    .unwrap();
                db.query_row(
                    "INSERT INTO person_jids (jid, person) VALUES (?1, ?2) RETURNING n",
                    params![jid, person],
                    |r| r.get(0),
                )
                .unwrap()
            })
        });
        let n = i as i64 + 1;
        let summary = (row.text.as_ref() != Some(&row.summary)).then(|| row.summary.clone());
        db.execute(
            "INSERT INTO messages (n, chat, id, at, t, sender, mine, kind, text, spans, summary, detail, view_once,
                deleted, raw, v)
            VALUES (?1, ?2, ?3, ?4, ?1, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 0)",
            params![
                n,
                chat,
                row.id,
                AT + n,
                sender,
                row.mine,
                row.kind,
                row.text,
                row.spans,
                summary,
                row.detail,
                row.view_once,
                before.deleted,
                Packed::new(&before.info.encode_to_vec()).0,
            ],
        )
        .unwrap();
        ns.push(n);
    }
    ns
}

/// Message `id` in Ana's chat, from her, as `message`.
fn from_ana(id: &str, message: e2e::Message) -> WebMessageInfo {
    WebMessageInfo {
        key: MessageKey { remote_jid: Some(ANA.into()), from_me: Some(false), id: Some(id.into()), participant: None },
        message: Some(message),
        message_timestamp: Some((AT / 1000) as u64),
        ..Default::default()
    }
}

/// A file's hash, its encrypted hash, the key to read it with, and where it is.
type Keys = (Option<Vec<u8>>, Option<Vec<u8>>, Option<Vec<u8>>, Option<String>);

/// The keys of a file named by `seed`, as WhatsApp gives them: its hash, its encrypted hash, the key
/// to read it with, and where it is.
fn keys(seed: u8) -> Keys {
    (Some(vec![seed; 32]), Some(vec![seed ^ 0xff; 32]), Some(vec![seed ^ 0x5a; 32]), Some(format!("/v/t62/{seed}")))
}

fn photo(seed: u8) -> e2e::ImageMessage {
    let (file_sha256, file_enc_sha256, media_key, direct_path) = keys(seed);
    e2e::ImageMessage {
        mimetype: Some("image/jpeg".into()),
        file_sha256,
        file_enc_sha256,
        media_key,
        direct_path,
        file_length: Some(2_048),
        width: Some(640),
        height: Some(480),
        jpeg_thumbnail: Some(vec![0xff, 0xd8, 0xff, 0xd9]),
        ..Default::default()
    }
}

fn video(seed: u8, gif: bool) -> e2e::VideoMessage {
    let (file_sha256, file_enc_sha256, media_key, direct_path) = keys(seed);
    e2e::VideoMessage {
        mimetype: Some("video/mp4".into()),
        file_sha256,
        file_enc_sha256,
        media_key,
        direct_path,
        file_length: Some(90_000),
        seconds: Some(12),
        width: Some(320),
        height: Some(240),
        gif_playback: Some(gif),
        ..Default::default()
    }
}

fn audio(seed: u8, voice: bool) -> e2e::AudioMessage {
    let (file_sha256, file_enc_sha256, media_key, direct_path) = keys(seed);
    e2e::AudioMessage {
        mimetype: Some("audio/ogg; codecs=opus".into()),
        file_sha256,
        file_enc_sha256,
        media_key,
        direct_path,
        file_length: Some(9_000),
        seconds: Some(4),
        ptt: Some(voice),
        ..Default::default()
    }
}

fn document(seed: u8) -> e2e::DocumentMessage {
    let (file_sha256, file_enc_sha256, media_key, direct_path) = keys(seed);
    e2e::DocumentMessage {
        mimetype: Some("application/pdf".into()),
        file_sha256,
        file_enc_sha256,
        media_key,
        direct_path,
        file_length: Some(40_000),
        file_name: Some("menu.pdf".into()),
        ..Default::default()
    }
}

fn sticker(seed: u8) -> e2e::StickerMessage {
    let (file_sha256, file_enc_sha256, media_key, direct_path) = keys(seed);
    e2e::StickerMessage {
        mimetype: Some("image/webp".into()),
        file_sha256,
        file_enc_sha256,
        media_key,
        direct_path,
        file_length: Some(3_000),
        width: Some(512),
        height: Some(512),
        ..Default::default()
    }
}

fn photo_message(seed: u8) -> e2e::Message {
    e2e::Message { image_message: Some(Box::new(photo(seed))), ..Default::default() }
}

/// The HD copy `hd` of message `parent` in `chat`, as WhatsApp's dual upload sends it: a message of
/// its own, wrapped, naming its SD message, which a jawd before media in kept as a photo of its own.
pub(crate) fn hd_child(chat: &str, parent: &str, hd: e2e::Message) -> e2e::Message {
    let mut inner = hd;
    inner.message_context_info = Some(e2e::MessageContextInfo {
        message_association: Some(e2e::MessageAssociation {
            association_type: Some(e2e::message_association::AssociationType::HdImageDualUpload as i32),
            parent_message_key: Some(MessageKey {
                remote_jid: Some(chat.into()),
                from_me: Some(false),
                id: Some(parent.into()),
                participant: None,
            }),
            message_index: None,
        }),
        ..Default::default()
    });
    let wrapped = e2e::FutureProofMessage { message: Some(Box::new(inner)) };
    e2e::Message { associated_child_message: Some(Box::new(wrapped)), ..Default::default() }
}

/// Every kind of message with a file, and some without: a photo, a video, a GIF, a video message, a
/// voice message, an audio file, a document, a sticker, a photo to view once, a deleted photo, the
/// HD copy of the photo kept as a message of its own, a text, and a link preview.
fn every_kind() -> Vec<Before> {
    let once = e2e::FutureProofMessage { message: Some(Box::new(photo_message(9))) };
    let link = e2e::ExtendedTextMessage {
        text: Some("the menu https://jaw.example/menu".into()),
        matched_text: Some("https://jaw.example/menu".into()),
        title: Some("Menu".into()),
        ..Default::default()
    };
    vec![
        Before::kept(from_ana("3EB0PHOTO", photo_message(1))),
        Before::kept(from_ana(
            "3EB0VIDEO",
            e2e::Message { video_message: Some(Box::new(video(2, false))), ..Default::default() },
        )),
        Before::kept(from_ana(
            "3EB0GIF",
            e2e::Message { video_message: Some(Box::new(video(3, true))), ..Default::default() },
        )),
        Before::kept(from_ana(
            "3EB0PTV",
            e2e::Message { ptv_message: Some(Box::new(video(4, false))), ..Default::default() },
        )),
        Before::kept(from_ana(
            "3EB0VOICE",
            e2e::Message { audio_message: Some(Box::new(audio(5, true))), ..Default::default() },
        )),
        Before::kept(from_ana(
            "3EB0AUDIO",
            e2e::Message { audio_message: Some(Box::new(audio(6, false))), ..Default::default() },
        )),
        Before::kept(from_ana(
            "3EB0DOC",
            e2e::Message { document_message: Some(Box::new(document(7))), ..Default::default() },
        )),
        Before::kept(from_ana(
            "3EB0STICKER",
            e2e::Message { sticker_message: Some(Box::new(sticker(8))), ..Default::default() },
        )),
        Before::kept(from_ana(
            "3EB0ONCE",
            e2e::Message { view_once_message_v2: Some(Box::new(once)), ..Default::default() },
        )),
        Before { deleted: true, ..Before::kept(from_ana("3EB0DELETED", photo_message(10))) },
        Before {
            info: from_ana("3EB0HD", hd_child(ANA, "3EB0PHOTO", photo_message(11))),
            shown_as: Some(from_ana("3EB0HD", photo_message(11))),
            deleted: false,
        },
        Before::kept(from_ana("3EB0TEXT", e2e::Message { conversation: Some("hello".into()), ..Default::default() })),
        Before::kept(from_ana(
            "3EB0LINK",
            e2e::Message { extended_text_message: Some(Box::new(link)), ..Default::default() },
        )),
    ]
}

async fn query<T: Send + 'static>(store: &Store, query: impl FnOnce(&Connection) -> Result<T> + Send + 'static) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

async fn backfill(store: &Store) -> Applied {
    store.apply(Lane::Background, vec![Change::Backfill(COPIES)]).await.unwrap()
}

async fn backfill_all(store: &Store) {
    while store.backfill_left(COPIES).unwrap() > 0 {
        assert_eq!(backfill(store).await.quarantined, 0, "no step fails");
    }
}

/// Every copy, by its message's ID and variant, with all it says.
async fn copies(store: &Store) -> Vec<(String, Value)> {
    query(store, |db| {
        Ok(db
            .prepare(
                "SELECT m.id || ' ' || md.variant, json_object('keys', md.keys, 'mime', md.mime, 'bytes', md.bytes,
                    'w', md.w, 'h', md.h, 'seconds', md.seconds, 'name', md.name, 'sha256', hex(md.sha256),
                    'enc_sha256', hex(md.enc_sha256), 'media_key', hex(md.media_key), 'path', md.direct_path,
                    'thumbnail', md.thumbnail, 'state', md.state, 'progress', md.progress, 'error', md.error,
                    'wanted', md.wanted, 'asked', md.asked, 'arrived', md.arrived, 'retried_at', md.retried_at,
                    'cancelled', md.cancelled)
                FROM media md JOIN messages m ON m.n = md.message ORDER BY m.id, md.variant",
            )?
            .query_map([], |r| Ok((r.get(0)?, serde_json::from_str(&r.get::<_, String>(1)?).unwrap())))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await
}

/// Each message's ID and version, and how many events name it.
async fn versions(store: &Store) -> Vec<(String, i64, i64)> {
    query(store, |db| {
        Ok(db
            .prepare(
                "SELECT m.id, m.v, (SELECT count(*) FROM events e WHERE e.type = 'message' AND e.key = m.n)
                FROM messages m ORDER BY m.n",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn every_message_with_a_file_gets_the_copy_it_would_arriving_now_and_nothing_else_does() {
    // The same messages arriving from history to a jawd of today, but the deleted one, which a
    // message arriving again never brings back; the HD copy goes to its message as its HD copy.
    let arrived_home = tempfile::tempdir().unwrap();
    let arrived = Store::open(arrived_home.path()).unwrap();
    let me = Me::new([ME.to_string()]);
    for before in every_kind().iter().filter(|before| !before.deleted) {
        arrived.apply(Lane::Background, convert::message(&before.info, &me, false)).await.unwrap();
    }
    let mut arriving = copies(&arrived).await;
    assert_eq!(arriving.len(), 10, "{arriving:?}");
    assert!(arriving.iter().any(|(copy, _)| copy == "3EB0PHOTO hd"), "{arriving:?}");
    for version in [16, 17, 18] {
        let home = tempfile::tempdir().unwrap();
        let ns = before_media(home.path(), version, &every_kind());
        let store = Store::open(home.path()).unwrap();
        assert_eq!(store.backfill_left(COPIES).unwrap(), ns.len() as u64, "from version {version}");
        let shown = versions(&store).await;
        backfill_all(&store).await;
        // The HD copy kept as a photo of its own has that photo's file as its copy, and is its
        // photo's HD copy too.
        let mut backfilled = copies(&store).await;
        let hd = backfilled.iter().position(|(copy, _)| copy == "3EB0HD sd").expect("the HD row's copy");
        let (_, hd) = backfilled.remove(hd);
        assert_eq!(
            (&hd["sha256"], &hd["media_key"], &hd["path"], &hd["wanted"]),
            (&json!("0B".repeat(32)), &json!("51".repeat(32)), &json!("/v/t62/11"), &Value::Null),
            "from version {version}: {hd}"
        );
        // Every other copy is the one arriving writes, nothing wanting it, and nothing else has one.
        arriving.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(backfilled, arriving, "from version {version}");
        // Each message given a copy has a new version with its event, so windows show its file: the
        // photo one for each copy at most, as its HD copy may come in a transaction of its own.
        let now = versions(&store).await;
        for ((id, v, events), (_, was, _)) in now.iter().zip(&shown) {
            let made = arriving.iter().filter(|(copy, _)| copy.split(' ').next() == Some(id.as_str())).count();
            let made = if id == "3EB0HD" { 1 } else { made as i64 };
            assert_eq!(*v > *was, made > 0, "from version {version}: {id}");
            assert!((made.min(1)..=made).contains(events), "from version {version}: {id}, {events} events");
        }
        // Done again from the first message, it changes nothing.
        let raw = Connection::open(home.path().join("jaw.db")).unwrap();
        raw.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        raw.execute("INSERT INTO backfills (name, after, upto) VALUES ('copies', 0, ?1)", [ns.len() as i64]).unwrap();
        let mut changed = 0;
        while store.backfill_left(COPIES).unwrap() > 0 {
            changed += backfill(&store).await.changed;
        }
        assert_eq!((changed, versions(&store).await), (0, now), "from version {version}: done twice, once");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_meeting_many_to_copy_copies_one_and_the_next_goes_on_after_it() {
    let home = tempfile::tempdir().unwrap();
    let photos: Vec<Before> =
        (0..64u8).map(|k| Before::kept(from_ana(&format!("3EB0P{k:02}"), photo_message(k)))).collect();
    before_media(home.path(), 17, &photos);
    let store = Store::open(home.path()).unwrap();
    backfill(&store).await;
    let (after, copied, last): (i64, i64, Option<i64>) = query(&store, |db| {
        Ok(db.query_row(
            "SELECT (SELECT after FROM backfills WHERE name = 'copies'), count(*), max(message) FROM media",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    })
    .await;
    assert_eq!(copied, 1, "a step meeting 64 to copy in a row copies one");
    assert_eq!((after, last), (1, Some(1)), "what it copied is the first in a row, and the next goes on after it");
    backfill_all(&store).await;
    assert_eq!(store.count("media").unwrap(), 64);
}

/// Between its steps, photos arrive again from history, some are deleted for everyone and some for
/// you, jawd restarts and a step is taken again: every photo still there has its copy, once, as
/// arriving writes it, and none deleted has one.
#[tokio::test(flavor = "multi_thread")]
async fn the_backfill_among_arrivals_deletions_restarts_and_repeats_copies_each_photo_once() {
    let home = tempfile::tempdir().unwrap();
    let photos: Vec<WebMessageInfo> =
        (0..200u32).map(|k| from_ana(&format!("3EB0P{k:03}"), photo_message(k as u8))).collect();
    before_media(home.path(), 17, &photos.iter().cloned().map(Before::kept).collect::<Vec<_>>());
    let me = Me::new([ME.to_string()]);
    let mut store = Store::open(home.path()).unwrap();
    let mut deleted = std::collections::BTreeSet::new();
    let mut step = 0;
    while store.backfill_left(COPIES).unwrap() > 0 {
        step += 1;
        let again = &photos[(step * 37) % 200];
        store.apply(Lane::Background, convert::message(again, &me, false)).await.unwrap();
        let gone = format!("3EB0P{:03}", (step * 53) % 200);
        let deletion = match step % 3 {
            0 => Some(Change::DeleteMessage { chat: ANA.into(), id: gone.clone() }),
            1 => Some(Change::Amend(crate::store::Amend {
                chat: ANA.into(),
                target: gone.clone(),
                sender: Some(ANA.into()),
                mine: false,
                at: AT + 100_000 + step as i64,
                what: Amendment::Revoke,
                raw: None,
            })),
            _ => None,
        };
        if let Some(deletion) = deletion {
            store.apply(Lane::Live, vec![deletion]).await.unwrap();
            deleted.insert(gone);
        }
        assert_eq!(backfill(&store).await.quarantined, 0);
        if step % 4 == 0 {
            drop(store);
            store = Store::open(home.path()).unwrap();
        }
        if step % 5 == 0 {
            let raw = Connection::open(home.path().join("jaw.db")).unwrap();
            raw.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
            raw.execute("UPDATE backfills SET after = max(0, after - 64) WHERE name = 'copies'", []).unwrap();
        }
    }
    assert!(step > 4, "it took steps enough to meet every kind of interruption: {step}");
    let copies = copies(&store).await;
    let expected: Vec<String> = (0..200u32)
        .map(|k| format!("3EB0P{k:03}"))
        .filter(|id| !deleted.contains(id))
        .map(|id| format!("{id} sd"))
        .collect();
    assert_eq!(
        copies.iter().map(|(copy, _)| copy.clone()).collect::<Vec<_>>(),
        expected,
        "one copy each, none deleted"
    );
    for (copy, said) in &copies {
        let k: usize = copy[5..8].parse().unwrap();
        let seed = format!("{:02X}", k as u8).repeat(32);
        assert_eq!(
            (&said["sha256"], &said["wanted"], &said["state"]),
            (&json!(seed), &Value::Null, &json!("remote")),
            "{copy}"
        );
    }
}

/// A copy of jaw.db as a kill now would leave it, every commit so far and nothing more, in a
/// folder of its own.
fn killed(home: &Path) -> tempfile::TempDir {
    let copy = tempfile::tempdir().unwrap();
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    db.execute("VACUUM INTO ?1", [copy.path().join("jaw.db").to_str().unwrap()]).unwrap();
    copy
}

/// What a backfill leaves to compare runs by: every copy with all it says, each message's events,
/// what waits in `pending`, and what's kept in quarantine and left of the backfill.
async fn left_by(store: &Store) -> (Vec<(String, Value)>, Vec<(String, i64)>, Vec<String>, i64) {
    let events = versions(store).await.into_iter().map(|(id, _, events)| (id, events)).collect();
    let waiting = query(store, |db| {
        Ok(db
            .prepare("SELECT chat || ' ' || target || ' ' || kind FROM pending ORDER BY n")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    let kept = store.count("quarantine").unwrap() as i64 + store.backfill_left(COPIES).unwrap() as i64;
    (copies(store).await, events, waiting, kept)
}

/// Photos, voice messages, documents and texts, an HD copy kept as a photo of its own and one whose
/// photo jaw.db never had, and a photo deleted.
fn mixed() -> Vec<Before> {
    let mut messages = Vec::new();
    for k in 0..40u8 {
        let id = format!("3EB0M{k:02}");
        let message = match k % 5 {
            0 => photo_message(k),
            1 => e2e::Message { audio_message: Some(Box::new(audio(k, true))), ..Default::default() },
            2 => e2e::Message { conversation: Some(format!("text {k}")), ..Default::default() },
            3 => e2e::Message { document_message: Some(Box::new(document(k))), ..Default::default() },
            _ => e2e::Message { sticker_message: Some(Box::new(sticker(k))), ..Default::default() },
        };
        messages.push(Before { deleted: k == 25, ..Before::kept(from_ana(&id, message)) });
    }
    for (id, parent, seed) in [("3EB0HDOF00", "3EB0M00", 200), ("3EB0HDOFNONE", "3EB0NEVERHERE", 201)] {
        messages.push(Before {
            info: from_ana(id, hd_child(ANA, parent, photo_message(seed))),
            shown_as: Some(from_ana(id, photo_message(seed))),
            deleted: false,
        });
    }
    messages
}

/// Killed after any of its commits, and started again, the backfill ends as it does never killed:
/// every copy once, as it says, with an event each, and the HD copies in their photo or waiting for
/// it. A kill inside a transaction leaves jaw.db as its last commit did, so these are every kill.
#[tokio::test(flavor = "multi_thread")]
async fn the_backfill_killed_after_any_of_its_commits_and_started_again_ends_as_one_never_killed() {
    let home = tempfile::tempdir().unwrap();
    before_media(home.path(), 17, &mixed());
    let store = Store::open(home.path()).unwrap();
    let mut kills = vec![killed(home.path())];
    while store.backfill_left(COPIES).unwrap() > 0 {
        assert_eq!(backfill(&store).await.quarantined, 0);
        kills.push(killed(home.path()));
    }
    let never_killed = left_by(&store).await;
    assert_eq!(never_killed.0.len(), 34, "31 files, the two HD rows' and the photo's HD copy: {:?}", never_killed.0);
    assert!(never_killed.0.iter().any(|(copy, _)| copy == "3EB0M00 hd"), "{:?}", never_killed.0);
    assert_eq!(never_killed.2, [format!("{ANA} 3EB0NEVERHERE hd")], "the HD copy of a photo jaw.db never had waits");
    assert_eq!(never_killed.3, 0);
    assert!(kills.len() > 20, "a kill after each of its {} commits", kills.len() - 1);
    for (at, killed) in kills.iter().enumerate() {
        let store = Store::open(killed.path()).unwrap();
        while store.backfill_left(COPIES).unwrap() > 0 {
            assert_eq!(backfill(&store).await.quarantined, 0, "killed after commit {at}");
        }
        assert_eq!(left_by(&store).await, never_killed, "killed after commit {at}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disk_filling_as_a_copy_is_written_fails_its_step_which_is_tried_again_and_copies_it_once() {
    let home = tempfile::tempdir().unwrap();
    let photos = [from_ana("3EB0P1", photo_message(1)), from_ana(FILLS, photo_message(2))];
    before_media(home.path(), 17, &photos.map(Before::kept));
    let store = Store::open(home.path()).unwrap();
    while store.backfill_left(COPIES).unwrap() > 0 {
        backfill(&store).await;
    }
    assert!(FILLED.load(std::sync::atomic::Ordering::Relaxed), "the disk filled once");
    assert_eq!(store.count("media").unwrap(), 2, "both copied, that one once it didn't fill the disk");
    assert_eq!(store.count("quarantine").unwrap(), 0, "and nothing kept in quarantine for it");
    let versions = versions(&store).await;
    assert!(versions.iter().all(|(_, _, events)| *events == 1), "each copy written once: {versions:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_whose_conversion_panics_is_kept_in_quarantine_and_the_rest_get_their_copies() {
    let home = tempfile::tempdir().unwrap();
    let photos = [
        from_ana("3EB0P1", photo_message(1)),
        from_ana(PANICS, photo_message(2)),
        from_ana("3EB0P3", photo_message(3)),
    ];
    let raw = Packed::new(&photos[1].encode_to_vec()).0;
    before_media(home.path(), 17, &photos.map(Before::kept));
    let store = Store::open(home.path()).unwrap();
    backfill_all(&store).await;
    let copied: Vec<String> = copies(&store).await.into_iter().map(|(copy, _)| copy).collect();
    assert_eq!(copied, ["3EB0P1 sd", "3EB0P3 sd"]);
    let kept: Vec<(String, Vec<u8>)> = query(&store, |db| {
        Ok(db
            .prepare("SELECT ref, raw FROM quarantine WHERE source = 'copies'")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(kept, [(format!("{ANA} {PANICS}"), raw)], "kept in quarantine with its raw protobuf");
    let panicked = versions(&store).await.into_iter().find(|(id, ..)| id == PANICS).unwrap();
    assert_eq!(panicked, (PANICS.to_string(), 0, 0), "with no version or event of its own");
}

/// A message whose conversion fails once its copy is written is kept in quarantine, and what it
/// stamped goes with what it wrote. Here the HD copy of a photo, kept by an earlier jawd as a photo
/// of its own and stored before it, fails once it has given the photo its HD copy, and the photo's
/// own step follows in the same transaction, as a background transaction's steps do; a live write
/// makes the two steps one transaction whatever the load. The photo's copy then takes a seq of its
/// own and writes its event, so windows are told of it.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_failing_once_its_copy_is_written_leaves_nothing_stamped_for_its_transaction() {
    let home = tempfile::tempdir().unwrap();
    let messages = [
        Before {
            info: from_ana(FAILS, hd_child(ANA, "3EB0PHOTO", photo_message(11))),
            shown_as: Some(from_ana(FAILS, photo_message(11))),
            deleted: false,
        },
        Before::kept(from_ana("3EB0PHOTO", photo_message(1))),
    ];
    before_media(home.path(), 17, &messages);
    let store = Store::open(home.path()).unwrap();
    let applied = store.apply(Lane::Live, vec![Change::Backfill(COPIES); 2]).await.unwrap();
    assert_eq!((applied.transactions, store.backfill_left(COPIES).unwrap()), (1, 0), "both steps in one transaction");
    let copied: Vec<String> = copies(&store).await.into_iter().map(|(copy, _)| copy).collect();
    assert_eq!((copied, store.count("quarantine").unwrap()), (vec!["3EB0PHOTO sd".to_string()], 1));
    let untold: Vec<(String, i64)> = query(&store, |db| {
        Ok(db
            .prepare(
                "SELECT id, v FROM messages m WHERE v > 0
                    AND NOT EXISTS (SELECT 1 FROM events e WHERE e.type = 'message' AND e.key = m.n AND e.seq = m.v)",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert!(untold.is_empty(), "versions with no event: {untold:?}");
}

/// Measures the backfill on one made of `JAW_MEASURE_MESSAGES` messages from before media in, one
/// in `JAW_MEASURE_EVERY` of them a photo with its file's keys and the rest texts, each on a copy
/// of it: the backfill as jawd takes it, 64 steps to the writer at a time, by itself, after a live
/// write every 10 ms for 3 s by themselves; the backfill again with a live write every 10 ms beside
/// it; and the first `JAW_MEASURE_STEPS` steps each alone in a transaction of its own:
/// `JAW_MEASURE_MESSAGES=1000000 cargo test --release -p jawd measure_the_copies -- --ignored --nocapture`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a measurement, a minute long at a million messages"]
async fn measure_the_copies_backfill() {
    let home = tempfile::tempdir().unwrap();
    let messages: i64 = std::env::var("JAW_MEASURE_MESSAGES").ok().and_then(|n| n.parse().ok()).unwrap_or(200_000);
    let every: i64 = std::env::var("JAW_MEASURE_EVERY").ok().and_then(|n| n.parse().ok()).unwrap_or(10);
    let made = std::time::Instant::now();
    {
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.pragma_update(None, "journal_mode", "WAL").unwrap();
        for migration in &schema::MIGRATIONS[..19] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", 19).unwrap();
        db.execute_batch(&format!(
            "INSERT INTO sync_state (key, value) VALUES ('me', '{ME}');
            BEGIN;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 2000)
            INSERT INTO people (n, name, v) SELECT x, 'Person ' || x, 1 FROM c;
            INSERT INTO person_jids (n, jid, person) SELECT n, printf('3519%08d@s.whatsapp.net', n), n FROM people;
            INSERT INTO chats (n, id, kind, person, v) SELECT n, printf('3519%08d@s.whatsapp.net', n), 'direct', n, 1 FROM people;
            COMMIT;"
        ))
        .unwrap();
        // Texts, and a photo every `every` messages whose detail says what its file is.
        let detail = r#"{"media":{"mime":"image/jpeg","bytes":2048,"w":640,"h":480}}"#;
        db.execute(
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < ?1)
            INSERT INTO messages (n, chat, id, at, t, sender, kind, text, summary, detail, raw, v)
            SELECT x, 1 + x % 2000, printf('3EB0%016d', x), ?3 + x * 1000, (?3 + x * 1000) << 20, 1 + x % 2000,
                iif(x % ?2 = 0, 'image', 'text'), iif(x % ?2 = 0, NULL, 'mensagem ' || x),
                iif(x % ?2 = 0, 'Photo', NULL), iif(x % ?2 = 0, ?4, NULL), randomblob(300), x
            FROM c",
            params![messages, every, AT, detail],
        )
        .unwrap();
        db.execute_batch("BEGIN").unwrap();
        let mut raw = db.prepare("UPDATE messages SET raw = ?2 WHERE n = ?1").unwrap();
        for x in (1..=messages).filter(|x| x % every == 0) {
            let chat = format!("3519{:08}@s.whatsapp.net", 1 + x % 2000);
            let mut info = from_ana(&format!("3EB0{x:016}"), photo_message((x % 251) as u8));
            info.key.remote_jid = Some(chat);
            raw.execute(params![x, Packed::new(&info.encode_to_vec()).0]).unwrap();
        }
        drop(raw);
        db.execute_batch("COMMIT; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    }
    let made = made.elapsed();
    let (beside_home, alone) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for copy in [&beside_home, &alone] {
        std::fs::copy(home.path().join("jaw.db"), copy.path().join("jaw.db")).unwrap();
    }
    let began = std::time::Instant::now();
    let store = Store::open(home.path()).unwrap();
    let migrated = began.elapsed();
    let live = |store: &Store, done: Arc<AtomicBool>| tokio::spawn(live_writes(store.clone(), done));
    let done = Arc::new(AtomicBool::new(false));
    let writing = live(&store, done.clone());
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    done.store(true, Ordering::Relaxed);
    let by_themselves = writing.await.unwrap();
    let run = |store: Store| async move {
        let began = std::time::Instant::now();
        let (mut transactions, mut changed) = (0, 0);
        while store.backfill_left(COPIES).unwrap() > 0 {
            let applied = store.apply(Lane::Background, vec![Change::Backfill(COPIES); 64]).await.unwrap();
            assert_eq!(applied.quarantined, 0, "a step failed");
            (transactions, changed) = (transactions + applied.transactions, changed + applied.changed);
        }
        (began.elapsed(), transactions, changed)
    };
    let (backfilled, transactions, changed) = run(store.clone()).await;
    let runs = store.report().run.unwrap();
    let copies = store.count("media").unwrap();
    drop(store);
    let store = Store::open(beside_home.path()).unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let writing = live(&store, done.clone());
    let (backfilled_beside, ..) = run(store.clone()).await;
    done.store(true, Ordering::Relaxed);
    let beside = writing.await.unwrap();
    drop(store);
    let most: usize = std::env::var("JAW_MEASURE_STEPS").ok().and_then(|n| n.parse().ok()).unwrap_or(20_000);
    let store = Store::open(alone.path()).unwrap();
    let (mut copying, mut looking) = (Vec::new(), Vec::new());
    while store.backfill_left(COPIES).unwrap() > 0 && copying.len() + looking.len() < most {
        let applied = backfill(&store).await;
        assert_eq!((applied.quarantined, applied.transactions), (0, 1));
        (if applied.changed > 0 { &mut copying } else { &mut looking }).push(applied.took);
    }
    eprintln!(
        "{messages} messages, {} of them photos, made in {:.1} s: migrated to 20 in {:.1} ms; backfilled in {:.2} s \
         ({:.0} messages a second), {copies} copies, {changed} steps changing some, in {transactions} transactions, \
         each {:.2} ms p50 and {:.2} ms p99 with its commit (the writer's last 1,024); a live write every 10 ms took \
         {} by itself, and {} beside the backfill, which then took {:.2} s; each step alone in a transaction, commit \
         included, took {} copying and {} looking only",
        messages / every,
        made.as_secs_f64(),
        migrated.as_secs_f64() * 1e3,
        backfilled.as_secs_f64(),
        messages as f64 / backfilled.as_secs_f64(),
        runs.p50_us as f64 / 1e3,
        runs.p99_us as f64 / 1e3,
        spread(by_themselves),
        spread(beside),
        backfilled_beside.as_secs_f64(),
        spread(copying),
        spread(looking),
    );
    assert_eq!(copies as i64, messages / every);
}

/// Live writes, one every 10 ms until `done` is set, each timed from its asking to its answer.
async fn live_writes(store: Store, done: Arc<AtomicBool>) -> Vec<std::time::Duration> {
    let mut every = tokio::time::interval(std::time::Duration::from_millis(10));
    let mut took = Vec::new();
    while !done.load(Ordering::Relaxed) {
        every.tick().await;
        let began = std::time::Instant::now();
        let write = Change::Remember { key: "measuring".into(), value: took.len().to_string() };
        store.apply(Lane::Live, vec![write]).await.unwrap();
        took.push(began.elapsed());
    }
    took
}

/// Durations as their median, 99th percentile and most, and how many there were.
fn spread(mut took: Vec<std::time::Duration>) -> String {
    if took.is_empty() {
        return "nothing".into();
    }
    took.sort_unstable();
    let at = |q: f64| took[((took.len() - 1) as f64 * q).round() as usize].as_secs_f64() * 1e3;
    format!("{:.2} ms p50, {:.2} ms p99 and {:.2} ms at most, of {}", at(0.5), at(0.99), at(1.0), took.len())
}
