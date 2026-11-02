//! jaw.db's VFS: SQLite's own, with each sync SQLite asks made what it's for. Every connection to
//! jaw.db has fullfsync off and checkpoint_fullfsync on, so SQLite asks a commit's sync at
//! SQLITE_SYNC_NORMAL, and a checkpoint's two (the WAL's before it copies and jaw.db's after) and a
//! new WAL header's at SQLITE_SYNC_FULL, as pager.c composes walSyncFlags. A commit is synced so it
//! can be acked, so its sync is a full flush, F_FULLFSYNC on macOS, which SQLite's own VFS makes of
//! SQLITE_SYNC_FULL. The others need only what they sync to reach the disk before anything written
//! after them: on macOS, fsync and then one barrier (F_BARRIERFSYNC), which keeps everything sent
//! to the drive before it ahead of everything after, and leaves it to the next full flush, anyone's,
//! to make durable. A barrier orders one device and needs the drive's support, which Apple's own
//! drives give, so a file off the home folder's volume, or one whose barrier is refused, has those
//! flushed in full too. Elsewhere fsync is durable, so every sync is SQLite's full one.

use std::ffi::{CStr, c_int, c_void};
use std::path::Path;
use std::sync::OnceLock;

use rusqlite::{Connection, ffi};

/// The VFS's name, which every connection to jaw.db opens it through.
pub const NAME: &str = "jaw";

/// What a sync was made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Made {
    /// Flushed past the drive's cache.
    Full,
    /// Sent to the drive and fenced off from everything written after it.
    Fenced,
}

/// SQLite's own VFS, which does the work.
static REAL: OnceLock<usize> = OnceLock::new();

fn real() -> *mut ffi::sqlite3_vfs {
    *REAL.get().expect("the VFS is registered") as *mut ffi::sqlite3_vfs
}

/// One of the VFS's files: SQLite's own after it, in the space SQLite gives ours, and its folder,
/// open while barriers fence the file's syncs, or -1 while they're flushed in full.
#[repr(C)]
struct File {
    base: ffi::sqlite3_file,
    real: *mut ffi::sqlite3_file,
    folder: c_int,
    #[cfg(test)]
    path: &'static str,
}

/// Where SQLite's own file starts in the space SQLite gives one of ours.
const REAL_AT: usize = std::mem::size_of::<File>().next_multiple_of(16);

/// Registers the VFS, once; SQLite's own stays the default.
pub fn register() {
    REAL.get_or_init(|| {
        // SAFETY: the default VFS lives as long as the process; ours copies it, is leaked, and is
        // registered once.
        unsafe {
            let real = ffi::sqlite3_vfs_find(std::ptr::null());
            assert!(!real.is_null(), "SQLite has a default VFS");
            let mut ours = *real;
            ours.zName = c"jaw".as_ptr();
            ours.szOsFile = c_int::try_from(REAL_AT).expect("a small struct") + (*real).szOsFile;
            ours.pNext = std::ptr::null_mut();
            ours.xOpen = Some(open);
            assert_eq!(ffi::sqlite3_vfs_register(Box::leak(Box::new(ours)), 0), ffi::SQLITE_OK);
            real as usize
        }
    });
}

/// `db`'s main file, as SQLite says.
fn main_file(db: &Connection) -> Option<*mut ffi::sqlite3_file> {
    let mut file: *mut ffi::sqlite3_file = std::ptr::null_mut();
    // SAFETY: SQLITE_FCNTL_FILE_POINTER writes a pointer to the main database's file, which `db`
    // owns and keeps open while it is, into `file`.
    let found = unsafe {
        ffi::sqlite3_file_control(db.handle(), c"main".as_ptr(), ffi::SQLITE_FCNTL_FILE_POINTER, (&raw mut file).cast())
    };
    (found == ffi::SQLITE_OK && !file.is_null()).then_some(file)
}

/// Whether `db` was opened through this VFS, which makes a commit's sync a full flush.
pub fn opened_through(db: &Connection) -> bool {
    // SAFETY: the file is open while `db` is, and its methods are its VFS's.
    main_file(db).is_some_and(|file| unsafe { std::ptr::eq((*file).pMethods, &METHODS) })
}

/// Whether barriers fence the checkpoints' syncs of `db`'s files, rather than full flushes.
pub fn fenced(db: &Connection) -> bool {
    // SAFETY: as `opened_through`; a file with this VFS's methods is one `open` made.
    opened_through(db) && main_file(db).is_some_and(|file| unsafe { (*file.cast::<File>()).folder >= 0 })
}

