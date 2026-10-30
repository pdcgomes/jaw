//! Files written so that a crash, a kill, a full disk or a power cut leaves either the whole file
//! under its name or nothing there. Each is written to a partial file of its own beside its name,
//! `<name>.<pid>-<n>.part`, claimed so no other job can share it; synced (`F_FULLFSYNC` on macOS,
//! where `fsync` doesn't reach the disk); linked in under its name; and its folder synced so the
//! name lasts. A hard link, unlike a rename, never replaces a file: a name that's taken is left
//! as it is and the job says so, since a row may point to what's there. Folders made for a file
//! are synced too, so a power cut can't take a new folder from under a synced file.
//!
//! A derived file, which can be made again from what jaw keeps (a preview), skips the two syncs:
//! a crash or a kill still leaves it whole or not there, and a power cut may leave it missing,
//! empty or cut short, which `whole_preview` tells, so whoever points to one checks it first.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{Error, Result};

const SUFFIX: &str = ".part";

/// Numbers each partial file this process writes, so two jobs on one destination never share one.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// The names calls in flight are writing, which they may still remove until they return.
static CLAIMED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Held while a file is synced and put in place, so the jobs of every pool flush the disk one at a
/// time, and a synced commit of jawd's waits behind one file's flushes at most.
static FINISHING: Mutex<()> = Mutex::new(());

/// A call's hold on the names it writes, from when it starts until it has returned, so another
/// call on one of them is told it's busy rather than that it exists, and `Error::Exists` only
/// ever names a file whose call succeeded or that a crash left whole.
pub(crate) struct Claim(Vec<PathBuf>);

impl Claim {
    pub fn take<'a>(names: impl IntoIterator<Item = &'a Path>) -> Result<Claim> {
        let names: Vec<PathBuf> = names.into_iter().map(Path::to_path_buf).collect();
        let mut claimed = CLAIMED.lock().expect("claimed names");
        if let Some(busy) = names.iter().find(|n| claimed.contains(n)) {
            return Err(Error::Busy { path: busy.clone() });
        }
        claimed.extend(names.iter().cloned());
        Ok(Claim(names))
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        CLAIMED.lock().expect("claimed names").retain(|n| !self.0.contains(n));
    }
}

/// How a file is put in place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Durability {
    /// Synced to the disk before its name, and its folder after, for what can't be made again: a
    /// copy to send, a poster.
    #[default]
    Synced,
    /// Put in place without waiting for the disk, for what can be made again: a preview. A power
    /// cut may leave it missing, empty or cut short (`whole_preview`).
    Derived,
}

/// A file on its way to `to`. Dropped before it's finished, it takes its partial file with it.
pub(crate) struct Part {
    part: PathBuf,
    to: PathBuf,
    file: Option<File>,
    durability: Durability,
}

impl Part {
    /// Claims a partial file for `to`, making `to`'s folder if it's missing. Refuses a name that's
    /// taken, and a name that is itself a partial file's.
    pub fn create(to: &Path) -> Result<Part> {
        Part::create_as(to, Durability::Synced)
    }

