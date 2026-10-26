//! A VFS for tests: SQLite's own, with every sync SQLite asks of a file noted, with its flags and
//! the thread that asked, so a test can tell how each commit and checkpoint reached the disk. On
//! macOS a sync with SQLITE_SYNC_FULL is F_FULLFSYNC, the only one that gets past the drive's cache.

use std::ffi::{CStr, c_int, c_void};
use std::sync::{Mutex, OnceLock};

use rusqlite::ffi;

/// The VFS's name, which `connect` opens jaw.db through while a test asks it to.
pub const NAME: &str = "jaw-syncs";

/// A sync SQLite asked of a file.
#[derive(Clone, Debug)]
pub struct Synced {
    pub thread: String,
    pub path: String,
    pub flags: c_int,
}

impl Synced {
    /// Whether it flushes the drive's cache too, as F_FULLFSYNC does.
    pub fn full(&self) -> bool {
        self.flags & 0x0f == ffi::SQLITE_SYNC_FULL
    }
}

static SYNCED: Mutex<Vec<Synced>> = Mutex::new(Vec::new());

/// Every sync asked of a file opened through the VFS so far, in order.
pub fn synced() -> Vec<Synced> {
    SYNCED.lock().expect("the syncs noted").clone()
}

/// SQLite's own VFS, which does the work.
static REAL: OnceLock<usize> = OnceLock::new();

fn real() -> *mut ffi::sqlite3_vfs {
    *REAL.get().expect("the VFS is registered") as *mut ffi::sqlite3_vfs
}

/// One of the VFS's files: SQLite's own after it, in the space SQLite gives ours, and its path.
#[repr(C)]
struct File {
    base: ffi::sqlite3_file,
    real: *mut ffi::sqlite3_file,
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
            ours.zName = c"jaw-syncs".as_ptr();
            ours.szOsFile = c_int::try_from(REAL_AT).expect("a small struct") + (*real).szOsFile;
            ours.pNext = std::ptr::null_mut();
            ours.xOpen = Some(open);
            assert_eq!(ffi::sqlite3_vfs_register(Box::leak(Box::new(ours)), 0), ffi::SQLITE_OK);
            real as usize
        }
    });
}

unsafe extern "C" fn open(
    _: *mut ffi::sqlite3_vfs,
    name: ffi::sqlite3_filename,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out: *mut c_int,
) -> c_int {
    // SAFETY: SQLite gives `file` szOsFile bytes, REAL_AT of them ours and the rest its own file's.
    unsafe {
        let inner = file.cast::<u8>().add(REAL_AT).cast::<ffi::sqlite3_file>();
        let real = real();
        let opened = ((*real).xOpen.expect("SQLite's own xOpen"))(real, name, inner, flags, out);
        let path: &'static str = if name.is_null() {
            ""
        } else {
            Box::leak(CStr::from_ptr(name).to_string_lossy().into_owned().into_boxed_str())
        };
        let methods = if (*inner).pMethods.is_null() { std::ptr::null() } else { &METHODS };
        file.cast::<File>().write(File { base: ffi::sqlite3_file { pMethods: methods }, real: inner, path });
        opened
    }
}

/// The file SQLite's own VFS opened for one of ours, and its methods.
unsafe fn inner(file: *mut ffi::sqlite3_file) -> (*mut ffi::sqlite3_file, &'static ffi::sqlite3_io_methods) {
    // SAFETY: SQLite only calls these methods on files `open` made.
    unsafe {
        let real = (*file.cast::<File>()).real;
        (real, &*(*real).pMethods)
    }
}

unsafe extern "C" fn sync(file: *mut ffi::sqlite3_file, flags: c_int) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let path = (*file.cast::<File>()).path.to_string();
        let thread = std::thread::current().name().unwrap_or_default().to_string();
        SYNCED.lock().expect("the syncs noted").push(Synced { thread, path, flags });
        let (real, methods) = inner(file);
        (methods.xSync.expect("xSync"))(real, flags)
    }
}

unsafe extern "C" fn close(file: *mut ffi::sqlite3_file) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xClose.expect("xClose"))(real)
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
        (methods.xWrite.expect("xWrite"))(real, buf, amount, at)
    }
}

unsafe extern "C" fn truncate(file: *mut ffi::sqlite3_file, size: i64) -> c_int {
    // SAFETY: as `inner`.
    unsafe {
        let (real, methods) = inner(file);
        (methods.xTruncate.expect("xTruncate"))(real, size)
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