unsafe extern "C" fn open(
    _: *mut ffi::sqlite3_vfs,
    name: ffi::sqlite3_filename,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out: *mut c_int,
) -> c_int {
    // SAFETY: SQLite gives `file` szOsFile bytes, REAL_AT of them ours and the rest its own file's,
    // and `name`, when there is one, is the file's whole path.
    unsafe {
        let inner = file.cast::<u8>().add(REAL_AT).cast::<ffi::sqlite3_file>();
        let real = real();
        let opened = ((*real).xOpen.expect("SQLite's own xOpen"))(real, name, inner, flags, out);
        let path = if name.is_null() { None } else { Some(CStr::from_ptr(name)) };
        let methods = if (*inner).pMethods.is_null() { std::ptr::null() } else { &METHODS };
        let folder = match path {
            Some(path) if opened == ffi::SQLITE_OK && !methods.is_null() => fence(path),
            _ => -1,
        };
        file.cast::<File>().write(File {
            base: ffi::sqlite3_file { pMethods: methods },
            real: inner,
            folder,
            #[cfg(test)]
            path: Box::leak(path.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default().into_boxed_str()),
        });
        opened
    }
}

/// The folder of the file at `path`, open for its barriers where they order its syncs: on macOS,
/// with the file on the home folder's volume, the Mac's own drive. -1 where they're flushed in
/// full, which is also where it can't tell.
fn fence(path: &CStr) -> c_int {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        let Some(folder) = Path::new(std::ffi::OsStr::from_bytes(path.to_bytes())).parent() else { return -1 };
        #[cfg(test)]
        if noted::flushed_in_full(folder) {
            return -1;
        }
        let home = std::env::var_os("HOME");
        if !home.is_some_and(|home| on_volume(Path::new(&home), folder)) {
            return -1;
        }
        let Ok(folder) = std::ffi::CString::new(folder.as_os_str().as_bytes()) else { return -1 };
        // SAFETY: open only reads the path, a C string that outlives the call.
        unsafe { libc::open(folder.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        -1
    }
}

/// Whether `folder` is on `own`'s volume; no where it can't tell.
fn on_volume(own: &Path, folder: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let device = |path: &Path| std::fs::metadata(path).map(|m| m.dev()).ok();
    device(own).is_some_and(|own| device(folder) == Some(own))
}

/// Makes the sync SQLite asked with `flags` of a file, through `sync`, SQLite's own for it: a
/// checkpoint's or a new WAL header's, at SQLITE_SYNC_FULL, sent to the drive and then fenced by
/// `barrier` where the file is `fenced` and the barrier is taken, and flushed in full otherwise;
/// a commit's, at SQLITE_SYNC_NORMAL, flushed in full. Says what it made, or SQLite's error.
fn make(
    flags: c_int,
    fenced: bool,
    mut sync: impl FnMut(c_int) -> c_int,
    barrier: impl FnOnce() -> bool,
) -> (c_int, Made) {
    let (asked, rest) = (flags & 0x0f, flags & !0x0f);
    if asked == ffi::SQLITE_SYNC_FULL && fenced {
        let sent = sync(rest | ffi::SQLITE_SYNC_NORMAL);
        if sent != ffi::SQLITE_OK || barrier() {
            return (sent, Made::Fenced);
        }
    }
    (sync(rest | ffi::SQLITE_SYNC_FULL), Made::Full)
}

/// The file SQLite's own VFS opened for one of ours, and its methods.
unsafe fn inner(file: *mut ffi::sqlite3_file) -> (*mut ffi::sqlite3_file, &'static ffi::sqlite3_io_methods) {
    // SAFETY: SQLite only calls these methods on files `open` made.
    unsafe {
        let real = (*file.cast::<File>()).real;
        (real, &*(*real).pMethods)
    }
}

/// Fences everything sent to `folder`'s drive so far off from everything written after, as
/// F_BARRIERFSYNC does; whether the drive took it.
fn barrier(folder: c_int) -> bool {
    #[cfg(target_os = "macos")]
    // SAFETY: the descriptor is a folder's, open while its file is, and the command takes no
    // argument.
    return unsafe { libc::fcntl(folder, libc::F_BARRIERFSYNC) } == 0;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = folder;
        false
    }
}

