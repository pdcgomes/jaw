//! A recording's file, written as the design asks of every file written for the data: to a `.part`
//! beside its name, synced (`F_FULLFSYNC` on macOS, which std's `sync_all` uses there), linked in
//! under its name, and its directory synced, so the name only ever holds a whole file, after a crash
//! or a power cut too. Until then the file is only the `.part`, which an abandoned recording
//! removes. A recording never replaces a file: it refuses a name that's taken when it starts, and
//! a hard link, unlike a rename, fails rather than replacing one that appeared since.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use crate::Error;

pub(crate) struct PartFile {
    path: PathBuf,
    part: PathBuf,
    file: Option<BufWriter<File>>,
}

/// Where a recording for `path` is written until it's finished.
pub fn part_path(path: &Path) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(".part");
    PathBuf::from(name)
}

impl PartFile {
    pub(crate) fn create(path: &Path) -> Result<PartFile, Error> {
        if fs::symlink_metadata(path).is_ok() {
            return Err(Error::file(
                path,
                "record to",
                io::Error::other("something is already there, and jaw never records over it"),
            ));
        }
        let part = part_path(path);
        let file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&part).map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                Error::file(
                    &part,
                    "create",
                    io::Error::other("an earlier recording left it there; jawd removes those when it starts"),
                )
            } else {
                Error::file(&part, "create", e)
            }
        })?;
        Ok(PartFile { path: path.to_path_buf(), part, file: Some(BufWriter::new(file)) })
    }

    /// Syncs the file and puts it in place under its name.
    pub(crate) fn finish(mut self) -> Result<(), Error> {
        let Some(writer) = self.file.take() else {
            return Err(Error::file(&self.part, "finish", io::Error::other("it was already closed")));
        };
        let mut linked = false;
        let result = (|| {
            let file = writer.into_inner().map_err(|e| Error::file(&self.part, "write", e.into_error()))?;
            file.sync_all().map_err(|e| Error::file(&self.part, "sync", e))?;
            drop(file);
            fs::hard_link(&self.part, &self.path).map_err(|e| Error::file(&self.path, "put in place", e))?;
            linked = true;
            fs::remove_file(&self.part).map_err(|e| Error::file(&self.part, "remove", e))?;
            let dir = self.path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
            File::open(dir).and_then(|d| d.sync_all()).map_err(|e| Error::file(dir, "sync the folder of", e))
        })();
        if result.is_err() {
            // A recording that failed leaves nothing of its own behind, and only its own: the name
            // is removed only when the link made it this recording's.
            let _ = fs::remove_file(&self.part);
            if linked {
                let _ = fs::remove_file(&self.path);
            }
        }
        result
    }
}

impl Write for PartFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.as_mut().ok_or_else(|| io::Error::other("the recording's file is closed"))?.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.as_mut().map_or(Ok(()), |f| f.flush())
    }
}

impl Drop for PartFile {
    fn drop(&mut self) {
        if self.file.take().is_some() {
            let _ = fs::remove_file(&self.part);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_file_is_whole_under_its_name_and_its_part_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.ogg");
        let mut f = PartFile::create(&path).unwrap();
        f.write_all(b"whole").unwrap();
        assert!(!path.exists());
        assert!(part_path(&path).exists());
        f.finish().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"whole");
        assert!(!part_path(&path).exists());
        let mode = fs::metadata(&path).unwrap().permissions();
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777, 0o600);
    }

    #[test]
    fn an_abandoned_file_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.ogg");
        let mut f = PartFile::create(&path).unwrap();
        f.write_all(b"half").unwrap();
        drop(f);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_file_that_appears_under_the_name_meanwhile_is_left_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.ogg");
        let mut f = PartFile::create(&path).unwrap();
        f.write_all(b"recording").unwrap();
        fs::write(&path, b"someone else's").unwrap();
        let err = f.finish().unwrap_err().to_string();
        assert!(err.starts_with("couldn't put in place"), "{err}");
        assert_eq!(fs::read(&path).unwrap(), b"someone else's");
        assert!(!part_path(&path).exists());
    }

    #[test]
    fn a_name_thats_taken_is_refused_when_recording_starts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.ogg");
        fs::write(&path, b"a voice message an outbox row points to").unwrap();
        let err = PartFile::create(&path).err().unwrap().to_string();
        assert!(err.contains("never records over it"), "{err}");
        assert_eq!(fs::read(&path).unwrap(), b"a voice message an outbox row points to");
        assert!(!part_path(&path).exists());
    }

    #[test]
    fn a_part_left_by_an_earlier_recording_is_refused_rather_than_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.ogg");
        fs::write(part_path(&path), b"left by a crash").unwrap();
        let err = PartFile::create(&path).err().unwrap().to_string();
        assert!(err.contains("jawd removes those when it starts"), "{err}");
        assert_eq!(fs::read(part_path(&path)).unwrap(), b"left by a crash");
    }
}