    /// As `create`, put in place as `durability` says.
    pub fn create_as(to: &Path, durability: Durability) -> Result<Part> {
        let part = partial_for(to)?;
        let file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&part).map_err(Error::io(&part))?;
        Ok(Part { part, to: to.to_path_buf(), file: Some(file), durability })
    }

    /// A partial file for `to` that is a copy of `source`: a clone where the file system makes one,
    /// as APFS and btrfs do, which shares `source`'s blocks until either is written, and a copy of
    /// its bytes where it doesn't. It's this process's own, as a created one is, and mode 0600.
    pub fn copy_of(source: &Path, to: &Path) -> Result<Part> {
        let part = partial_for(to)?;
        let made = (|| -> std::io::Result<File> {
            let from = File::open(source)?;
            if !from.metadata()?.is_file() {
                return Err(std::io::Error::other("it isn't a file"));
            }
            let file = match clone(&from, &part)? {
                Some(file) => file,
                None => {
                    let mut file =
                        OpenOptions::new().read(true).write(true).create_new(true).mode(0o600).open(&part)?;
                    std::io::copy(&mut &from, &mut file)?;
                    file
                }
            };
            fault("copy")?;
            file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
            Ok(file)
        })();
        match made {
            Ok(file) => Ok(Part { part, to: to.to_path_buf(), file: Some(file), durability: Durability::Synced }),
            Err(error) => {
                if error.kind() != std::io::ErrorKind::AlreadyExists {
                    let _ = std::fs::remove_file(&part);
                }
                Err(Error::Io { path: source.to_path_buf(), error })
            }
        }
    }

    /// Where the file is written until it's whole.
    pub fn path(&self) -> &Path {
        &self.part
    }

    /// Writes the whole file with `write`, then puts it in place.
    pub fn write(mut self, write: impl FnOnce(&mut BufWriter<&File>) -> Result<()>) -> Result<u64> {
        let file = self.file.as_ref().expect("a part's file is open until it's finished");
        let mut out = BufWriter::new(file);
        write(&mut out)?;
        out.flush().map_err(Error::io(&self.part))?;
        drop(out);
        self.finish_mut()
    }

    /// Puts in place the partial file something else wrote, such as ffmpeg, which truncates and
    /// writes the file this part claimed rather than making one of its own.
    pub fn finish(mut self) -> Result<u64> {
        self.finish_mut()
    }

    fn finish_mut(&mut self) -> Result<u64> {
        let file = self.file.take().expect("a part is finished once");
        let result = self.put_in_place(&file);
        if result.is_err() {
            let _ = std::fs::remove_file(&self.part);
        }
        result
    }

    fn put_in_place(&self, file: &File) -> Result<u64> {
        let ours = file.metadata().map_err(Error::io(&self.part))?;
        let there = std::fs::symlink_metadata(&self.part).map_err(Error::io(&self.part))?;
        if (ours.dev(), ours.ino()) != (there.dev(), there.ino()) {
            return Err(Error::Io {
                path: self.part.clone(),
                error: std::io::Error::other("the partial file was replaced while it was written"),
            });
        }
        let synced = self.durability == Durability::Synced;
        let _finishing = synced.then(|| FINISHING.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
        if synced {
            sync_file(file, &self.part)?;
        }
        fault("link").map_err(Error::io(&self.part))?;
        match std::fs::hard_link(&self.part, &self.to) {
            Ok(()) => record(|| format!("link {} {}", self.part.display(), self.to.display())),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(Error::Exists { path: self.to.clone() });
            }
            Err(e) => return Err(Error::Io { path: self.to.clone(), error: e }),
        }
        let placed = std::fs::remove_file(&self.part)
            .map_err(Error::io(&self.part))
            .and_then(|()| fault("placed").map_err(Error::io(&self.to)))
            .and_then(|()| if synced { sync_folder(self.to.parent().unwrap_or(Path::new(""))) } else { Ok(()) });
        if let Err(e) = placed {
            let _ = std::fs::remove_file(&self.to);
            return Err(e);
        }
        Ok(ours.len())
    }
}

impl Drop for Part {
    fn drop(&mut self) {
        if self.file.take().is_some() {
            let _ = std::fs::remove_file(&self.part);
        }
    }
}

/// A partial file's name for `to`, beside it, once `to` is a name a file can be written to: not a
/// partial file's own, not taken, and in a folder that's there, made and synced if it wasn't.
fn partial_for(to: &Path) -> Result<PathBuf> {
    if to.as_os_str().to_string_lossy().ends_with(SUFFIX) {
        return Err(Error::Io {
            path: to.into(),
            error: std::io::Error::other("a file's name can't end in .part, which jaw keeps for files on their way"),
        });
    }
    if std::fs::symlink_metadata(to).is_ok() {
        return Err(Error::Exists { path: to.into() });
    }
    if let Some(folder) = to.parent().filter(|f| !f.as_os_str().is_empty()) {
        create_folders(folder)?;
    }
    let mut part = to.as_os_str().to_owned();
    part.push(format!(".{}-{}{SUFFIX}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    Ok(PathBuf::from(part))
}

/// Clones `from` as a new file at `to`, open for writing, where the file system can; None where it
/// can't, as on HFS+, ext4 or across file systems, and nothing is made then.
#[cfg(target_os = "macos")]
fn clone(from: &File, to: &Path) -> std::io::Result<Option<File>> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(to.as_os_str().as_bytes()).map_err(std::io::Error::other)?;
    // SAFETY: `from` is an open file and `name` a NUL-terminated path; fclonefileat makes the file at
    // `name`, or fails without making anything, and never follows a link there.
    if unsafe { libc::fclonefileat(from.as_raw_fd(), libc::AT_FDCWD, name.as_ptr(), 0) } != 0 {
        let error = std::io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ENOTSUP | libc::EXDEV | libc::ENOSYS) => Ok(None),
            _ => Err(error),
        };
    }
    OpenOptions::new().read(true).write(true).open(to).map(Some)
}