unsafe extern "C" fn sync(file: *mut ffi::sqlite3_file, flags: c_int) -> c_int {
    // SAFETY: as `inner`; the folder is open while the file is.
    unsafe {
        let folder = (*file.cast::<File>()).folder;
        let (real, methods) = inner(file);
        let own = methods.xSync.expect("xSync");
        let making = || make(flags, folder >= 0, |flags| own(real, flags), || barrier(folder));
        #[cfg(test)]
        let (synced, _) = noted::around((*file.cast::<File>()).path, making, |&(synced, made)| noted::Event::Sync {
            path: (*file.cast::<File>()).path.to_string(),
            thread: std::thread::current().name().unwrap_or_default().to_string(),
            asked: flags,
            made: (synced == ffi::SQLITE_OK).then_some(made),
        });
        #[cfg(test)]
        noted::linger((*file.cast::<File>()).path);
        #[cfg(not(test))]
        let (synced, _) = making();
        synced
    }
}

unsafe extern "C" fn close(file: *mut ffi::sqlite3_file) -> c_int {
    // SAFETY: as `inner`; the folder was opened for this file alone.
    unsafe {
        let folder = (*file.cast::<File>()).folder;
        let (real, methods) = inner(file);
        let closed = (methods.xClose.expect("xClose"))(real);
        if folder >= 0 {
            libc::close(folder);
        }
        closed
    }
}

unsafe extern "C" fn read(file: *mut ffi::sqlite3_file, buf: *mut c_void, amount: c_int, at: i64) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xRead.expect("xRead"))(real, buf, amount, at)
    }
}

unsafe extern "C" fn write(file: *mut ffi::sqlite3_file, buf: *const c_void, amount: c_int, at: i64) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        let wrote = || (methods.xWrite.expect("xWrite"))(real, buf, amount, at);
        #[cfg(test)]
        if noted::filled((*file.cast::<File>()).path) {
            return ffi::SQLITE_FULL;
        }
        #[cfg(test)]
        return noted::around((*file.cast::<File>()).path, wrote, |_| noted::Event::Write {
            path: (*file.cast::<File>()).path.to_string(),
            at,
            len: amount,
        });
        #[cfg(not(test))]
        wrote()
    }
}

unsafe extern "C" fn truncate(file: *mut ffi::sqlite3_file, size: i64) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        let truncated = || (methods.xTruncate.expect("xTruncate"))(real, size);
        #[cfg(test)]
        return noted::around((*file.cast::<File>()).path, truncated, |_| noted::Event::Truncate {
            path: (*file.cast::<File>()).path.to_string(),
            size,
        });
        #[cfg(not(test))]
        truncated()
    }
}

unsafe extern "C" fn file_size(file: *mut ffi::sqlite3_file, size: *mut i64) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xFileSize.expect("xFileSize"))(real, size)
    }
}

unsafe extern "C" fn lock(file: *mut ffi::sqlite3_file, level: c_int) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xLock.expect("xLock"))(real, level)
    }
}

unsafe extern "C" fn unlock(file: *mut ffi::sqlite3_file, level: c_int) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xUnlock.expect("xUnlock"))(real, level)
    }
}

unsafe extern "C" fn reserved(file: *mut ffi::sqlite3_file, out: *mut c_int) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xCheckReservedLock.expect("xCheckReservedLock"))(real, out)
    }
}

unsafe extern "C" fn control(file: *mut ffi::sqlite3_file, op: c_int, arg: *mut c_void) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xFileControl.expect("xFileControl"))(real, op, arg)
    }
}

unsafe extern "C" fn sector_size(file: *mut ffi::sqlite3_file) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xSectorSize.expect("xSectorSize"))(real)
    }
}

unsafe extern "C" fn characteristics(file: *mut ffi::sqlite3_file) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xDeviceCharacteristics.expect("xDeviceCharacteristics"))(real)
    }
}

unsafe extern "C" fn shm_map(
    file: *mut ffi::sqlite3_file,
    page: c_int,
    size: c_int,
    extend: c_int,
    out: *mut *mut c_void,
) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xShmMap.expect("xShmMap"))(real, page, size, extend, out)
    }
}

unsafe extern "C" fn shm_lock(file: *mut ffi::sqlite3_file, offset: c_int, n: c_int, flags: c_int) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xShmLock.expect("xShmLock"))(real, offset, n, flags)
    }
}

unsafe extern "C" fn shm_barrier(file: *mut ffi::sqlite3_file) {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xShmBarrier.expect("xShmBarrier"))(real)
    }
}

unsafe extern "C" fn shm_unmap(file: *mut ffi::sqlite3_file, delete: c_int) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xShmUnmap.expect("xShmUnmap"))(real, delete)
    }
}

unsafe extern "C" fn fetch(file: *mut ffi::sqlite3_file, at: i64, amount: c_int, out: *mut *mut c_void) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xFetch.expect("xFetch"))(real, at, amount, out)
    }
}

