//! The jaw.db `media::tests` opens from `crates/jawd/tests/fixtures/media-in/`, made by jawd's own
//! sync at f917587, before jawd kept media: what an engine handed over live and the history an
//! earlier jawd kept raw, converted as that jawd converts them, messages with a file of every kind
//! among them, then `VACUUM INTO`; and beside it the files WhatsApp's server held for them, by where
//! it held them. This file was added to `crates/jawd/src/sync/tests.rs` at f917587 as
//! `#[path = "…/made-at-f917587.rs"] mod made_at_f917587;`, run with
//! `JAW_FIXTURES=<folder> cargo test -p jawd made_at_f917587 -- --ignored --nocapture`, and taken
//! out again.
//!
//! In Ana's chat, from history: a photo with its HD copy, which that jawd kept as a photo of its
//! own; a video, a GIF and a video message; a voice message and an audio file; a document, and one
//! with a caption; a sticker; a photo to view once; a photo in a disappearing chat; the HD copy of a
//! photo jaw.db never had; a photo of yours from your phone; a photo she deleted for everyone and
//! one you deleted for you; and a text. Live: a photo, then her deletion and yours. In a group: a
//! photo of Rui's.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use prost::Message as _;
use rusqlite::Connection;
use tokio::sync::{broadcast, mpsc};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_e2e::message_association::AssociationType;
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::{app_state, live_message, until};
use crate::store::Store;

const ANA: &str = "351912345678@s.whatsapp.net";
const RUI: &str = "351912345679@s.whatsapp.net";
const GROUP: &str = "120363000000000042@g.us";
const AT: i64 = 1_792_000_000_000;

/// A file WhatsApp's server holds: its bytes, their SHA-256, and where.
struct File {
    data: Vec<u8>,
    sha256: Vec<u8>,
    path: String,
}

fn file(name: &str, data: Vec<u8>) -> File {
    let sha256 = <sha2::Sha256 as sha2::Digest>::digest(&data).to_vec();
    File { data, sha256, path: format!("/v/t62.7117-24/fixture-{name}.enc?ccb=11-4") }
}

/// A JPEG of `w` by `h`, a gradient.
fn jpeg(w: u32, h: u32, shade: u8) -> Vec<u8> {
    let picture =
        image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, shade]));
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80).encode_image(&picture).unwrap();
    out
}

/// Bytes standing in for a file that isn't a picture.
fn bytes(seed: u8, len: usize) -> Vec<u8> {
    (0..len).map(|n| (n as u8).wrapping_mul(31).wrapping_add(seed)).collect()
}

/// A file's keys as WhatsApp gives them: its hash, its encrypted hash, the key to read it with, and
/// where it is.
fn keys(f: &File, seed: u8) -> (Option<Vec<u8>>, Option<Vec<u8>>, Option<Vec<u8>>, Option<String>) {
    (Some(f.sha256.clone()), Some(f.sha256.iter().rev().copied().collect()), Some(vec![seed; 32]), Some(f.path.clone()))
}

fn photo(f: &File, seed: u8, (w, h): (u32, u32), caption: Option<&str>, thumbnail: &[u8]) -> e2e::Message {
    let (file_sha256, file_enc_sha256, media_key, direct_path) = keys(f, seed);
    let image = e2e::ImageMessage {
        mimetype: Some("image/jpeg".into()),
        caption: caption.map(str::to_string),
        file_sha256,
        file_enc_sha256,
        media_key,
        direct_path,
        file_length: Some(f.data.len() as u64),
        width: Some(w),
        height: Some(h),
        jpeg_thumbnail: Some(thumbnail.to_vec()),
        ..Default::default()
    };
    e2e::Message { image_message: Some(Box::new(image)), ..Default::default() }
}

fn video(f: &File, seed: u8, gif: bool, thumbnail: &[u8]) -> e2e::VideoMessage {
    let (file_sha256, file_enc_sha256, media_key, direct_path) = keys(f, seed);
    e2e::VideoMessage {
        mimetype: Some("video/mp4".into()),
        file_sha256,
        file_enc_sha256,
        media_key,
        direct_path,
        file_length: Some(f.data.len() as u64),
        seconds: Some(4),
        width: Some(320),
        height: Some(240),
        gif_playback: Some(gif),
        jpeg_thumbnail: Some(thumbnail.to_vec()),
        ..Default::default()
    }
}

