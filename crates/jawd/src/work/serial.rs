//! Jobs in order for each key, many keys at once: sends go one after another in each chat, so
//! messages arrive as they were written, while up to `limit` chats send at the same time. A job can
//! ask to run again once something has happened, a pause or a connection coming back; it waits for
//! that without one of the limit's places, so other keys go meanwhile, and still ahead of its key's
//! later jobs.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::Semaphore;

use super::Report;
use super::samples::Samples;
use crate::engine::BoxFuture;

/// What a job wants once it has run.
pub enum Next {
    Done,
    /// To run again once this has happened.
    After(BoxFuture<'static, ()>),
}

type Job = Box<dyn FnMut() -> BoxFuture<'static, Next> + Send>;

pub struct Serial {
    name: &'static str,
    limit: usize,
    permits: Arc<Semaphore>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// Each key's jobs; the front one is taken out while it runs.
    queues: HashMap<String, VecDeque<(Instant, Option<Job>)>>,
    queued: usize,
    running: usize,
    waiting: usize,
    done: u64,
    wait: Samples,
    run: Samples,
}

impl Serial {
    pub fn new(name: &'static str, limit: usize) -> Arc<Serial> {
        let limit = limit.max(1);
        Arc::new(Serial { name, limit, permits: Arc::new(Semaphore::new(limit)), state: Mutex::default() })
    }

    /// Runs `job` after every job already queued for `key`.
    pub fn push<F>(self: &Arc<Self>, key: impl Into<String>, job: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut job = Some(job);
        self.push_again(key, move || {
            let job = job.take();
            async move {
                if let Some(job) = job {
                    job.await;
                }
                Next::Done
            }
        });
    }

    /// Runs `job` after every job already queued for `key`, and again each time it asks to.
    pub fn push_again<F, R>(self: &Arc<Self>, key: impl Into<String>, mut job: F)
    where
        F: FnMut() -> R + Send + 'static,
        R: Future<Output = Next> + Send + 'static,
    {
        let key = key.into();
        let job: Job = Box::new(move || Box::pin(job()));
        let mut state = self.state.lock().expect("a serial queue");
        state.queued += 1;
        let queue = state.queues.entry(key.clone()).or_default();
        let first = queue.is_empty();
        queue.push_back((Instant::now(), Some(job)));
        drop(state);
        if first {
            tokio::spawn(self.clone().drain(key));
        }
    }

    /// Runs `key`'s jobs one at a time until its queue is empty, taking a permit for each run, so a
    /// chat with many to send doesn't keep the others waiting, and giving it up while a job waits.
    async fn drain(self: Arc<Self>, key: String) {
        let mut again = false;
        loop {
            let Ok(permit) = self.permits.clone().acquire_owned().await else { return };
            let next = {
                let mut state = self.state.lock().expect("a serial queue");
                let front = state.queues.get_mut(&key).and_then(|queue| queue.front_mut());
                let taken = front.and_then(|(queued, job)| Some((*queued, job.take()?)));
                if let Some((queued, _)) = &taken {
                    if again {
                        state.waiting -= 1;
                    } else {
                        state.queued -= 1;
                        state.wait.add(queued.elapsed());
                    }
                    state.running += 1;
                }
                taken
            };
            let Some((_, mut job)) = next else { return };
            let started = Instant::now();
            let next = job().await;
            drop(permit);
            let wait = {
                let mut state = self.state.lock().expect("a serial queue");
                state.running -= 1;
                state.run.add(started.elapsed());
                let queue = state.queues.get_mut(&key).expect("a key being drained");
                match next {
                    Next::After(wait) => {
                        if let Some((_, slot)) = queue.front_mut() {
                            *slot = Some(job);
                        }
                        state.waiting += 1;
                        wait
                    }
                    Next::Done => {
                        queue.pop_front();
                        let empty = queue.is_empty();
                        state.done += 1;
                        if empty {
                            state.queues.remove(&key);
                            return;
                        }
                        again = false;
                        continue;
                    }
                }
            };
            wait.await;
            again = true;
        }
    }

    #[cfg(test)]
    fn idle(&self) -> bool {
        self.state.lock().unwrap().queues.is_empty()
    }
}

impl Report for Serial {
    fn report(&self) -> jaw_proto::Pool {
        let state = self.state.lock().expect("a serial queue");
        jaw_proto::Pool {
            name: self.name.into(),
            workers: self.limit as u32,
            running: state.running as u32,
            queued_interactive: state.queued as u32,
            queued_background: state.waiting as u32,
            done: state.done,
            wait_interactive: state.wait.percentiles(),
            wait_background: None,
            run: state.run.percentiles(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;

    /// Waits until `serial` has nothing left.
    async fn settle(serial: &Serial) {
        while !serial.idle() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn each_key_keeps_its_order_and_keys_run_side_by_side_up_to_the_limit() {
        let serial = Serial::new("sends", 3);
        let log = Arc::new(Mutex::new(Vec::new()));
        let now = Arc::new(AtomicUsize::new(0));
        let most = Arc::new(AtomicUsize::new(0));
        for chat in 0..6 {
            for n in 0..5 {
                let (log, now, most) = (log.clone(), now.clone(), most.clone());
                serial.push(format!("chat{chat}"), async move {
                    let running = now.fetch_add(1, Ordering::SeqCst) + 1;
                    most.fetch_max(running, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(2)).await;
                    log.lock().unwrap().push((chat, n));
                    now.fetch_sub(1, Ordering::SeqCst);
                });
            }
        }
        settle(&serial).await;
        let log = log.lock().unwrap();
        assert_eq!(log.len(), 30);
        for chat in 0..6 {
            let order: Vec<_> = log.iter().filter(|(c, _)| *c == chat).map(|(_, n)| *n).collect();
            assert_eq!(order, [0, 1, 2, 3, 4], "chat {chat} sent out of order");
        }
        assert_eq!(most.load(Ordering::SeqCst), 3, "more chats ran at once than the limit, or fewer");
        let report = serial.report();
        assert_eq!((report.done, report.running, report.queued_interactive), (30, 0, 0));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_job_waiting_to_run_again_gives_up_its_place_but_not_its_keys_turn() {
        let serial = Serial::new("sends", 1);
        let log = Arc::new(Mutex::new(Vec::new()));
        let (open, opened) = tokio::sync::watch::channel(false);
        let tries = Arc::new(AtomicUsize::new(0));
        {
            let (log, tries) = (log.clone(), tries.clone());
            serial.push_again("stuck", move || {
                let (log, tries, mut opened) = (log.clone(), tries.clone(), opened.clone());
                async move {
                    if tries.fetch_add(1, Ordering::SeqCst) == 0 {
                        return Next::After(Box::pin(async move {
                            let _ = opened.wait_for(|open| *open).await;
                        }));
                    }
                    log.lock().unwrap().push("stuck, at last");
                    Next::Done
                }
            });
        }
        let after = log.clone();
        serial.push("stuck", async move { after.lock().unwrap().push("stuck's next") });
        let other = log.clone();
        serial.push("other", async move { other.lock().unwrap().push("other") });
        while log.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        assert_eq!(*log.lock().unwrap(), ["other"], "the only place went to another key while stuck waited");
        assert_eq!(serial.report().queued_background, 1);
        open.send_replace(true);
        settle(&serial).await;
        assert_eq!(*log.lock().unwrap(), ["other", "stuck, at last", "stuck's next"]);
        assert_eq!(tries.load(Ordering::SeqCst), 2);
    }
}
