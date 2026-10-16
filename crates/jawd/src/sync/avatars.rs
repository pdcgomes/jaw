//! Avatars for chats, in the background lane: each asked of WhatsApp with the picture's ID jaw has,
//! so an unchanged one costs a question and no download; a new one is downloaded once, resized to
//! SIZE pixels and kept in media/avatars/. One at a time with a pause between, since asking for a
//! hundred pictures at once is what scrapers do. A chat is asked for when something wants it
//! (an interface showing it, a picture changing) and, at each start, the newest chats never asked.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;

use crate::engine::pb;
use crate::engine::supervisor::Supervisor;
use crate::store::{Change, Chat, Lane, Store};
use crate::work::Priority;

/// The avatar ID kept for a chat WhatsApp says has no picture, or hides it, so it isn't asked again
/// at each start; a picture event asks again.
pub const NONE: &str = "none";
/// The longest side of an avatar as it's kept, in pixels.
const SIZE: u32 = 256;
/// The pause between two pictures asked for.
const PAUSE: Duration = Duration::from_millis(500);
/// How many chats never asked are seeded at each start, from the top of the chat list.
const SEED: usize = 50;
/// Where avatars go, in jaw's folder.
const FOLDER: &str = "media/avatars";

/// Asks for chats' avatars in the background lane.
#[derive(Clone)]
pub struct Avatars {
    tx: mpsc::UnboundedSender<String>,
    rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<String>>>>,
    queued: Arc<Mutex<HashSet<String>>>,
}

impl Avatars {
    pub fn new() -> Avatars {
        let (tx, rx) = mpsc::unbounded_channel();
        Avatars { tx, rx: Arc::new(Mutex::new(Some(rx))), queued: Arc::default() }
    }

    /// Asks for `jid`'s avatar, unless it's already waiting.
    pub fn want(&self, jid: &str) {
        if self.queued.lock().expect("avatars queued").insert(jid.to_string()) {
            let _ = self.tx.send(jid.to_string());
        }
    }

    /// Asks for the avatars of the chats at the top of the list that were never asked for, the
    /// chat list's first and then the archived list's, each read down its own index.
    pub async fn seed(&self, store: &Store) {
        let chats: anyhow::Result<Vec<String>> = store
            .reads()
            .query(Priority::Background, |db| {
                let mut ids = Vec::new();
                for archived in [false, true] {
                    let more = SEED - ids.len();
                    ids.extend(
                        db.prepare_cached(
                            "SELECT id FROM chats WHERE archived = ?1 AND avatar_id IS NULL ORDER BY sort DESC LIMIT ?2",
                        )?
                        .query_map(rusqlite::params![archived, more as i64], |r| r.get::<_, String>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()?,
                    );
                }
                Ok(ids)
            })
            .await;
        match chats {
            Ok(chats) => chats.iter().for_each(|jid| self.want(jid)),
            Err(e) => eprintln!("jawd: can't read which chats need avatars: {e:#}"),
        }
    }

    /// Fetches what's asked for, one at a time, until jawd stops.
    pub async fn run(self, store: Store, engine: Supervisor, home: PathBuf) {
        let Some(mut rx) = self.rx.lock().expect("avatars").take() else { return };
        while let Some(jid) = rx.recv().await {
            self.queued.lock().expect("avatars queued").remove(&jid);
            if let Err(e) = one(&jid, &store, &engine, &home).await {
                eprintln!("jawd: no avatar for {jid} this time: {e:#}");
            }
            tokio::time::sleep(PAUSE).await;
        }
    }
}

async fn one(jid: &str, store: &Store, engine: &Supervisor, home: &Path) -> anyhow::Result<()> {
    let asked = jid.to_string();
    let known: Option<(Option<String>, String)> = store
        .reads()
        .query(Priority::Background, move |db| {
            Ok(db
                .query_row("SELECT avatar_id, kind FROM chats WHERE id = ?1", [&asked], |r| Ok((r.get(0)?, r.get(1)?)))
                .ok())
        })
        .await?;
    let Some((existing, kind)) = known else { return Ok(()) };
    let name = file_name(jid);
    let full = home.join(FOLDER).join(format!("{name}.download"));
    let command = pb::command::Kind::Avatar(pb::Avatar {
        jid: jid.to_string(),
        existing_id: existing.clone().filter(|id| id != NONE).unwrap_or_default(),
        path: full.display().to_string(),
        community: kind == "community",
    });
    let pb::reply::Kind::AvatarFile(file) = engine.call(command).await? else {
        anyhow::bail!("the engine didn't answer with a picture");
    };
    let chat = match file.state() {
        pb::avatar_file::State::Unchanged | pb::avatar_file::State::Unspecified => return Ok(()),
        pb::avatar_file::State::None => {
            Chat { avatar: Some(String::new()), avatar_id: Some(NONE.into()), ..Chat::default() }
        }
        pb::avatar_file::State::Downloaded => {
            let relative = format!("{FOLDER}/{name}.jpg");
            let kept = home.join(&relative);
            let resized = full.clone();
            let outcome = tokio::task::spawn_blocking(move || resize(&resized, &kept)).await?;
            let _ = std::fs::remove_file(&full);
            outcome?;
            Chat { avatar: Some(relative), avatar_id: Some(file.id), ..Chat::default() }
        }
    };
    store.apply(Lane::Background, vec![Change::Chat(Chat { id: jid.to_string(), ..chat })]).await?;
    Ok(())
}

/// A file name for a JID's avatar: its user part and server, with nothing a path could misread.
fn file_name(jid: &str) -> String {
    jid.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect()
}

/// Writes `from`, an image, to `to` as a JPEG whose longest side is at most SIZE pixels, through a
/// file renamed into place once it's whole.
fn resize(from: &Path, to: &Path) -> anyhow::Result<()> {
    let image = image::ImageReader::open(from)?.with_guessed_format()?.decode()?;
    let small = if image.width().max(image.height()) > SIZE { image.thumbnail(SIZE, SIZE) } else { image };
    let part = to.with_extension("jpg.part");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&part)?);
    small.to_rgb8().write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85))?;
    drop(out);
    std::fs::rename(&part, to)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_kept_resized_to_its_longest_side() {
        let folder = tempfile::tempdir().unwrap();
        let from = folder.path().join("a.download");
        image::RgbImage::from_pixel(640, 320, image::Rgb([200, 30, 30]))
            .save_with_format(&from, image::ImageFormat::Png)
            .unwrap();
        let to = folder.path().join("a.jpg");
        resize(&from, &to).unwrap();
        let kept = image::open(&to).unwrap();
        assert_eq!((kept.width(), kept.height()), (SIZE, SIZE / 2));
        assert!(!folder.path().join("a.jpg.part").exists());
    }

    #[test]
    fn an_avatars_file_name_has_nothing_a_path_could_misread() {
        assert_eq!(file_name("120363000000000001@g.us"), "120363000000000001_g.us");
        assert_eq!(file_name("../x@lid"), ".._x_lid");
    }
}