unsafe extern "C" fn unfetch(file: *mut ffi::sqlite3_file, at: i64, page: *mut c_void) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xUnfetch.expect("xUnfetch"))(real, at, page)
    }
}

static METHODS: ffi::sqlite3_io_methods = ffi::sqlite3_io_methods {
    iVersion: 3,
    xClose: Some(close),
    xRead: Some(read),
    xWrite: Some(write),
    xTruncate: Some(truncate),
    xSync: Some(sync),
    xFileSize: Some(file_size),
    xLock: Some(lock),
    xUnlock: Some(unlock),
    xCheckReservedLock: Some(reserved),
    xFileControl: Some(control),
    xSectorSize: Some(sector_size),
    xDeviceCharacteristics: Some(characteristics),
    xShmMap: Some(shm_map),
    xShmLock: Some(shm_lock),
    xShmBarrier: Some(shm_barrier),
    xShmUnmap: Some(shm_unmap),
    xFetch: Some(fetch),
    xUnfetch: Some(unfetch),
};

/// What a test sees of the VFS: each write, sync and truncate of a file in a folder it watches, in
/// the order the drive was asked them, and the folders whose files it has flushed in full, as off
/// the Mac's own drive.
#[cfg(test)]
pub mod noted {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::Made;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Event {
        Write {
            path: String,
            at: i64,
            len: i32,
        },
        /// A sync, with the flags SQLite asked it with and what it was made of, if it was.
        Sync {
            path: String,
            thread: String,
            asked: i32,
            made: Option<Made>,
        },
        Truncate {
            path: String,
            size: i64,
        },
    }

    impl Event {
        pub fn path(&self) -> &str {
            match self {
                Event::Write { path, .. } | Event::Sync { path, .. } | Event::Truncate { path, .. } => path,
            }
        }
    }

