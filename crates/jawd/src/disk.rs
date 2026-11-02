//! The disk watchdog: the free space on the disk jaw's folder is on, read with statvfs each time
//! background work or a download is about to start. Below the floor, 2 GB or 5% of the disk where
//! that's less, they wait and interfaces are told, until the disk has the floor and a margin more;
//! inbox rows and live writes never wait for it. A ballast of 64 MB kept in jaw's folder is given up
//! when a full disk refuses a write to jaw.db, so the commit, checkpoint or rollback tried again has
//! room, and is made again once the disk has room for it beside the floor.

use std::ffi::CString;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use jaw_proto::Push;
use jaw_proto::sync::{Notice, Tone};
use tokio::sync::{broadcast, watch};

/// The ballast's name in jaw's folder, and the name it's made under until it's whole.
const BALLAST: &str = "ballast";
const PARTIAL: &str = "ballast.partial";
/// How often the watchdog looks at the disk by itself, making the ballast again once there's room.
const EVERY: Duration = Duration::from_secs(10);
/// How often whatever waits for room looks at the disk again.
pub const POLL: Duration = Duration::from_millis(250);

/// The watchdog's limits; the defaults are the design's.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Background work and downloads wait below this much free, or below `floor_percent` of the
    /// disk where that's less.
    pub floor: u64,
    pub floor_percent: u64,
    /// How far above the floor the disk must be before they go on again, so they don't stop and
    /// start at its edge.
    pub margin: u64,
    /// The ballast's size.
    pub ballast: u64,
}

impl Default for Config {
    fn default() -> Config {
        Config { floor: 2_000_000_000, floor_percent: 5, margin: 256_000_000, ballast: 64_000_000 }
    }
}

impl Config {
    /// The floor on a disk of `size` bytes.
    pub fn floor(&self, size: u64) -> u64 {
        self.floor.min(size / 100 * self.floor_percent)
    }
}

/// Where the free space comes from.
enum Source {
    /// Nowhere: a store opened without a watchdog, whose disk is never short of room.
    None,
    /// The disk this folder is on.
    System(PathBuf),
    /// What a test sets.
    #[cfg(test)]
    Fake(Arc<Fake>),
}

/// A disk's free space and size, as a test sets them.
#[cfg(test)]
#[derive(Default)]
pub struct Fake {
    pub free: Arc<AtomicU64>,
    pub size: AtomicU64,
}

/// The watchdog of the disk jaw's folder is on, shared by everything that waits for its room.
#[derive(Clone)]
pub struct Disk(Arc<Inner>);

struct Inner {
    source: Source,
    config: Config,
    /// The ballast's path, where one is kept.
    ballast: Option<PathBuf>,
    /// The free space and size last read.
    free: AtomicU64,
    size: AtomicU64,
    /// Why background work and downloads wait, while they do.
    paused: watch::Sender<Option<String>>,
    given_up: AtomicU64,
    waits: AtomicU64,
    waited_ms: AtomicU64,
}

impl Disk {
    /// The disk `home` is on, watched with `config`, with its ballast in `home`.
    pub fn watch(home: &Path, config: Config) -> Disk {
        Disk::new(Source::System(home.to_path_buf()), config, Some(home.join(BALLAST)))
    }

    /// A disk that's never short of room and keeps no ballast.
    pub fn unwatched() -> Disk {
        Disk::new(Source::None, Config::default(), None)
    }

    /// As `watch`, with the free space and size `fake` says.
    #[cfg(test)]
    pub fn faked(home: &Path, config: Config, free: u64, size: u64) -> (Disk, Arc<Fake>) {
        let fake = Arc::new(Fake { free: Arc::new(AtomicU64::new(free)), size: AtomicU64::new(size) });
        (Disk::new(Source::Fake(fake.clone()), config, Some(home.join(BALLAST))), fake)
    }

    fn new(source: Source, config: Config, ballast: Option<PathBuf>) -> Disk {
        let (paused, _) = watch::channel(None);
        Disk(Arc::new(Inner {
            source,
            config,
            ballast,
            free: AtomicU64::new(0),
            size: AtomicU64::new(0),
            paused,
            given_up: AtomicU64::new(0),
            waits: AtomicU64::new(0),
            waited_ms: AtomicU64::new(0),
        }))
    }

