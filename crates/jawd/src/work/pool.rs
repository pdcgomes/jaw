//! A pool of workers for one resource, such as downloads or video transcodes: a queue for
//! interactive jobs ahead of one for background jobs, one run for each key however many ask for
//! it, and a background queue that makes producers wait when it's full.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{Notify, watch};

use super::Report;
use super::samples::Samples;
use crate::engine::BoxFuture;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Priority {
    /// What an interface shows or asked for right now.
    Interactive,
    /// Everything else: it waits for interactive work, and for room in its queue.
    Background,
}

/// A background job refused because its queue is full.
#[derive(Debug, PartialEq, Eq)]
pub struct Full;

/// A job's result, once it has run. Every handle for the same key gets the same result.
pub struct Handle<T> {
    done: watch::Receiver<Option<T>>,
}

impl<T: Clone> Handle<T> {
    /// The result, or `None` if the job was cancelled before it ran.
    pub async fn result(mut self) -> Option<T> {
        self.done.wait_for(Option::is_some).await.ok().and_then(|done| done.clone())
    }
}

struct Job<T> {
    key: String,
    interactive: AtomicBool,
    taken: AtomicBool,
    future: Mutex<Option<BoxFuture<'static, T>>>,
    done: Mutex<Option<watch::Sender<Option<T>>>>,
    /// When it joined the queue it's in: a promoted job's interactive wait starts at its promotion.
    queued: Mutex<Instant>,
}

impl<T> Job<T> {
    fn handle(&self) -> Option<Handle<T>> {
        self.done.lock().expect("a job").as_ref().map(|done| Handle { done: done.subscribe() })
    }
}

struct Queue<T> {
    interactive: VecDeque<Arc<Job<T>>>,
    background: VecDeque<Arc<Job<T>>>,
    by_key: HashMap<String, Arc<Job<T>>>,
    queued_interactive: usize,
    queued_background: usize,
    running: usize,
    done: u64,
    wait_interactive: Samples,
    wait_background: Samples,
    run: Samples,
    last_warning: Option<Instant>,
}

pub struct Pool<T> {
    name: &'static str,
    workers: usize,
    background_limit: usize,
    /// How long an interactive job may wait before the pool says it's over budget.
    budget: Duration,
    queue: Mutex<Queue<T>>,
    ready: Arc<Notify>,
    room: Notify,
}

impl<T> Drop for Pool<T> {
    /// Workers waiting for a job wake, find the pool gone, and end.
    fn drop(&mut self) {
        self.ready.notify_waiters();
    }
}

