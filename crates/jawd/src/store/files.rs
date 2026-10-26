//! What the read path knows of the files rows name, and tells media of the copies it shows. A
//! preview or thumbnail is derived and put in place without waiting for the disk, so a power cut
//! may leave one missing, empty or cut short: each a row names is checked once a run before a row
//! carries it, and one that isn't whole reads as missing and is reported, to be made again. A copy
//! shown without its thumbnail or file is reported too, so media can draw or fetch it then rather
//! than for every message history brings.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use tokio::sync::mpsc;

use super::media::Made;

/// The most derived files remembered as whole; past it, the memory starts again.
const REMEMBERED: usize = 100_000;

/// A copy a row showed, and what it lacked.
#[derive(Clone, Debug, PartialEq)]
pub struct Seen {
    /// Its message's n, and its file's hash.
    pub message: i64,
    pub sha256: Vec<u8>,
    /// Its file is in the cache.
    pub local: bool,
    /// Its message carries a thumbnail, and there's no whole one drawn of it yet.
    pub no_thumbnail: bool,
    /// Its state, as the row says: `remote`, `downloading`, `expired`, `waiting_for_phone` or `gone`.
    pub state: String,
    /// Derived files the row names that aren't whole, and their paths.
    pub broken: Vec<(Made, String)>,
}

/// The files of one jaw.db's folder, as its read path sees them.
pub struct Files {
    home: PathBuf,
    whole: Mutex<HashSet<String>>,
    seen: Mutex<Option<mpsc::Sender<Seen>>>,
}

/// Every open store's Files, by jaw.db's path, for the read path to find from its connection.
static OPEN: Mutex<Vec<(PathBuf, Weak<Files>)>> = Mutex::new(Vec::new());

impl Files {
    /// The Files for jaw.db at `path`, kept for as long as the returned Arc lives.
    pub(super) fn open(path: &Path) -> Arc<Files> {
        let files = Arc::new(Files {
            home: path.parent().map(Path::to_path_buf).unwrap_or_default(),
            whole: Mutex::new(HashSet::new()),
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

    /// Whether the derived file at `relative` (to jaw's folder) is whole, checked once a run.
    pub fn whole(&self, relative: &str) -> bool {
        if self.whole.lock().expect("whole files").contains(relative) {
            return true;
        }
        if !jaw_media::whole_preview(&self.home.join(relative)) {
            return false;
        }
        let mut whole = self.whole.lock().expect("whole files");
        if whole.len() >= REMEMBERED {
            whole.clear();
        }
        whole.insert(relative.to_string());
        true
    }

    /// Forgets which files were found whole, as a jawd started again after a power cut has.
    #[cfg(test)]
    pub fn forget_whole(&self) {
        self.whole.lock().expect("whole files").clear();
    }

    /// Tells media of a copy a row showed; one that finds no room is said again the next time.
    pub fn seen(&self, seen: Seen) {
        if let Some(watching) = self.seen.lock().expect("who watches").as_ref() {
            let _ = watching.try_send(seen);
        }
    }
}
