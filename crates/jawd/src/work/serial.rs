//! Jobs in order for each key, many keys at once: sends go one after another in each chat, so
//! messages arrive as they were written, while up to `limit` chats send at the same time.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::Semaphore;

use super::Report;
use super::samples::Samples;
use crate::engine::BoxFuture;

pub struct Serial {
    name: &'static str,
    limit: usize,
    permits: Arc<Semaphore>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    queues: HashMap<String, VecDeque<(Instant, BoxFuture<'static, ()>)>>,
    queued: usize,
    running: usize,
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
        let key = key.into();
        let mut state = self.state.lock().expect("a serial queue");
        state.queued += 1;
        let queue = state.queues.entry(key.clone()).or_default();
        let first = queue.is_empty();
        queue.push_back((Instant::now(), Box::pin(job)));
        drop(state);
        if first {
            tokio::spawn(self.clone().drain(key));
        }
    }

    /// Runs `key`'s jobs one at a time until its queue is empty, taking a permit for each, so a chat
    /// with many to send doesn't keep the others waiting.
    async fn drain(self: Arc<Self>, key: String) {
        loop {
            let Ok(permit) = self.permits.clone().acquire_owned().await else { return };
            let next = {
                let mut state = self.state.lock().expect("a serial queue");
                let front = state
                    .queues
                    .get_mut(&key)
                    .and_then(|queue| queue.front_mut())
                    .map(|(queued, job)| (*queued, std::mem::replace(job, Box::pin(async {}))));
                if let Some((queued, _)) = &front {
                    state.queued -= 1;
                    state.running += 1;
                    state.wait.add(queued.elapsed());
                }
                front
            };
            let Some((_, job)) = next else { return };
            let started = Instant::now();
            job.await;
            drop(permit);
            let mut state = self.state.lock().expect("a serial queue");
            state.running -= 1;
            state.done += 1;
            state.run.add(started.elapsed());
            let queue = state.queues.get_mut(&key).expect("a key being drained");
            queue.pop_front();
            if queue.is_empty() {
                state.queues.remove(&key);
                return;
            }
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
            queued_background: 0,
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
}