impl<T: Clone + Send + Sync + 'static> Pool<T> {
    /// A pool of `workers` running on the current tokio runtime, holding at most `background_limit`
    /// queued background jobs.
    pub fn new(name: &'static str, workers: usize, background_limit: usize) -> Arc<Pool<T>> {
        let pool = Arc::new(Pool {
            name,
            workers: workers.max(1),
            background_limit: background_limit.max(1),
            budget: Duration::from_millis(50),
            queue: Mutex::new(Queue {
                interactive: VecDeque::new(),
                background: VecDeque::new(),
                by_key: HashMap::new(),
                queued_interactive: 0,
                queued_background: 0,
                running: 0,
                done: 0,
                wait_interactive: Samples::default(),
                wait_background: Samples::default(),
                run: Samples::default(),
                last_warning: None,
            }),
            ready: Arc::new(Notify::new()),
            room: Notify::new(),
        });
        for _ in 0..pool.workers {
            tokio::spawn(work(Arc::downgrade(&pool), pool.ready.clone()));
        }
        pool
    }

    /// Queues a job, or joins the one already queued or running for `key`, promoting it if this
    /// asks sooner. A background job waits for room in its queue.
    pub async fn submit<F>(&self, key: impl Into<String>, priority: Priority, job: F) -> Handle<T>
    where
        F: Future<Output = T> + Send + 'static,
    {
        let key = key.into();
        let mut job: Option<BoxFuture<'static, T>> = Some(Box::pin(job));
        loop {
            let room = self.room.notified();
            match self.enqueue(&key, priority, &mut job, priority == Priority::Background) {
                Ok(handle) => return handle,
                Err(Full) => room.await,
            }
        }
    }

    /// As `submit`, but a background job is refused at once when its queue is full.
    pub fn try_submit<F>(&self, key: impl Into<String>, priority: Priority, job: F) -> Result<Handle<T>, Full>
    where
        F: Future<Output = T> + Send + 'static,
    {
        let mut job: Option<BoxFuture<'static, T>> = Some(Box::pin(job));
        self.enqueue(&key.into(), priority, &mut job, priority == Priority::Background)
    }

    fn enqueue(
        &self,
        key: &str,
        priority: Priority,
        job: &mut Option<BoxFuture<'static, T>>,
        bounded: bool,
    ) -> Result<Handle<T>, Full> {
        let mut queue = self.queue.lock().expect("a pool's queue");
        if let Some(existing) = queue.by_key.get(key).cloned()
            && let Some(handle) = existing.handle()
        {
            if priority == Priority::Interactive {
                promote(&mut queue, &existing);
                drop(queue);
                self.ready.notify_one();
            }
            return Ok(handle);
        }
        if bounded && queue.queued_background >= self.background_limit {
            return Err(Full);
        }
        let (done, handle) = watch::channel(None);
        let entry = Arc::new(Job {
            key: key.to_string(),
            interactive: AtomicBool::new(priority == Priority::Interactive),
            taken: AtomicBool::new(false),
            future: Mutex::new(job.take()),
            done: Mutex::new(Some(done)),
            queued: Mutex::new(Instant::now()),
        });
        queue.by_key.insert(key.to_string(), entry.clone());
        match priority {
            Priority::Interactive => {
                queue.queued_interactive += 1;
                queue.interactive.push_back(entry);
            }
            Priority::Background => {
                queue.queued_background += 1;
                queue.background.push_back(entry);
            }
        }
        drop(queue);
        self.ready.notify_one();
        Ok(Handle { done: handle })
    }

    /// Moves a queued background job ahead of every other background job, as when a window comes
    /// to show its message. False if there's no such job waiting.
    pub fn promote(&self, key: &str) -> bool {
        let mut queue = self.queue.lock().expect("a pool's queue");
        let Some(job) = queue.by_key.get(key).cloned() else { return false };
        let promoted = promote(&mut queue, &job);
        drop(queue);
        if promoted {
            self.ready.notify_one();
        }
        promoted
    }

    /// Takes a job off the queue before it runs; its handles get `None`. False if it's running or gone.
    pub fn cancel(&self, key: &str) -> bool {
        let mut queue = self.queue.lock().expect("a pool's queue");
        let Some(job) = queue.by_key.get(key).cloned() else { return false };
        if job.taken.swap(true, Ordering::AcqRel) {
            return false;
        }
        if job.interactive.load(Ordering::Acquire) {
            queue.queued_interactive -= 1;
        } else {
            queue.queued_background -= 1;
        }
        queue.by_key.remove(key);
        drop(queue);
        job.future.lock().expect("a job").take();
        job.done.lock().expect("a job").take();
        self.room.notify_one();
        true
    }

    /// The next job, interactive first; jobs already taken are passed over.
    fn next(&self) -> Option<Arc<Job<T>>> {
        let mut queue = self.queue.lock().expect("a pool's queue");
        let queue = &mut *queue;
        for interactive in [true, false] {
            let lane = if interactive { &mut queue.interactive } else { &mut queue.background };
            while let Some(job) = lane.pop_front() {
                if job.taken.swap(true, Ordering::AcqRel) {
                    continue;
                }
                let waited = job.queued.lock().expect("a job").elapsed();
                if job.interactive.load(Ordering::Acquire) {
                    queue.queued_interactive -= 1;
                    queue.wait_interactive.add(waited);
                    if waited > self.budget
                        && queue.last_warning.is_none_or(|at| at.elapsed() > Duration::from_secs(10))
                    {
                        queue.last_warning = Some(Instant::now());
                        eprintln!(
                            "jawd: {} waited {} ms for the {} pool, over its {} ms",
                            job.key,
                            waited.as_millis(),
                            self.name,
                            self.budget.as_millis()
                        );
                    }
                } else {
                    queue.queued_background -= 1;
                    queue.wait_background.add(waited);
                    self.room.notify_one();
                }
                queue.running += 1;
                return Some(job);
            }
        }
        None
    }

    fn finished(&self, job: &Arc<Job<T>>, took: Duration) {
        let mut queue = self.queue.lock().expect("a pool's queue");
        queue.running -= 1;
        queue.done += 1;
        queue.run.add(took);
        if queue.by_key.get(&job.key).is_some_and(|current| Arc::ptr_eq(current, job)) {
            queue.by_key.remove(&job.key);
        }
    }
}