fn audio(f: &File, seed: u8, voice: bool) -> e2e::Message {
    let (file_sha256, file_enc_sha256, media_key, direct_path) = keys(f, seed);
    let audio = e2e::AudioMessage {
        mimetype: Some("audio/ogg; codecs=opus".into()),
        file_sha256,
        file_enc_sha256,
        media_key,
        direct_path,
        file_length: Some(f.data.len() as u64),
        seconds: Some(3),
        ptt: Some(voice),
        ..Default::default()
    };
    e2e::Message { audio_message: Some(Box::new(audio)), ..Default::default() }
}

fn document(f: &File, seed: u8, caption: Option<&str>) -> e2e::Message {
    let (file_sha256, file_enc_sha256, media_key, direct_path) = keys(f, seed);
    let document = e2e::DocumentMessage {
        mimetype: Some("application/pdf".into()),
        file_sha256,
        file_enc_sha256,
        media_key,
        direct_path,
        file_length: Some(f.data.len() as u64),
        file_name: Some("menu.pdf".into()),
        caption: caption.map(str::to_string),
        ..Default::default()
    };
    e2e::Message { document_message: Some(Box::new(document)), ..Default::default() }
}

fn sticker(f: &File, seed: u8) -> e2e::Message {
    let (file_sha256, file_enc_sha256, media_key, direct_path) = keys(f, seed);
    let sticker = e2e::StickerMessage {
        mimetype: Some("image/webp".into()),
        file_sha256,
        file_enc_sha256,
        media_key,
        direct_path,
        file_length: Some(f.data.len() as u64),
        width: Some(512),
        height: Some(512),
        ..Default::default()
    };
    e2e::Message { sticker_message: Some(Box::new(sticker)), ..Default::default() }
}

/// `message` inside a wrapper of WhatsApp's.
fn wrapped(message: e2e::Message) -> Option<Box<e2e::FutureProofMessage>> {
    Some(Box::new(e2e::FutureProofMessage { message: Some(Box::new(message)) }))
}

/// The HD copy `hd` of message `parent` in Ana's chat, as WhatsApp's dual upload sends it: a
/// message of its own, wrapped, naming its SD message.
fn hd_of(parent: &str, hd: e2e::Message) -> e2e::Message {
    let mut inner = hd;
    inner.message_context_info = Some(e2e::MessageContextInfo {
        message_association: Some(e2e::MessageAssociation {
            association_type: Some(AssociationType::HdImageDualUpload as i32),
            parent_message_key: Some(MessageKey {
                remote_jid: Some(ANA.into()),
                from_me: Some(false),
                id: Some(parent.into()),
                participant: None,
            }),
            message_index: None,
        }),
        ..Default::default()
    });
    e2e::Message { associated_child_message: wrapped(inner), ..Default::default() }
}

