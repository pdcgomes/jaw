use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::{Error, Result};

/// Cancels a job from any thread: the job stops at its next step, and an ffmpeg it's running is
/// killed at once.
#[derive(Clone, Default)]
pub struct Cancel(Arc<State>);

#[derive(Default)]
struct State {
    cancelled: AtomicBool,
    /// The ffmpeg the job is running, until it has finished and before it's reaped, so the pid
    /// can't belong to another process by the time it's killed.
    child: Mutex<Option<libc::pid_t>>,
}

impl Cancel {
    pub fn new() -> Cancel {
        Cancel::default()
    }

    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::SeqCst);
        let child = self.0.child.lock().expect("cancel");
        if let Some(pid) = *child {
            kill(pid);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::SeqCst)
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.is_cancelled() { Err(Error::Cancelled) } else { Ok(()) }
    }

    /// Holds `pid` until `release`, killing it now if the job was cancelled meanwhile.
    pub(crate) fn hold(&self, pid: libc::pid_t) {
        let mut child = self.0.child.lock().expect("cancel");
        *child = Some(pid);
        if self.is_cancelled() {
            kill(pid);
        }
    }

    /// Lets go of the child, before it's reaped.
    pub(crate) fn release(&self) {
        *self.0.child.lock().expect("cancel") = None;
    }
}

pub(crate) fn kill(pid: libc::pid_t) {
    // The pid is an unreaped child of this process, so it can't name anything else.
    unsafe { libc::kill(pid, libc::SIGKILL) };
}

/// How far a copy has got: `done` from 0 to 1 by the media's time, and the bytes written so far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Progress {
    /// Which of the copies asked for this is, from 0.
    pub copy: usize,
    pub done: f32,
    pub bytes: u64,
}

/// What a job reports to and is cancelled through.
pub struct Job<'a> {
    pub cancel: Cancel,
    progress: Box<dyn FnMut(Progress) + Send + 'a>,
}

impl<'a> Job<'a> {
    pub fn new(cancel: Cancel) -> Job<'a> {
        Job { cancel, progress: Box::new(|_| {}) }
    }

    /// Calls `progress` as the job goes: a few times a second for an ffmpeg job, once per step
    /// otherwise.
    pub fn on_progress(mut self, progress: impl FnMut(Progress) + Send + 'a) -> Job<'a> {
        self.progress = Box::new(progress);
        self
    }

    pub(crate) fn report(&mut self, progress: Progress) {
        (self.progress)(progress);
    }
}
