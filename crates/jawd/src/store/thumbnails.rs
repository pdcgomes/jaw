//! The thumbnails of link previews, kept as files in media/links/ under jaw's folder, each named by
//! the SHA-256 of its JPEG: written through a .part file synced before it's renamed into place, and
//! the folder synced before any is said to be kept, so a file found under its name is whole and
//! stays; only then does its row go in (`Change::Thumbnail`), and a preview shows a thumbnail only
//! once its row is in. They take THUMBNAILS_MOST bytes at most, each THUMBNAIL_MOST: past either a
//! preview goes without its thumbnail, which stays in the message's raw protobuf.

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use sha2::{Digest, Sha256};

/// Where the thumbnails go, in jaw's folder.
pub const FOLDER: &str = "media/links";
/// The largest thumbnail kept, in bytes; WhatsApp's are a few kilobytes.
pub const THUMBNAIL_MOST: usize = 64 * 1024;
/// The most bytes the thumbnails take in all.
pub const THUMBNAILS_MOST: u64 = 256 * 1024 * 1024;

/// The path of the thumbnail whose JPEG has SHA-256 `sha256`, relative to jaw's folder, as
/// interfaces find it.
pub fn path(sha256: &str) -> String {
    format!("{FOLDER}/{sha256}.jpg")
}

/// SHA-256 of `bytes`, in lower-case hex, as thumbnails are named.
pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

pub struct Thumbnails {
    home: PathBuf,
    most: u64,
    /// The bytes in the files kept, as jaw.db said at start and as jawd has written since.
    bytes: AtomicU64,
    /// How many were refused past THUMBNAILS_MOST since jawd started, and whether that's been said.
    refused: AtomicU64,
    said: AtomicBool,
    /// How long a test has keeping thumbnails take, in milliseconds, as a slow disk's syncs do.
    #[cfg(test)]
    slow_ms: AtomicU64,
}

impl Thumbnails {
    /// The thumbnails under `home`, whose rows say they take `kept` bytes.
    pub(super) fn open(home: &Path, kept: u64) -> Thumbnails {
        Thumbnails::bounded(home, kept, THUMBNAILS_MOST)
    }

    pub(super) fn bounded(home: &Path, kept: u64, most: u64) -> Thumbnails {
        Thumbnails {
            home: home.to_path_buf(),
            most,
            bytes: AtomicU64::new(kept),
            refused: AtomicU64::new(0),
            said: AtomicBool::new(false),
            #[cfg(test)]
            slow_ms: AtomicU64::new(0),
        }
    }

    /// Has keeping thumbnails take `by` longer, as a slow disk's syncs do.
    #[cfg(test)]
    pub fn slow(&self, by: std::time::Duration) {
        self.slow_ms.store(by.as_millis() as u64, Ordering::Relaxed);
    }

    /// How many thumbnails were refused for want of room since jawd started.
    pub fn refused(&self) -> u64 {
        self.refused.load(Ordering::Relaxed)
    }

    /// Keeps each of `thumbnails` (a SHA-256 and its JPEG) whose file isn't whole already, and says
    /// which are kept: each then has its whole file, durably, for its row to point to. One whose
    /// bytes aren't what its name says, that's too large, or that there's no room for, isn't. It
    /// blocks, on writes and syncs, so it runs where that's allowed.
    pub fn keep(&self, thumbnails: &[(String, Vec<u8>)]) -> Vec<String> {
        #[cfg(test)]
        std::thread::sleep(std::time::Duration::from_millis(self.slow_ms.load(Ordering::Relaxed)));
        let folder = self.home.join(FOLDER);
        let mut kept: Vec<String> = Vec::new();
        for (name, jpeg) in thumbnails {
            if kept.contains(name) || jpeg.is_empty() || jpeg.len() > THUMBNAIL_MOST || sha256(jpeg) != *name {
                continue;
            }
            let file = folder.join(format!("{name}.jpg"));
            if file.is_file() {
                kept.push(name.clone());
                continue;
            }
            let bytes = jpeg.len() as u64;
            if self.bytes.load(Ordering::Relaxed) + bytes > self.most {
                self.refused.fetch_add(1, Ordering::Relaxed);
                if !self.said.swap(true, Ordering::Relaxed) {
                    eprintln!(
                        "jawd: link previews' thumbnails take {} MB, as many as they may, so new ones show without theirs",
                        self.most / 1_000_000
                    );
                }
                continue;
            }
            match write_whole(&folder, &file, jpeg) {
                Ok(()) => {
                    self.bytes.fetch_add(bytes, Ordering::Relaxed);
                    kept.push(name.clone());
                }
                Err(e) => eprintln!("jawd: can't keep a link preview's thumbnail, so it shows without it: {e}"),
            }
        }
        // A rename reaches the disk with its folder, and a row may point only to what has.
        if !kept.is_empty()
            && let Err(e) = std::fs::File::open(&folder).and_then(|f| f.sync_all())
        {
            eprintln!("jawd: can't sync the folder of link previews' thumbnails, so these show without them: {e}");
            return Vec::new();
        }
        kept
    }
}

/// Writes `data` to `file` in `folder` through a .part file synced before it's renamed into place,
/// so after a power cut the file is either whole or not there.
fn write_whole(folder: &Path, file: &Path, data: &[u8]) -> std::io::Result<()> {
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(folder)?;
    let part = file.with_extension("jpg.part");
    let written = (|| {
        let mut out = std::fs::OpenOptions::new().create(true).write(true).truncate(true).mode(0o600).open(&part)?;
        out.write_all(data)?;
        // F_FULLFSYNC on macOS, where fsync stops at the drive's cache.
        out.sync_all()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thumbnail_is_kept_whole_under_its_hash_once_and_one_not_its_hash_or_past_the_bound_is_not() {
        let home = tempfile::tempdir().unwrap();
        let thumbnails = Thumbnails::bounded(home.path(), 0, 10);
        let jpeg = b"0123456".to_vec();
        let name = sha256(&jpeg);
        let wrong = ("0".repeat(64), b"abc".to_vec());
        assert_eq!(thumbnails.keep(&[(name.clone(), jpeg.clone()), wrong]), std::slice::from_ref(&name));
        let file = home.path().join(path(&name));
        assert_eq!(std::fs::read(&file).unwrap(), jpeg);
        assert!(!file.with_extension("jpg.part").exists());
        assert_eq!(
            thumbnails.keep(&[(name.clone(), jpeg.clone())]),
            std::slice::from_ref(&name),
            "kept again, as it is"
        );
        let more = b"0123".to_vec();
        assert!(thumbnails.keep(&[(sha256(&more), more)]).is_empty(), "7 bytes kept, 4 more is past 10");
        assert_eq!(thumbnails.refused(), 1);
        let large = vec![7u8; THUMBNAIL_MOST + 1];
        let roomy = Thumbnails::bounded(home.path(), 0, THUMBNAILS_MOST);
        assert!(roomy.keep(&[(sha256(&large), large)]).is_empty(), "too large to be a thumbnail");
    }
}