    static WATCHED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    static IN_FULL: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    static FILLED: Mutex<Vec<(PathBuf, Arc<AtomicU64>)>> = Mutex::new(Vec::new());
    static SLOW: Mutex<Vec<(PathBuf, Duration)>> = Mutex::new(Vec::new());
    static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());

    /// Has each sync the checkpointer asks of the files in `folder` take `by` longer, as on a busy
    /// drive, so a commit lands while one of its passes runs.
    pub fn slow(folder: &Path, by: Duration) {
        SLOW.lock().unwrap().push((named(folder), by));
    }

    /// Waits as long as `slow` asked for a sync of the file at `path`, on this thread.
    pub(super) fn linger(path: &str) {
        if std::thread::current().name() != Some("jawd-checkpoint") {
            return;
        }
        let by = SLOW.lock().unwrap().iter().find(|(f, _)| Path::new(path).starts_with(f)).map(|&(_, by)| by);
        if let Some(by) = by {
            std::thread::sleep(by);
        }
    }

    /// `folder` as SQLite names the files in it, its links followed.
    fn named(folder: &Path) -> PathBuf {
        std::fs::canonicalize(folder).expect("the test's folder")
    }

    /// Notes what the drive is asked of the files in `folder` from now on.
    pub fn watch(folder: &Path) {
        WATCHED.lock().unwrap().push(named(folder));
    }

    /// Has the files in `folder` opened from now on flushed in full, as off the Mac's own drive.
    pub fn flush_in_full(folder: &Path) {
        IN_FULL.lock().unwrap().push(named(folder));
    }

    pub(super) fn flushed_in_full(folder: &Path) -> bool {
        IN_FULL.lock().unwrap().iter().any(|f| f == folder)
    }

    /// Has writes to the files in `folder` fail as on a full disk while `free` is 0.
    pub fn fill(folder: &Path, free: Arc<AtomicU64>) {
        FILLED.lock().unwrap().push((named(folder), free));
    }

    pub(super) fn filled(path: &str) -> bool {
        FILLED
            .lock()
            .unwrap()
            .iter()
            .any(|(f, free)| Path::new(path).starts_with(f) && free.load(Ordering::Relaxed) == 0)
    }

    /// What was asked of the files in `folder`, in order.
    pub fn events(folder: &Path) -> Vec<Event> {
        let folder = named(folder);
        EVENTS.lock().unwrap().iter().filter(|e| Path::new(e.path()).starts_with(&folder)).cloned().collect()
    }

    /// Whether the drive `folder` is on takes a barrier, so a sync fenced there is made one.
    pub fn takes_barriers(folder: &Path) -> bool {
        let Ok(folder) = std::fs::File::open(folder) else { return false };
        super::barrier(std::os::fd::AsRawFd::as_raw_fd(&folder))
    }

    /// Runs `op` on the file at `path` and, if its folder is watched, notes `event` once it's
    /// done; both under one lock, so what's noted is in the order the drive was asked.
    pub(super) fn around<T>(path: &str, op: impl FnOnce() -> T, event: impl FnOnce(&T) -> Event) -> T {
        let watched = WATCHED.lock().unwrap().iter().any(|f| Path::new(path).starts_with(f));
        if !watched {
            return op();
        }
        let mut events = EVENTS.lock().unwrap();
        let done = op();
        events.push(event(&done));
        done
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// What `make` asks of SQLite's own sync and of the barrier, given whether the file is fenced
    /// and whether the barrier is taken.
    fn asked(flags: c_int, fenced: bool, taken: bool) -> (Vec<c_int>, bool, Made) {
        let (syncs, barrier) = (RefCell::new(Vec::new()), RefCell::new(false));
        let (synced, made) = make(
            flags,
            fenced,
            |flags| {
                syncs.borrow_mut().push(flags);
                ffi::SQLITE_OK
            },
            || {
                *barrier.borrow_mut() = true;
                taken
            },
        );
        assert_eq!(synced, ffi::SQLITE_OK);
        (syncs.into_inner(), barrier.into_inner(), made)
    }

    #[test]
    fn a_commits_sync_is_a_full_flush_and_a_checkpoints_is_fenced_only_where_barriers_hold() {
        let (full, normal, data) = (ffi::SQLITE_SYNC_FULL, ffi::SQLITE_SYNC_NORMAL, ffi::SQLITE_SYNC_DATAONLY);
        assert_eq!(asked(normal, true, true), (vec![full], false, Made::Full), "a commit's, fenced or not");
        assert_eq!(asked(normal, false, true), (vec![full], false, Made::Full));
        assert_eq!(asked(full, true, true), (vec![normal], true, Made::Fenced), "sent, then one barrier");
        assert_eq!(asked(full, true, false), (vec![normal, full], true, Made::Full), "a barrier refused");
        assert_eq!(asked(full, false, true), (vec![full], false, Made::Full), "off the Mac's own drive");
        assert_eq!(asked(full | data, true, true), (vec![normal | data], true, Made::Fenced), "its other flags kept");
        assert_eq!(asked(normal | data, true, true), (vec![full | data], false, Made::Full));
    }

    #[test]
    fn a_sync_that_cant_be_sent_fails_with_no_barrier() {
        let barrier = RefCell::new(false);
        let failed = make(
            ffi::SQLITE_SYNC_FULL,
            true,
            |_| ffi::SQLITE_IOERR_FSYNC,
            || {
                *barrier.borrow_mut() = true;
                true
            },
        );
        assert_eq!(failed.0, ffi::SQLITE_IOERR_FSYNC);
        assert!(!barrier.into_inner(), "a barrier over what never reached the drive");
    }

    #[test]
    fn a_barrier_is_asked_of_the_drive_and_one_it_cant_take_says_so() {
        assert!(!barrier(-1), "a barrier on no file at all");
        let folder = tempfile::tempdir().unwrap();
        let open = std::fs::File::open(folder.path()).unwrap();
        let fd = std::os::fd::AsRawFd::as_raw_fd(&open);
        assert_eq!(barrier(fd), cfg!(target_os = "macos") && noted::takes_barriers(folder.path()));
    }

    #[test]
    fn barriers_order_only_a_folder_on_the_home_folders_volume() {
        let home = tempfile::tempdir().unwrap();
        assert!(on_volume(home.path(), home.path()));
        assert!(!on_volume(home.path(), Path::new("/dev")), "a folder on another device");
        assert!(!on_volume(home.path(), &home.path().join("nowhere")), "one it can't look at");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_file_on_the_home_folders_volume_has_its_folder_to_fence_with_and_one_flushed_in_full_doesnt() {
        let home = std::env::var_os("HOME").expect("a home folder");
        let folder = tempfile::tempdir_in(&home).unwrap();
        let path = std::ffi::CString::new(folder.path().join("jaw.db").to_string_lossy().into_owned()).unwrap();
        let fd = fence(&path);
        assert!(fd >= 0, "the home folder's volume fences");
        // SAFETY: the descriptor is the folder's, and nothing else has it.
        unsafe { libc::close(fd) };
        noted::flush_in_full(folder.path());
        assert_eq!(fence(&path), -1, "a folder a test has flushed in full");
        assert_eq!(fence(c"/dev/null"), -1, "a file on another device");
    }
}
