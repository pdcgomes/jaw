//! A thread that looks at the process's memory ten times a second while it's on, keeping the most it
//! has seen; off, it waits without waking, so it can't count against an idle jawd.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use jaw_client::usage::{now, thread_cpu};

/// How often the sampler looks while it's on.
const EVERY: Duration = Duration::from_millis(100);

pub struct Sampler {
    on: mpsc::Sender<bool>,
    most: Arc<Mutex<(u64, u64)>>,
}

impl Sampler {
    pub fn start() -> Sampler {
        let (on, turned) = mpsc::channel();
        let most: Arc<Mutex<(u64, u64)>> = Arc::default();
        let seen = most.clone();
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
                        let usage = now();
                        let mut most = seen.lock().expect("the sampler's most");
                        *most = (most.0.max(usage.footprint), most.1.max(usage.resident));
                    }
                }
            })
            .expect("the sampler's thread");
        Sampler { on, most }
    }

    pub fn sampling(&self, on: bool) {
        let _ = self.on.send(on);
    }

    /// The most footprint and resident memory seen since it was last asked, now included; it
    /// counts again from now.
    pub fn most(&self) -> (u64, u64) {
        let usage = now();
        let mut most = self.most.lock().expect("the sampler's most");
        let seen = (most.0.max(usage.footprint), most.1.max(usage.resident));
        *most = (usage.footprint, usage.resident);
        seen
    }
}
