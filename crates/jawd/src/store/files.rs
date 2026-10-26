//! What the read path knows of the files rows name, and tells media of the copies it shows. A
//! preview or thumbnail is derived and put in place without waiting for the disk, so a power cut
//! may leave one missing, empty or cut short. Media sends them to the drive later and notes up to
//! which write they're durable; at a start, the files with writes past that are suspect, checked off
//! the read path, which shows their previews and thumbnail only once the check has passed them,
//! never opening a file itself. A copy shown without its thumbnail or file is reported, so media
//! can draw or fetch it then rather than for every message history brings.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use tokio::sync::mpsc;

/// A copy a row showed, and what it lacked.
#[derive(Clone, Debug, PartialEq)]
pub struct Seen {
    /// Its message's n, and its file's hash.
    pub message: i64,
    pub sha256: Vec<u8>,
    /// Its file is in the cache.
    pub local: bool,
    /// Its message carries a thumbnail, and there's none drawn of it yet.
    pub no_thumbnail: bool,
    /// Its state, as the row says: `remote`, `downloading`, `expired`, `waiting_for_phone` or `gone`.
    pub state: String,
}

/// The files of one jaw.db's folder, as its read path sees them.
pub struct Files {
    /// The files, by their hash in uppercase hex as SQLite's hex() gives it, whose previews or
    /// thumbnail a power cut may have broken and the check hasn't passed yet.
    suspect: Mutex<HashSet<String>>,
    any_suspect: AtomicBool,
    seen: Mutex<Option<mpsc::Sender<Seen>>>,
}

/// Every open store's Files, by jaw.db's path, for the read path to find from its connection.
static OPEN: Mutex<Vec<(PathBuf, Weak<Files>)>> = Mutex::new(Vec::new());

impl Files {
    /// The Files for jaw.db at `path`, kept for as long as the returned Arc lives.
    pub(super) fn open(path: &Path) -> Arc<Files> {
        let files = Arc::new(Files {
            suspect: Mutex::new(HashSet::new()),
            any_suspect: AtomicBool::new(false),
            seen: Mutex::new(None),
        });
        let mut open = OPEN.lock().expect("open stores");
        open.retain(|(_, files)| files.strong_count() > 0);
        open.push((path.to_path_buf(), Arc::downgrade(&files)));
        files
    }

    /// The Files of the jaw.db `db` is open on, if a store has it open.
    pub(super) fn of(db: &rusqlite::Connection) -> Option<Arc<Files>> {
        let path = Path::new(db.path()?);
        let open = OPEN.lock().expect("open stores");
        open.iter().rev().find(|(at, _)| at == path).and_then(|(_, files)| files.upgrade())
    }

    /// Sends what rows show to `seen`, from now on.
    pub fn watch(&self, seen: mpsc::Sender<Seen>) {
        *self.seen.lock().expect("who watches") = Some(seen);
    }

    /// The previews and thumbnails of these files, by hash, are shown only once the check has
    /// passed them, as a start finds them.
    pub fn suspect(&self, sha256s: impl IntoIterator<Item = Vec<u8>>) {
        let mut suspect = self.suspect.lock().expect("suspect files");
        suspect.extend(sha256s.into_iter().map(|sha256| hex(&sha256)));
        self.any_suspect.store(!suspect.is_empty(), Ordering::SeqCst);
    }

    /// The check has passed these files: they're whole, or mended.
    pub fn passed(&self, sha256s: &[Vec<u8>]) {
        let mut suspect = self.suspect.lock().expect("suspect files");
        for sha256 in sha256s {
            suspect.remove(&hex(sha256));
        }
        self.any_suspect.store(!suspect.is_empty(), Ordering::SeqCst);
    }

    /// How many files the check hasn't passed yet.
    pub fn suspects(&self) -> usize {
        self.suspect.lock().expect("suspect files").len()
    }

    /// Whether the previews and thumbnail of the file whose hash is `sha256`, in uppercase hex, can
    /// be shown.
    pub fn trusted(&self, sha256: &str) -> bool {
        !self.any_suspect.load(Ordering::SeqCst) || !self.suspect.lock().expect("suspect files").contains(sha256)
    }

    /// Tells media of a copy a row showed; one that finds no room is said again the next time.
    pub fn seen(&self, seen: Seen) {
        if let Some(watching) = self.seen.lock().expect("who watches").as_ref() {
            let _ = watching.try_send(seen);
        }
    }
}

fn hex(sha256: &[u8]) -> String {
    sha256.iter().map(|b| format!("{b:02X}")).collect()
}
