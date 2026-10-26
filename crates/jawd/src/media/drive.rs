//! Sending files to the drive in an order that holds across a power cut. On macOS fsync stops at
//! the drive's cache and orders nothing, so what must reach the disk before something else is sent
//! with fsync, then fenced with one barrier (F_BARRIERFSYNC): nothing written after it can reach
//! the disk first, and the next F_FULLFSYNC, by anyone, makes it durable. A barrier orders one
//! device, and needs the drive's support, which Apple's own drives give; so where jaw's folder, its
//! media or jaw.db sit on another volume than the home folder's, the barrier is a full flush
//! instead. Elsewhere fsync is durable itself.

use std::fs::File;
use std::io;
#[cfg(target_os = "macos")]
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Arc;

/// What's asked of the drive for a file or a folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flush {
    /// Its data and attributes sent to the drive: fsync on macOS, durable elsewhere.
    Push,
    /// What was pushed before, fenced off from everything written after.
    Barrier,
}

/// What asks the drive for a flush of the file or folder at a path.
type Asks = dyn Fn(Flush, &Path) -> io::Result<()> + Send + Sync;

/// How a settle sends files to the drive: the system's calls, or a test's stand-in that records them.
#[derive(Clone)]
pub struct Drive(Arc<Asks>);

impl Drive {
    /// The system's calls, with full flushes for barriers when `full`.
    pub fn system(full: bool) -> Drive {
        Drive(Arc::new(move |flush, path| {
            let file = File::open(path)?;
            match flush {
                Flush::Push => push(&file),
                Flush::Barrier => barrier(&file, full),
            }
        }))
    }

    /// A stand-in for the drive, as a test makes one.
    #[cfg(test)]
    pub fn stand_in(send: impl Fn(Flush, &Path) -> io::Result<()> + Send + Sync + 'static) -> Drive {
        Drive(Arc::new(send))
    }

    pub fn send(&self, flush: Flush, path: &Path) -> io::Result<()> {
        (self.0)(flush, path)
    }
}

impl std::fmt::Debug for Drive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Drive")
    }
}

/// Sends what's written to `file` to the drive: fsync on macOS, which stops at the drive's cache,
/// so a barrier or a full flush must follow; fdatasync elsewhere, which is durable.
pub fn push(file: &File) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: the descriptor is the open file's, which outlives the call.
        if unsafe { libc::fsync(file.as_raw_fd()) } == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
    }
    #[cfg(not(target_os = "macos"))]
    file.sync_data()
}

/// Fences off what was pushed to the drive before from everything written after, `file`'s own
/// writes first: F_BARRIERFSYNC on macOS, or F_FULLFSYNC with `full` or where the drive has no
/// barrier; fsync elsewhere, which is durable and so ordered.
pub fn barrier(file: &File, full: bool) -> io::Result<()> {
    // SAFETY: the descriptor is the open file's, which outlives the call, and each command takes
    // no argument.
    barrier_by(file, full, |fd, command| unsafe { libc::fcntl(fd, command) })
}

/// `barrier`, with the fcntl it calls given, as a test sees what it asks.
fn barrier_by(file: &File, full: bool, fcntl: impl Fn(i32, i32) -> i32) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let fd = file.as_raw_fd();
        if !full && fcntl(fd, libc::F_BARRIERFSYNC) == 0 {
            return Ok(());
        }
        if fcntl(fd, libc::F_FULLFSYNC) == 0 {
            return Ok(());
        }
        Err(io::Error::last_os_error())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (full, &fcntl);
        file.sync_all()
    }
}

/// Whether a barrier orders everything jaw writes: jaw's folder, its media folders and jaw.db all
/// on the one volume the home folder is on, the Mac's own drive. Where it can't tell, it says no.
pub fn barriers_hold(jaw: &Path, folders: &[&Path]) -> bool {
    std::env::var_os("HOME").is_some_and(|home| all_on(Path::new(&home), jaw, folders))
}

/// Whether jaw's folder, jaw.db and `folders` are all on `own`'s volume.
fn all_on(own: &Path, jaw: &Path, folders: &[&Path]) -> bool {
    let device = |path: &Path| std::fs::metadata(path).map(|m| m.dev()).ok();
    let Some(own) = device(own) else { return false };
    let mut all = vec![jaw.to_path_buf(), jaw.join("jaw.db")];
    all.extend(folders.iter().map(|folder| folder.to_path_buf()));
    all.iter().all(|path| device(path) == Some(own))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn a_barrier_asks_the_drive_for_one_and_a_full_flush_only_where_it_must() {
        let file = tempfile::tempfile().unwrap();
        let asked = Mutex::new(Vec::new());
        let answering = |ok: bool| {
            let asked = &asked;
            move |_: i32, command: i32| {
                asked.lock().unwrap().push(command);
                if ok || command == libc::F_FULLFSYNC { 0 } else { -1 }
            }
        };
        barrier_by(&file, false, answering(true)).unwrap();
        assert_eq!(*asked.lock().unwrap(), [libc::F_BARRIERFSYNC], "one barrier, and nothing more");
        asked.lock().unwrap().clear();
        barrier_by(&file, false, answering(false)).unwrap();
        assert_eq!(
            *asked.lock().unwrap(),
            [libc::F_BARRIERFSYNC, libc::F_FULLFSYNC],
            "a full flush where it's refused"
        );
        asked.lock().unwrap().clear();
        barrier_by(&file, true, answering(true)).unwrap();
        assert_eq!(*asked.lock().unwrap(), [libc::F_FULLFSYNC], "and a full flush off the Mac's own drive");
        barrier(&file, false).unwrap();
    }

    #[test]
    fn barriers_hold_only_with_everything_on_the_home_folders_volume() {
        let jaw = tempfile::tempdir().unwrap();
        std::fs::write(jaw.path().join("jaw.db"), b"").unwrap();
        let media = jaw.path().join("media");
        std::fs::create_dir(&media).unwrap();
        assert!(all_on(jaw.path(), jaw.path(), &[&media]));
        assert!(!all_on(jaw.path(), jaw.path(), &[Path::new("/dev")]), "a folder on another device");
        assert!(!all_on(Path::new("/dev"), jaw.path(), &[&media]), "jaw's folder off the home folder's volume");
        assert!(!all_on(jaw.path(), jaw.path(), &[&jaw.path().join("nowhere")]), "one it can't look at");
    }
}
