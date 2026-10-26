//! What the read path knows of the files rows name, and tells media of the copies it shows. A
//! preview or thumbnail is derived and put in place without waiting for the disk, so a power cut
//! may leave one missing, empty or cut short. Each write of them is numbered, and media sends them
//! to the drive and notes up to which number they're durable; at a start, those past it are
//! checked off the read path, which shows them only once the check has passed them, never opening a
//! file itself. A copy shown without its thumbnail or file is reported, so media can draw or fetch
//! it then rather than for every message history brings.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
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
    /// Previews and thumbnails numbered past `settled` and up to `made` when jawd started may be
    /// broken; those the check hasn't passed, up to `checked`, aren't shown.
    settled: AtomicI64,
    made: AtomicI64,
    checked: AtomicI64,
    seen: Mutex<Option<mpsc::Sender<Seen>>>,
}

/// Every open store's Files, by jaw.db's path, for the read path to find from its connection.
static OPEN: Mutex<Vec<(PathBuf, Weak<Files>)>> = Mutex::new(Vec::new());

impl Files {
    /// The Files for jaw.db at `path`, kept for as long as the returned Arc lives.
    pub(super) fn open(path: &Path) -> Arc<Files> {
        let files = Arc::new(Files {
            settled: AtomicI64::new(0),
            made: AtomicI64::new(0),
            checked: AtomicI64::new(0),
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

    /// Previews and thumbnails numbered past `settled` and up to `made` are shown only once the
    /// check has passed them, as a start finds them.
    pub fn suspect(&self, settled: i64, made: i64) {
        self.checked.store(settled, Ordering::SeqCst);
        self.made.store(made, Ordering::SeqCst);
        self.settled.store(settled, Ordering::SeqCst);
    }

    /// The check has passed every number up to `through`.
    pub fn checked(&self, through: i64) {
        self.checked.fetch_max(through, Ordering::SeqCst);
    }

    /// Whether a file's previews and thumbnail, last written as number `made`, can be shown.
    pub fn trusted(&self, made: i64) -> bool {
        made <= self.checked.load(Ordering::SeqCst)
            || made <= self.settled.load(Ordering::SeqCst)
            || made > self.made.load(Ordering::SeqCst)
    }

    /// Tells media of a copy a row showed; one that finds no room is said again the next time.
    pub fn seen(&self, seen: Seen) {
        if let Some(watching) = self.seen.lock().expect("who watches").as_ref() {
            let _ = watching.try_send(seen);
        }
    }
}
