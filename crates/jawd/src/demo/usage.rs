//! A thread that looks at the process's memory and the WAL's size on disk ten times a second while
//! it's on, keeping the most it has seen; off, it waits without waking, so it can't count against
//! an idle jawd.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use jaw_client::usage::{now, thread_cpu};

/// How often the sampler looks while it's on.
const EVERY: Duration = Duration::from_millis(100);

/// The most the sampler has seen, in bytes.
#[derive(Clone, Copy, Debug, Default)]
pub struct Most {
    pub footprint: u64,
    pub resident: u64,
    pub wal: u64,
}

impl Most {
    fn with(self, wal: &Path) -> Most {
        let usage = now();
        let size = std::fs::metadata(wal).map_or(0, |m| m.len());
        Most {
            footprint: self.footprint.max(usage.footprint),
            resident: self.resident.max(usage.resident),
            wal: self.wal.max(size),
        }
    }
}

pub struct Sampler {
    on: mpsc::Sender<bool>,
    most: Arc<Mutex<Most>>,
    wal: PathBuf,
}

impl Sampler {
    /// Starts sampling this process and the WAL at `wal`.
    pub fn start(wal: PathBuf) -> Sampler {
        let (on, turned) = mpsc::channel();
        let most: Arc<Mutex<Most>> = Arc::default();
        let (seen, watched) = (most.clone(), wal.clone());
        std::thread::Builder::new()
            .name("jawd-demo-sampler".into())
            .spawn(move || {
                let mut sampling = true;
                loop {
                    let next = if sampling {
                        turned.recv_timeout(EVERY)
                    } else {
                        turned.recv().map_err(|_| RecvTimeoutError::Disconnected)
                    };
                    match next {
                        Ok(now_on) => sampling = now_on,
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                    if sampling {
                        let mut most = seen.lock().expect("the sampler's most");
                        *most = most.with(&watched);
                    }
                }
            })
            .expect("the sampler's thread");
        Sampler { on, most, wal }
    }

    pub fn sampling(&self, on: bool) {
        let _ = self.on.send(on);
    }

    /// The most seen since it was last asked, now included; it counts again from now.
    pub fn most(&self) -> Most {
        let mut most = self.most.lock().expect("the sampler's most");
        let seen = most.with(&self.wal);
        *most = Most::default().with(&self.wal);
        seen
    }
}