/// History kept raw: message `id` in `chat`, from `sender` in a group, or yours.
fn kept(chat: &str, sender: Option<&str>, mine: bool, id: &str, at: i64, message: e2e::Message) -> WebMessageInfo {
    WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(chat.into()),
            from_me: Some(mine),
            id: Some(id.into()),
            participant: sender.map(str::to_string),
        },
        message: Some(message),
        message_timestamp: Some((at / 1000) as u64),
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn make_the_media_in_fixture() {
    let folder = std::env::var("JAW_FIXTURES").expect("JAW_FIXTURES names the folder to write to");
    let thumbnail = jpeg(32, 24, 40);
    let files = [
        file("photo", jpeg(640, 480, 60)),
        file("hd", jpeg(1280, 960, 60)),
        file("video", bytes(3, 20_000)),
        file("gif", bytes(4, 12_000)),
        file("ptv", bytes(5, 16_000)),
        file("voice", bytes(6, 6_000)),
        file("audio", bytes(7, 9_000)),
        file("document", bytes(8, 30_000)),
        file("captioned", bytes(9, 25_000)),
        file("sticker", bytes(10, 3_000)),
        file("once", jpeg(320, 240, 90)),
        file("disappearing", jpeg(300, 200, 120)),
        file("orphan", jpeg(1024, 768, 150)),
        file("mine", jpeg(400, 300, 180)),
        file("revoked", jpeg(200, 150, 200)),
        file("deleted-for-me", jpeg(240, 180, 210)),
        file("live", jpeg(480, 360, 220)),
        file("group", jpeg(360, 240, 230)),
    ];
    let f = |name: &str| {
        let path = format!("/v/t62.7117-24/fixture-{name}.enc?ccb=11-4");
        files.iter().find(|f| f.path == path).unwrap()
    };
    let ana = |id: &str, at: i64, message: e2e::Message| (ANA, id.to_string(), kept(ANA, None, false, id, at, message));
    let history = vec![
        ana("3EB0FXPHOTO", AT, photo(f("photo"), 1, (640, 480), Some("the lake"), &thumbnail)),
        ana("3EB0FXHD", AT + 1, hd_of("3EB0FXPHOTO", photo(f("hd"), 2, (1280, 960), None, &thumbnail))),
        ana("3EB0FXVIDEO", AT + 2, e2e::Message {
            video_message: Some(Box::new(video(f("video"), 3, false, &thumbnail))),
            ..Default::default()
        }),
        ana("3EB0FXGIF", AT + 3, e2e::Message {
            video_message: Some(Box::new(video(f("gif"), 4, true, &thumbnail))),
            ..Default::default()
        }),
        ana("3EB0FXPTV", AT + 4, e2e::Message {
            ptv_message: Some(Box::new(video(f("ptv"), 5, false, &thumbnail))),
            ..Default::default()
        }),
        ana("3EB0FXVOICE", AT + 5, audio(f("voice"), 6, true)),
        ana("3EB0FXAUDIO", AT + 6, audio(f("audio"), 7, false)),
        ana("3EB0FXDOC", AT + 7, document(f("document"), 8, None)),
        ana("3EB0FXDOCCAP", AT + 8, e2e::Message {
            document_with_caption_message: wrapped(document(f("captioned"), 9, Some("the menu"))),
            ..Default::default()
        }),
        ana("3EB0FXSTICKER", AT + 9, sticker(f("sticker"), 10)),
        ana("3EB0FXONCE", AT + 10, e2e::Message {
            view_once_message_v2: wrapped(photo(f("once"), 11, (320, 240), None, &thumbnail)),
            ..Default::default()
        }),
        ana("3EB0FXEPHEMERAL", AT + 11, e2e::Message {
            ephemeral_message: wrapped(photo(f("disappearing"), 12, (300, 200), None, &thumbnail)),
            ..Default::default()
        }),
        ana("3EB0FXORPHAN", AT + 12, hd_of("3EB0FXNOPARENT", photo(f("orphan"), 13, (1024, 768), None, &thumbnail))),
        (ANA, "3EB0FXMINE".into(), kept(ANA, None, true, "3EB0FXMINE", AT + 13, photo(f("mine"), 14, (400, 300), None, &thumbnail))),
        ana("3EB0FXREVOKED", AT + 14, photo(f("revoked"), 15, (200, 150), None, &thumbnail)),
        ana("3EB0FXDELME", AT + 15, photo(f("deleted-for-me"), 16, (240, 180), None, &thumbnail)),
        ana("3EB0FXTEXT", AT + 16, e2e::Message { conversation: Some("see you there".into()), ..Default::default() }),
        (GROUP, "3EB0FXGROUP".into(), kept(GROUP, Some(RUI), false, "3EB0FXGROUP", AT + 17, photo(f("group"), 18, (360, 240), None, &thumbnail))),
    ];
    let revoke = e2e::Message {
        protocol_message: Some(Box::new(e2e::ProtocolMessage {
            key: Some(MessageKey {
                remote_jid: Some(ANA.into()),
                from_me: Some(false),
                id: Some("3EB0FXREVOKED".into()),
                participant: None,
            }),
            r#type: Some(e2e::protocol_message::Type::Revoke as i32),
            ..Default::default()
        })),
        ..Default::default()
    };
    let inbox: Vec<(&str, Vec<u8>)> = vec![
        (
            "message",
            live_message(ANA, ANA, "3EB0FXLIVE", AT + 20_000, photo(f("live"), 17, (480, 360), None, &thumbnail))
                .encode_to_vec(),
        ),
        ("message", live_message(ANA, ANA, "3EB0FXREVOKE", AT + 21_000, revoke).encode_to_vec()),
        (
            "app_state",
            app_state(&["deleteMessageForMe", ANA, "3EB0FXDELME", "0", "0"], |v| {
                v.delete_message_for_me_action = Some(Default::default());
            }),
        ),
    ];
    let home = tempfile::tempdir().unwrap();
    drop(Store::open(home.path()).unwrap());
    {
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        for (kind, payload) in &inbox {
            db.execute("INSERT INTO inbox (kind, at, payload) VALUES (?1, 0, ?2)", rusqlite::params![kind, payload])
                .unwrap();
        }
        for (chat, id, info) in &history {
            db.execute(
                "INSERT INTO history_raw (chat, id, at, raw) VALUES (?1, ?2, 0, ?3)",
                rusqlite::params![chat, id, info.encode_to_vec()],
            )
            .unwrap();
        }
    }
    let store = Store::open(home.path()).unwrap();
    let (events, incoming) = mpsc::channel(8);
    let running = tokio::spawn(super::run(
        incoming,
        store.clone(),
        Arc::default(),
        broadcast::channel(16).0,
        None,
        home.path().to_path_buf(),
    ));
    let connected = crate::engine::demo::connected();
    events.send(crate::engine::Incoming { event: connected, ack: None }).await.unwrap();
    until("everything converted", || {
        store.count("inbox").unwrap() == 0 && store.count("history_raw").unwrap() == 0
    })
    .await;
    store.apply(crate::store::Lane::Background, Vec::new()).await.unwrap();
    running.abort();
    drop(store);
    let folder = Path::new(&folder);
    let made = folder.join("made-by-main.db");
    let _ = std::fs::remove_file(&made);
    {
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        db.execute("VACUUM INTO ?1", [made.to_str().unwrap()]).unwrap();
    }
    let packed = zstd::encode_all(std::fs::read(&made).unwrap().as_slice(), 19).unwrap();
    std::fs::write(folder.join("made-by-main.db.zst"), packed).unwrap();
    let served: BTreeMap<&str, String> = files
        .iter()
        .map(|f| (f.path.as_str(), f.data.iter().map(|b| format!("{b:02x}")).collect::<String>()))
        .collect();
    let served = serde_json::to_vec(&served).unwrap();
    std::fs::write(folder.join("served.json.zst"), zstd::encode_all(served.as_slice(), 19).unwrap()).unwrap();
    let db = Connection::open(&made).unwrap();
    println!("made-by-main.db:");
    for sql in [
        "SELECT 'message', m.n, c.id, m.id, m.kind, m.deleted, m.view_once, instr(m.detail, '\"media\":') > 0,
            m.raw IS NOT NULL, m.envelope IS NOT NULL
        FROM messages m JOIN chats c ON c.n = m.chat ORDER BY m.n",
        "SELECT 'tombstone', c.id, t.id FROM tombstones t JOIN chats c ON c.n = t.chat",
        "SELECT 'pending', chat, target, kind FROM pending ORDER BY n",
        "SELECT 'quarantine', source, ref FROM quarantine",
        "SELECT 'media', count(*) FROM media",
        "SELECT 'backfills', name, after, upto FROM backfills",
        "SELECT 'version', (SELECT user_version FROM pragma_user_version)",
    ] {
        let mut statement = db.prepare(sql).unwrap();
        let columns = statement.column_count();
        let mut rows = statement.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let values: Vec<String> =
                (0..columns).map(|i| format!("{:?}", row.get::<_, rusqlite::types::Value>(i).unwrap())).collect();
            println!("  {}", values.join(" "));
        }
    }
    std::fs::remove_file(&made).unwrap();
}