#[cfg(target_os = "linux")]
fn clone(from: &File, to: &Path) -> std::io::Result<Option<File>> {
    use std::os::fd::AsRawFd;
    let file = OpenOptions::new().read(true).write(true).create_new(true).mode(0o600).open(to)?;
    // SAFETY: both descriptors are open files; FICLONE makes `file` share `from`'s blocks, or fails
    // and leaves `file` empty.
    if unsafe { libc::ioctl(file.as_raw_fd(), libc::FICLONE as _, from.as_raw_fd()) } != 0 {
        let error = std::io::Error::last_os_error();
        let _ = std::fs::remove_file(to);
        return match error.raw_os_error() {
            Some(libc::EOPNOTSUPP | libc::EXDEV | libc::EINVAL | libc::ENOTTY | libc::EBADF) => Ok(None),
            _ => Err(error),
        };
    }
    Ok(Some(file))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn clone(_: &File, _: &Path) -> std::io::Result<Option<File>> {
    Ok(None)
}

/// Keeps a copy of `source`, someone else's file, at `to`, whole and synced: a clone where the file
/// system makes one, which costs no time or room, and a copy where it doesn't. Whatever happens to
/// `source` after, `to` stays as it was. Refuses a name that's taken (`Exists`), or that another
/// call is writing (`Busy`); says how many bytes it holds.
pub fn keep(source: &Path, to: &Path) -> Result<u64> {
    let _claim = Claim::take([to])?;
    Part::copy_of(source, to)?.finish()
}

/// Gives the whole, synced file at `file` a second name, `to`, and syncs `to`'s folder, so the name
/// lasts; refuses a name that's taken (`Exists`), which, where names come from the content, holds
/// the same file already. For putting a file made under a name of its own into a cache named by
/// content.
pub fn link(file: &Path, to: &Path) -> Result<()> {
    let _claim = Claim::take([to])?;
    if let Some(folder) = to.parent().filter(|f| !f.as_os_str().is_empty()) {
        create_folders(folder)?;
    }
    let _finishing = FINISHING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    match std::fs::hard_link(file, to) {
        Ok(()) => record(|| format!("link {} {}", file.display(), to.display())),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err(Error::Exists { path: to.into() }),
        Err(e) => return Err(Error::Io { path: to.into(), error: e }),
    }
    if let Err(e) = sync_folder(to.parent().unwrap_or(Path::new(""))) {
        let _ = std::fs::remove_file(to);
        return Err(e);
    }
    Ok(())
}

/// Makes `folder` and any folders above it that are missing, and syncs each one made and the
/// folder it was made in, deepest first.
fn create_folders(folder: &Path) -> Result<()> {
    let mut missing = Vec::new();
    let mut at = folder;
    while std::fs::symlink_metadata(at).is_err() {
        missing.push(at);
        match at.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => at = parent,
            _ => break,
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(folder).map_err(Error::io(folder))?;
    for made in &missing {
        sync_folder(made)?;
    }
    match missing.last().and_then(|top| top.parent()) {
        Some(parent) => sync_folder(parent),
        None => Ok(()),
    }
}

fn sync_file(file: &File, path: &Path) -> Result<()> {
    file.sync_all().map_err(Error::io(path))?;
    record(|| format!("sync {}", path.display()));
    Ok(())
}

fn sync_folder(folder: &Path) -> Result<()> {
    let folder = if folder.as_os_str().is_empty() { Path::new(".") } else { folder };
    File::open(folder).and_then(|f| f.sync_all()).map_err(Error::io(folder))?;
    record(|| format!("sync-folder {}", folder.display()));
    Ok(())
}

/// Whether `name` is a partial file's: `<name>.<pid>-<n>.part`.
fn is_partial(name: &str) -> bool {
    let Some(rest) = name.strip_suffix(SUFFIX) else { return false };
    let Some((_, tag)) = rest.rsplit_once('.') else { return false };
    let Some((pid, n)) = tag.split_once('-') else { return false };
    [pid, n].iter().all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

/// Removes the partial files a job killed with its process left in `folder` and the folders
/// below it; returns how many there were. Only for jaw's own folders, once jawd holds `jaw.lock`
/// and before its media pools start, since a running job's partial file looks the same.
pub fn remove_partial(folder: &Path) -> std::io::Result<usize> {
    let mut removed = 0;
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            removed += remove_partial(&entry.path())?;
        } else if kind.is_file() && is_partial(&entry.file_name().to_string_lossy()) {
            std::fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// The syncs and links done, for the tests that check their order.
#[cfg(test)]
pub(crate) static EVENTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
fn record(event: impl FnOnce() -> String) {
    EVENTS.lock().unwrap().push(event());
}

#[cfg(not(test))]
fn record(_: impl FnOnce() -> String) {}

#[cfg(test)]
thread_local! {
    /// The step of a file put in place on this thread that fails next, for the tests of what a
    /// failure leaves.
    static FAULT: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
}

/// Fails at `step` once, if a test asked for it on this thread.
#[cfg(test)]
fn fault(step: &str) -> std::io::Result<()> {
    match FAULT.with(|fault| fault.get()) {
        Some(at) if at == step => {
            FAULT.with(|fault| fault.set(None));
            Err(std::io::Error::other(format!("a fault the test made at {step}")))
        }
        _ => Ok(()),
    }
}

#[cfg(not(test))]
fn fault(_: &str) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The events recorded under `folder`, with its path taken out and each partial file's pid and
    /// number written as N.
    fn events(folder: &Path) -> Vec<String> {
        let folder = folder.display().to_string();
        let numberless = |word: &str| match word.strip_suffix(SUFFIX).and_then(|w| w.rsplit_once('.')) {
            Some((name, _)) if is_partial(word) => format!("{name}.N{SUFFIX}"),
            _ => word.to_string(),
        };
        EVENTS
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.contains(&folder))
            .map(|e| e.replace(&folder, "").split(' ').map(numberless).collect::<Vec<_>>().join(" "))
            .collect()
    }

    #[test]
    fn a_file_appears_under_its_name_only_once_whole() {
        let folder = tempfile::tempdir().unwrap();
        let to = folder.path().join("a.jpg");
        let part = Part::create(&to).unwrap();
        let partial = part.path().to_path_buf();
        let size = part
            .write(|out| {
                out.write_all(b"half").unwrap();
                assert!(!to.exists());
                out.write_all(b" and the rest").unwrap();
                Ok(())
            })
            .unwrap();
        assert_eq!((size, std::fs::read(&to).unwrap()), (17, b"half and the rest".to_vec()));
        assert!(!partial.exists());
    }

    #[test]
    fn the_file_is_synced_before_its_name_and_the_folders_made_for_it_after() {
        let folder = tempfile::tempdir().unwrap();
        let to = folder.path().join("media/ab/a.jpg");
        Part::create(&to).unwrap().write(|out| out.write_all(b"whole").map_err(Error::io(&to))).unwrap();
        assert_eq!(
            events(folder.path()),
            [
                "sync-folder /media/ab",
                "sync-folder /media",
                "sync-folder ",
                "sync /media/ab/a.jpg.N.part",
                "link /media/ab/a.jpg.N.part /media/ab/a.jpg",
                "sync-folder /media/ab",
            ]
        );
    }

    #[test]
    fn a_derived_file_is_put_in_place_whole_without_waiting_for_the_disk() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("previews")).unwrap();
        let to = folder.path().join("previews/a.256.jpg");
        let size = Part::create_as(&to, Durability::Derived)
            .unwrap()
            .write(|out| out.write_all(b"derived").map_err(Error::io(&to)))
            .unwrap();
        assert_eq!((size, std::fs::read(&to).unwrap()), (7, b"derived".to_vec()));
        assert_eq!(events(folder.path()), ["link /previews/a.256.jpg.N.part /previews/a.256.jpg"]);
        assert_eq!(std::fs::read_dir(folder.path().join("previews")).unwrap().count(), 1, "and no partial file");
        let taken = Part::create_as(&to, Durability::Derived);
        assert!(matches!(taken, Err(Error::Exists { .. })), "nor does it replace a name that's taken");
    }

    #[test]
    fn a_failed_write_leaves_nothing_behind() {
        let folder = tempfile::tempdir().unwrap();
        let to = folder.path().join("a.jpg");
        let failed = Part::create(&to).unwrap().write(|out| {
            out.write_all(b"half").unwrap();
            Err(Error::Cancelled)
        });
        assert!(matches!(failed, Err(Error::Cancelled)));
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_name_thats_taken_is_left_as_it_was_and_the_job_says_so() {
        let folder = tempfile::tempdir().unwrap();
        let to = folder.path().join("a.jpg");
        std::fs::write(&to, b"what a row points to").unwrap();
        assert!(matches!(Part::create(&to), Err(Error::Exists { .. })));

        let other = folder.path().join("b.jpg");
        let late = Part::create(&other).unwrap();
        std::fs::write(&other, b"what came first").unwrap();
        let result = late.write(|out| out.write_all(b"what came second").map_err(Error::io(&other)));
        assert!(matches!(result, Err(Error::Exists { .. })), "{result:?}");
        assert_eq!(std::fs::read(&other).unwrap(), b"what came first");
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 2, "and its partial file is gone");
    }

    #[test]
    fn two_jobs_on_one_destination_leave_one_whole_file_and_never_a_mix() {
        let folder = tempfile::tempdir().unwrap();
        let to = folder.path().join("a.mp4");
        let (a, b) = (Part::create(&to).unwrap(), Part::create(&to).unwrap());
        assert_ne!(a.path(), b.path());
        let (a_file, b_file) = (a.path().to_path_buf(), b.path().to_path_buf());
        std::fs::write(&a_file, b"aaaaaaaa").unwrap();
        std::fs::write(&b_file, b"BBBB").unwrap();
        assert_eq!(b.finish().unwrap(), 4);
        assert!(matches!(a.finish(), Err(Error::Exists { .. })));
        assert_eq!(std::fs::read(&to).unwrap(), b"BBBB");

        let (c, d) = (Part::create(&folder.path().join("c")).unwrap(), Part::create(&folder.path().join("c")).unwrap());
        let d_file = d.path().to_path_buf();
        drop(c);
        assert!(d_file.exists(), "a job that gives up removes only its own partial file");
    }

    #[test]
    fn a_name_a_call_is_writing_is_busy_until_the_call_lets_go() {
        let folder = tempfile::tempdir().unwrap();
        let (a, b) = (folder.path().join("a.jpg"), folder.path().join("b.jpg"));
        let claim = Claim::take([a.as_path(), b.as_path()]).unwrap();
        assert!(matches!(Claim::take([b.as_path()]), Err(Error::Busy { .. })));
        drop(claim);
        assert!(Claim::take([b.as_path()]).is_ok());
    }

    #[test]
    fn a_partial_file_replaced_while_it_was_written_is_never_put_in_place() {
        let folder = tempfile::tempdir().unwrap();
        let to = folder.path().join("a.mp4");
        let part = Part::create(&to).unwrap();
        std::fs::remove_file(part.path()).unwrap();
        std::fs::write(part.path(), b"someone else's").unwrap();
        assert!(part.finish().is_err());
        assert!(!to.exists());
    }

    #[test]
    fn names_ending_in_part_are_refused() {
        assert!(Part::create(Path::new("/tmp/a.part")).is_err());
        assert!(keep(Path::new("/etc/hosts"), Path::new("/tmp/a.part")).is_err());
    }

    #[test]
    fn a_kept_copy_is_whole_synced_before_its_name_and_its_own_whatever_happens_to_the_file_after() {
        let folder = tempfile::tempdir().unwrap();
        let yours = folder.path().join("holiday.jpg");
        std::fs::write(&yours, b"the photo you picked").unwrap();
        let kept = folder.path().join("media/outgoing/3EB0AA.jpg");
        assert_eq!(keep(&yours, &kept).unwrap(), 20);
        assert_eq!(std::fs::read(&kept).unwrap(), b"the photo you picked");
        let mode = std::fs::metadata(&kept).unwrap().permissions();
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777, 0o600);
        let order: Vec<String> = events(folder.path()).into_iter().filter(|e| e.contains("3EB0AA")).collect();
        assert_eq!(
            order,
            [
                "sync /media/outgoing/3EB0AA.jpg.N.part",
                "link /media/outgoing/3EB0AA.jpg.N.part /media/outgoing/3EB0AA.jpg",
            ]
        );
        // Written over where it is, or replaced, yours changes and jaw's copy doesn't.
        std::fs::OpenOptions::new().write(true).open(&yours).unwrap().write_all(b"THE PHOTO").unwrap();
        std::fs::write(&yours, b"another").unwrap();
        assert_eq!(std::fs::read(&kept).unwrap(), b"the photo you picked");
        assert_eq!(std::fs::read_dir(kept.parent().unwrap()).unwrap().count(), 1, "and no partial file");
    }

    #[test]
    fn keeping_a_copy_never_replaces_a_name_and_a_failure_leaves_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let yours = folder.path().join("a.pdf");
        std::fs::write(&yours, b"yours").unwrap();
        let kept = folder.path().join("kept.pdf");
        std::fs::write(&kept, b"what a row points to").unwrap();
        assert!(matches!(keep(&yours, &kept), Err(Error::Exists { .. })));
        assert_eq!(std::fs::read(&kept).unwrap(), b"what a row points to");
        let gone = keep(&folder.path().join("not there.pdf"), &folder.path().join("b.pdf"));
        assert!(matches!(gone, Err(Error::Io { .. })), "{gone:?}");
        assert!(keep(folder.path(), &folder.path().join("c")).is_err(), "a folder isn't a file to send");
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 2, "nothing more was left");
    }

    #[test]
    fn a_keep_that_fails_once_its_partial_file_is_there_leaves_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let yours = folder.path().join("yours.jpg");
        std::fs::write(&yours, b"the photo you picked").unwrap();
        // Once the partial file is a copy of yours, then once it's synced, then once it has its name.
        for step in ["copy", "link", "placed"] {
            FAULT.with(|fault| fault.set(Some(step)));
            let failed = keep(&yours, &folder.path().join(format!("kept-at-{step}.jpg")));
            assert!(failed.is_err(), "a fault at {step} came");
            let left: Vec<String> = std::fs::read_dir(folder.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert_eq!(left, ["yours.jpg"], "a keep that failed at {step} left {left:?}");
        }
        assert!(keep(&yours, &folder.path().join("kept.jpg")).is_ok(), "and one with no fault keeps it");
    }

    #[test]
    fn a_second_name_is_synced_into_its_folder_and_never_replaces_one() {
        let folder = tempfile::tempdir().unwrap();
        let made = folder.path().join("3EB0AA.sd.jpg");
        Part::create(&made).unwrap().write(|out| out.write_all(b"sd").map_err(Error::io(&made))).unwrap();
        let cached = folder.path().join("files/ab12.jpg");
        link(&made, &cached).unwrap();
        assert_eq!(std::fs::read(&cached).unwrap(), b"sd");
        let order = events(folder.path());
        assert_eq!(order[order.len() - 2..], ["link /3EB0AA.sd.jpg /files/ab12.jpg", "sync-folder /files"]);
        assert!(matches!(link(&made, &cached), Err(Error::Exists { .. })), "the name holds that file already");
    }

    #[test]
    fn files_are_synced_and_put_in_place_one_at_a_time_across_threads() {
        let folder = tempfile::tempdir().unwrap();
        std::thread::scope(|scope| {
            for n in 0..8 {
                let to = folder.path().join(format!("f{n}.jpg"));
                scope.spawn(move || {
                    for round in 0..4 {
                        let to = to.with_extension(format!("{round}.jpg"));
                        Part::create(&to).unwrap().write(|out| out.write_all(b"x").map_err(Error::io(&to))).unwrap();
                    }
                });
            }
        });
        // Each file's sync, link and folder sync come together, never another's between them.
        let order = events(folder.path());
        assert_eq!(order.len(), 8 * 4 * 3);
        for step in order.chunks(3) {
            let file = step[0].strip_prefix("sync ").and_then(|f| f.strip_suffix(".N.part")).unwrap();
            assert!(step[1].starts_with(&format!("link {file}.N.part")) && step[2] == "sync-folder ", "{order:#?}");
        }
    }

    #[test]
    fn partial_files_left_by_a_killed_job_are_removed_and_nothing_else() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("ab")).unwrap();
        std::fs::write(folder.path().join("a.mp4.123-4.part"), b"half").unwrap();
        std::fs::write(folder.path().join("ab/b.jpg.9-0.part"), b"half").unwrap();
        std::fs::write(folder.path().join("finished.part"), b"whole").unwrap();
        std::fs::write(folder.path().join("b.mp4"), b"whole").unwrap();
        assert_eq!(remove_partial(folder.path()).unwrap(), 2);
        assert!(folder.path().join("b.mp4").exists() && folder.path().join("finished.part").exists());
    }
}