    /// The free space and size of the disk now, if they can be read.
    fn read(&self) -> Option<(u64, u64)> {
        match &self.0.source {
            Source::None => None,
            Source::System(folder) => free_space(folder),
            #[cfg(test)]
            Source::Fake(fake) => Some((fake.free.load(Ordering::Relaxed), fake.size.load(Ordering::Relaxed))),
        }
    }

    /// Whether the disk has room for background work and downloads, as it has now. One whose free
    /// space can't be read has.
    pub fn room(&self) -> bool {
        let Some((free, size)) = self.read() else { return true };
        let inner = &self.0;
        inner.free.store(free, Ordering::Relaxed);
        inner.size.store(size, Ordering::Relaxed);
        let floor = inner.config.floor(size);
        inner.paused.send_if_modified(|paused| match paused {
            None if free < floor => {
                let why = format!(
                    "jaw's disk has {} free, under the {} jaw keeps free, so history, downloads and other background work wait until it has more; messages still arrive and send",
                    gb(free),
                    gb(floor)
                );
                eprintln!("jawd: {why}");
                *paused = Some(why);
                true
            }
            Some(_) if free >= floor.saturating_add(inner.config.margin) => {
                eprintln!("jawd: jaw's disk has {} free again, so background work and downloads go on", gb(free));
                *paused = None;
                true
            }
            _ => false,
        });
        inner.paused.borrow().is_none()
    }

    /// Waits until the disk has room, looking again each time what's waited for changes and every
    /// POLL.
    pub async fn wait_for_room(&self) {
        let mut paused = self.0.paused.subscribe();
        while !self.room() {
            let _ = tokio::time::timeout(POLL, paused.changed()).await;
        }
    }

    /// Why background work and downloads wait, while they do.
    pub fn paused(&self) -> Option<String> {
        self.0.paused.borrow().clone()
    }

    /// Why background work and downloads wait, each time that changes.
    pub fn changes(&self) -> watch::Receiver<Option<String>> {
        self.0.paused.subscribe()
    }

    /// A background transaction waited this long for room.
    pub fn waited(&self, waited: Duration) {
        self.0.waits.fetch_add(1, Ordering::Relaxed);
        self.0.waited_ms.fetch_add(waited.as_millis() as u64, Ordering::Relaxed);
    }