/// Moves `job` to the interactive queue if it's a background job still waiting.
fn promote<T>(queue: &mut Queue<T>, job: &Arc<Job<T>>) -> bool {
    if job.taken.load(Ordering::Acquire) || job.interactive.swap(true, Ordering::AcqRel) {
        return false;
    }
    queue.queued_background -= 1;
    queue.queued_interactive += 1;
    *job.queued.lock().expect("a job") = Instant::now();
    queue.interactive.push_back(job.clone());
    true
}

/// One worker: runs jobs until the pool is gone. It holds the pool only while it takes a job or
/// says one is done, so dropping the pool ends its workers.
async fn work<T: Clone + Send + Sync + 'static>(pool: std::sync::Weak<Pool<T>>, ready: Arc<Notify>) {
    loop {
        let notified = ready.notified();
        let Some(strong) = pool.upgrade() else { return };
        let job = strong.next();
        drop(strong);
        let Some(job) = job else {
            notified.await;
            continue;
        };
        let future = job.future.lock().expect("a job").take();
        let started = Instant::now();
        let output = match future {
            Some(future) => Some(future.await),
            None => None,
        };
        if let Some(output) = output
            && let Some(done) = job.done.lock().expect("a job").take()
        {
            done.send_replace(Some(output));
        }
        if let Some(strong) = pool.upgrade() {
            strong.finished(&job, started.elapsed());
        }
    }
}

