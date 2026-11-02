//! The recordings folder, `voice/recordings` in jaw's folder, mode 0700, where each recording goes
//! under a name of its own, the ID made up front for the message it may become, so no two share
//! one and none is used again. jaw-voice writes a recording to a `.part` beside its name and puts
//! it in place with a hard link once it's synced, so the folder must take hard links, which the
//! start checks once. A recording lasts while jawd runs, and no row ever names one, so a start
//! removes whatever a stopped jawd left there, whole or partial.

use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Where recordings go, under jaw's folder.
pub(super) const RECORDINGS: &str = "voice/recordings";

pub(super) struct Folder {
    path: PathBuf,
    /// Whether a hard link could be made there, as putting a recording in place needs.
    pub(super) hard_links: bool,
    /// What a stopped jawd left, removed as this one started.
    pub(super) removed_at_start: u64,
}

impl Folder {
    /// Makes the folder and the one above it, each synced into its parent, empties it, and checks
    /// that it takes hard links.
    pub(super) fn prepare(home: &Path) -> std::io::Result<Folder> {
        let path = home.join(RECORDINGS);
        for (folder, parent) in [(home.join("voice"), home.to_path_buf()), (path.clone(), home.join("voice"))] {
            match std::fs::DirBuilder::new().mode(0o700).create(&folder) {
                Ok(()) => std::fs::File::open(&parent)?.sync_all()?,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))?
                }
                Err(e) => return Err(e),
            }
        }
        let mut removed_at_start = 0;
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                std::fs::remove_file(entry.path())?;
                removed_at_start += 1;
            }
        }
        if removed_at_start > 0 {
            std::fs::File::open(&path)?.sync_all()?;
        }
        let hard_links = links(&path);
        Ok(Folder { path, hard_links, removed_at_start })
    }

    /// Where recording `id` goes.
    pub(super) fn path_for(&self, id: &str) -> PathBuf {
        self.path.join(format!("{id}.ogg"))
    }

    /// Removes the recording at `path`, and the partial file a recording cut short would leave
    /// beside it, once nothing needs it.
    pub(super) fn remove(&self, path: &Path) {
        if path.parent() != Some(self.path.as_path()) {
            return;
        }
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(jaw_voice::part_path(path));
    }

    /// How many files are there, and their bytes.
    pub(super) fn counts(&self) -> (u64, u64) {
        let Ok(entries) = std::fs::read_dir(&self.path) else { return (0, 0) };
        entries
            .filter_map(|e| e.ok()?.metadata().ok())
            .filter(|m| m.is_file())
            .fold((0, 0), |(files, bytes), m| (files + 1, bytes + m.len()))
    }
}

/// Whether a hard link can be made in `folder`: one made and removed, with the file it names.
fn links(folder: &Path) -> bool {
    let (file, link) = (folder.join(".links"), folder.join(".linked"));
    let linked = std::fs::write(&file, b"").and_then(|()| std::fs::hard_link(&file, &link)).is_ok();
    let _ = std::fs::remove_file(&link);
    let _ = std::fs::remove_file(&file);
    linked
}