    /// A write to jaw.db failed for a full disk: the ballast goes, if it's kept, so the write tried
    /// again has its room.
    pub fn full(&self) {
        let Some(path) = &self.0.ballast else { return };
        match std::fs::remove_file(path) {
            Ok(()) => {
                self.0.given_up.fetch_add(1, Ordering::Relaxed);
                eprintln!(
                    "jawd: jaw's disk is full, so jawd gave up its ballast's {} for jaw.db",
                    mb(self.0.config.ballast)
                );
                #[cfg(test)]
                if let Source::Fake(fake) = &self.0.source {
                    fake.free.fetch_add(self.0.config.ballast, Ordering::Relaxed);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => eprintln!("jawd: can't give up the ballast, {}: {e}", path.display()),
        }
    }

    /// One look at the disk: whether it has room, and the ballast made again if it isn't kept and
    /// the disk has room for it beside the floor.
    pub async fn tick(&self) {
        self.room();
        if self.0.ballast.is_some() {
            let disk = self.clone();
            let _ = tokio::task::spawn_blocking(move || disk.keep_ballast()).await;
        }
    }

    /// Looks at the disk every EVERY until jawd stops, the first time now.
    pub fn start(&self) {
        let disk = self.clone();
        tokio::spawn(async move {
            loop {
                disk.tick().await;
                tokio::time::sleep(EVERY).await;
            }
        });
    }

    /// Makes the ballast unless it's kept at its size or the disk hasn't room for it; a partial one,
    /// left by a stop while it was made, goes.
    fn keep_ballast(&self) {
        let Some(path) = &self.0.ballast else { return };
        let bytes = self.0.config.ballast;
        let partial = path.with_file_name(PARTIAL);
        let _ = std::fs::remove_file(&partial);
        if std::fs::metadata(path).is_ok_and(|kept| kept.len() == bytes) {
            return;
        }
        let (free, size) = (self.0.free.load(Ordering::Relaxed), self.0.size.load(Ordering::Relaxed));
        if self.paused().is_some() || free < self.0.config.floor(size).saturating_add(bytes) {
            return;
        }
        let _ = std::fs::remove_file(path);
        let made = make(&partial, bytes).and_then(|()| std::fs::rename(&partial, path));
        match made {
            Ok(()) =>
            {
                #[cfg(test)]
                if let Source::Fake(fake) = &self.0.source {
                    fake.free.fetch_sub(bytes, Ordering::Relaxed);
                }
            }
            Err(e) => {
                let _ = std::fs::remove_file(&partial);
                eprintln!("jawd: can't make the ballast, {}: {e}", path.display());
            }
        }
    }

    /// What `status` says of the disk, if it's watched.
    pub fn status(&self) -> Option<jaw_proto::Disk> {
        if matches!(self.0.source, Source::None) {
            return None;
        }
        let size = self.0.size.load(Ordering::Relaxed);
        let ballast = self.0.ballast.as_ref().and_then(|path| std::fs::metadata(path).ok()).map_or(0, |m| m.len());
        Some(jaw_proto::Disk {
            free: self.0.free.load(Ordering::Relaxed),
            size,
            floor: self.0.config.floor(size),
            paused: self.paused(),
            ballast,
            ballast_given_up: self.0.given_up.load(Ordering::Relaxed),
            waits: self.0.waits.load(Ordering::Relaxed),
            waited_ms: self.0.waited_ms.load(Ordering::Relaxed),
        })
    }
}

/// Tells interfaces when the disk goes below the floor, and when it has room again.
pub async fn tell(mut changes: watch::Receiver<Option<String>>, pushes: broadcast::Sender<Push>) {
    while changes.changed().await.is_ok() {
        let notice = match changes.borrow_and_update().clone() {
            Some(why) => Notice { text: why, tone: Tone::Warning },
            None => Notice {
                text: "jaw's disk has room again, so history, downloads and other background work go on".into(),
                tone: Tone::Info,
            },
        };
        let params = serde_json::to_value(notice).expect("a notice serializes");
        let _ = pushes.send(Push { method: "notice".into(), params });
    }
}

/// The free space and size of the disk `folder` is on, if they can be read.
fn free_space(folder: &Path) -> Option<(u64, u64)> {
    let name = CString::new(folder.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `name` is a NUL-terminated path and `stats` is a statvfs to fill in.
    (unsafe { libc::statvfs(name.as_ptr(), &mut stats) } == 0).then(|| {
        let block = stats.f_frsize as u64;
        (stats.f_bavail as u64 * block, stats.f_blocks as u64 * block)
    })
}

/// Makes `path` a file of `bytes` that holds its room on the disk: reserved where the file system
/// can, written as zeros where it can't.
fn make(path: &Path, bytes: u64) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    if reserve(&file, bytes)? {
        return Ok(());
    }
    let zeros = vec![0u8; 1 << 20];
    let mut left = bytes;
    while left > 0 {
        let n = left.min(zeros.len() as u64) as usize;
        file.write_all(&zeros[..n])?;
        left -= n as u64;
    }
    Ok(())
}

/// Reserves `bytes` for `file` and makes it that long, saying whether the file system could.
#[cfg(target_os = "macos")]
fn reserve(file: &File, bytes: u64) -> std::io::Result<bool> {
    let mut store = libc::fstore_t {
        fst_flags: libc::F_ALLOCATEALL,
        fst_posmode: libc::F_PEOFPOSMODE,
        fst_offset: 0,
        fst_length: bytes as libc::off_t,
        fst_bytesalloc: 0,
    };
    // SAFETY: the descriptor is the open file's, and `store` is an fstore_t it fills in.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_PREALLOCATE, &mut store) } != 0 {
        let e = std::io::Error::last_os_error();
        return if e.raw_os_error() == Some(libc::ENOTSUP) { Ok(false) } else { Err(e) };
    }
    file.set_len(bytes)?;
    Ok(true)
}

/// Reserves `bytes` for `file` and makes it that long, saying whether the file system could.
#[cfg(target_os = "linux")]
fn reserve(file: &File, bytes: u64) -> std::io::Result<bool> {
    // SAFETY: the descriptor is the open file's.
    match unsafe { libc::posix_fallocate(file.as_raw_fd(), 0, bytes as libc::off_t) } {
        0 => Ok(true),
        libc::EOPNOTSUPP => Ok(false),
        e => Err(std::io::Error::from_raw_os_error(e)),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn reserve(_: &File, _: u64) -> std::io::Result<bool> {
    Ok(false)
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

fn mb(bytes: u64) -> String {
    format!("{} MB", bytes / 1_000_000)
}

#[cfg(test)]
mod tests;