impl<T: Clone + Send + Sync + 'static> Report for Pool<T> {
    fn report(&self) -> jaw_proto::Pool {
        let queue = self.queue.lock().expect("a pool's queue");
        jaw_proto::Pool {
            name: self.name.into(),
            workers: self.workers as u32,
            running: queue.running as u32,
            queued_interactive: queue.queued_interactive as u32,
            queued_background: queue.queued_background as u32,
            done: queue.done,
            wait_interactive: queue.wait_interactive.percentiles(),
            wait_background: queue.wait_background.percentiles(),
            run: queue.run.percentiles(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use tokio::sync::oneshot;

    use super::*;

    /// A job that records its key when it starts, and waits for `gate` if given.
    fn job(
        order: &Arc<StdMutex<Vec<String>>>,
        key: &str,
        gate: Option<oneshot::Receiver<()>>,
    ) -> impl Future<Output = String> + Send + 'static {
        let order = order.clone();
        let key = key.to_string();
        async move {
            order.lock().unwrap().push(key.clone());
            if let Some(gate) = gate {
                let _ = gate.await;
            }
            key
        }
    }

    #[tokio::test]
    async fn interactive_jobs_go_before_background_ones_and_promoted_ones_with_them() {
        let pool = Pool::new("test", 1, 100);
        let order = Arc::new(StdMutex::new(Vec::new()));
        let (open, gate) = oneshot::channel();
        let first = pool.submit("first", Priority::Background, job(&order, "first", Some(gate))).await;
        while order.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
        let history = pool.submit("history", Priority::Background, job(&order, "history", None)).await;
        let avatar = pool.submit("avatar", Priority::Background, job(&order, "avatar", None)).await;
        let photo = pool.submit("photo", Priority::Interactive, job(&order, "photo", None)).await;
        assert!(pool.promote("avatar"));
        assert!(!pool.promote("first"), "a running job can't be promoted");
        open.send(()).unwrap();
        for handle in [first, history, avatar, photo] {
            handle.result().await.unwrap();
        }
        assert_eq!(*order.lock().unwrap(), ["first", "photo", "avatar", "history"]);
        let report = pool.report();
        assert_eq!((report.done, report.running, report.queued_background), (4, 0, 0));
        assert!(report.wait_interactive.is_some() && report.wait_background.is_some() && report.run.is_some());
    }

    #[tokio::test]
    async fn the_same_key_runs_once_for_everyone_who_asks() {
        let pool = Pool::new("test", 2, 100);
        let order = Arc::new(StdMutex::new(Vec::new()));
        let (open, gate) = oneshot::channel();
        let a = pool.submit("same", Priority::Background, job(&order, "same", Some(gate))).await;
        let b = pool.submit("same", Priority::Interactive, job(&order, "again", None)).await;
        open.send(()).unwrap();
        assert_eq!(a.result().await.as_deref(), Some("same"));
        assert_eq!(b.result().await.as_deref(), Some("same"));
        assert_eq!(*order.lock().unwrap(), ["same"]);
        let after = pool.submit("same", Priority::Background, job(&order, "later", None)).await;
        assert_eq!(after.result().await.as_deref(), Some("later"), "a finished key runs again");
    }

    #[tokio::test]
    async fn a_full_background_queue_makes_producers_wait_but_never_interactive_work() {
        let pool = Pool::new("test", 1, 2);
        let order = Arc::new(StdMutex::new(Vec::new()));
        let (open, gate) = oneshot::channel();
        let running = pool.submit("running", Priority::Background, job(&order, "running", Some(gate))).await;
        while order.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
        let _one = pool.submit("one", Priority::Background, job(&order, "one", None)).await;
        let _two = pool.submit("two", Priority::Background, job(&order, "two", None)).await;
        assert_eq!(pool.try_submit("three", Priority::Background, job(&order, "three", None)).err(), Some(Full));
        let interactive = pool.try_submit("now", Priority::Interactive, job(&order, "now", None));
        assert!(interactive.is_ok(), "interactive work never waits for room");
        let waiting = {
            let pool = pool.clone();
            let order = order.clone();
            tokio::spawn(async move {
                pool.submit("three", Priority::Background, job(&order, "three", None)).await.result().await
            })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!waiting.is_finished(), "a background producer waits while the queue is full");
        open.send(()).unwrap();
        running.result().await.unwrap();
        assert_eq!(waiting.await.unwrap().as_deref(), Some("three"));
    }

    #[tokio::test]
    async fn a_cancelled_job_never_runs() {
        let pool = Pool::new("test", 1, 10);
        let order = Arc::new(StdMutex::new(Vec::new()));
        let (open, gate) = oneshot::channel();
        let _running = pool.submit("running", Priority::Background, job(&order, "running", Some(gate))).await;
        while order.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
        let video = pool.submit("video", Priority::Background, job(&order, "video", None)).await;
        assert!(pool.cancel("video"));
        assert!(!pool.cancel("video"));
        assert!(!pool.cancel("running"), "a running job can't be cancelled");
        assert_eq!(video.result().await, None);
        open.send(()).unwrap();
        let last = pool.submit("last", Priority::Background, job(&order, "last", None)).await;
        last.result().await.unwrap();
        assert_eq!(*order.lock().unwrap(), ["running", "last"]);
    }
}
