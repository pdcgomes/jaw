//! Keeps an engine running: starts it, starts it again after a pause that doubles each time it
//! stops (back to the first once it has run a minute), and says how it is for `status`. Told to
//! stop, it does at once, whatever it's waiting for, and starts the engine no more.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, watch};

use super::{Commands, Engine, EngineError, Incoming, pb};

#[derive(Clone, Debug, PartialEq)]
pub struct EngineStatus {
    pub kind: &'static str,
    pub state: State,
    /// What the engine said it is in its hello.
    pub version: Option<String>,
    pub restarts: u32,
    /// Why it last stopped.
    pub last_exit: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Starting,
    Running,
    Restarting,
    Stopped,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Starting => "starting",
            State::Running => "running",
            State::Restarting => "restarting",
            State::Stopped => "stopped",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Backoff {
    pub first: Duration,
    pub most: Duration,
    /// How long an engine must run for the pause to start again from `first`.
    pub settled: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Backoff { first: Duration::from_millis(500), most: Duration::from_secs(30), settled: Duration::from_secs(60) }
    }
}

#[derive(Clone)]
pub struct Supervisor {
    status: watch::Receiver<EngineStatus>,
    current: watch::Receiver<Option<Commands>>,
    stop: Arc<watch::Sender<bool>>,
}

impl Supervisor {
    /// Runs `engine` until jawd stops listening to `events`.
    pub fn spawn(engine: Arc<dyn Engine>, events: mpsc::Sender<Incoming>, backoff: Backoff) -> Supervisor {
        let initial =
            EngineStatus { kind: engine.kind(), state: State::Starting, version: None, restarts: 0, last_exit: None };
        let (status_tx, status) = watch::channel(initial);
        let (current_tx, current) = watch::channel(None);
        let (stop, stopping) = watch::channel(false);
        tokio::spawn(run(engine, events, backoff, status_tx, current_tx, stopping));
        Supervisor { status, current, stop: Arc::new(stop) }
    }

    /// Ends the engine and starts it no more, and waits until it has: a start under way is given
    /// up, a pause before the next is cut short, and an engine running is ended as jawd exiting
    /// would end it.
    pub async fn stop(&self) {
        self.stop.send_replace(true);
        let mut status = self.status.clone();
        let _ = status.wait_for(|status| status.state == State::Stopped).await;
    }

    pub fn status(&self) -> EngineStatus {
        self.status.borrow().clone()
    }

    /// Sends a command to the engine running now; if none is, says why at once.
    pub async fn call(&self, kind: pb::command::Kind) -> Result<pb::reply::Kind, EngineError> {
        let commands = self.current.borrow().clone();
        match commands {
            Some(commands) => commands.call(kind).await,
            None => {
                let status = self.status();
                Err(EngineError::Unavailable(
                    status.last_exit.unwrap_or_else(|| format!("it's {}", status.state.name())),
                ))
            }
        }
    }
}

async fn run(
    engine: Arc<dyn Engine>,
    events: mpsc::Sender<Incoming>,
    backoff: Backoff,
    status: watch::Sender<EngineStatus>,
    current: watch::Sender<Option<Commands>>,
    mut stopping: watch::Receiver<bool>,
) {
    supervise(engine, events, backoff, &status, &current, &mut stopping).await;
    current.send_replace(None);
    status.send_modify(|s| s.state = State::Stopped);
}

/// Waits until the supervisor is told to stop; with no one left to tell it, that's never.
async fn told(stopping: &mut watch::Receiver<bool>) {
    if stopping.wait_for(|stop| *stop).await.is_err() {
        std::future::pending::<()>().await;
    }
}

/// Runs the engine until jawd stops listening to `events` or says to stop.
async fn supervise(
    engine: Arc<dyn Engine>,
    events: mpsc::Sender<Incoming>,
    backoff: Backoff,
    status: &watch::Sender<EngineStatus>,
    current: &watch::Sender<Option<Commands>>,
    stopping: &mut watch::Receiver<bool>,
) {
    let mut pause = backoff.first;
    loop {
        status.send_modify(|s| s.state = State::Starting);
        let started = Instant::now();
        let instance = tokio::select! {
            instance = engine.start(events.clone()) => instance,
            _ = told(stopping) => return,
        };
        let why = match instance {
            Ok(instance) => {
                let hello = &instance.hello;
                let version = if hello.whatsmeow.is_empty() {
                    hello.program.clone()
                } else {
                    format!("{} (whatsmeow {})", hello.program, hello.whatsmeow)
                };
                status.send_modify(|s| {
                    s.state = State::Running;
                    s.version = Some(version);
                });
                current.send_replace(Some(instance.commands));
                let why = tokio::select! {
                    why = instance.exited => why,
                    _ = told(stopping) => return,
                };
                current.send_replace(None);
                why
            }
            Err(e) => format!("{e:#}"),
        };
        if events.is_closed() {
            return;
        }
        if started.elapsed() >= backoff.settled {
            pause = backoff.first;
        }
        eprintln!("jawd: the {} engine stopped ({why}); starting it again in {pause:?}", engine.kind());
        status.send_modify(|s| {
            s.state = State::Restarting;
            s.restarts += 1;
            s.last_exit = Some(why);
        });
        tokio::select! {
            _ = tokio::time::sleep(pause) => {}
            _ = told(stopping) => return,
        }
        pause = (pause * 2).min(backoff.most);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::engine::demo::DemoEngine;
    use crate::engine::{BoxFuture, Instance};

    /// An engine that fails to start `failures` times, then runs the demo for as long as asked.
    struct Flaky {
        failures: u32,
        tries: AtomicU32,
        demo: DemoEngine,
    }

    impl Engine for Flaky {
        fn kind(&self) -> &'static str {
            "flaky"
        }

        fn start(&self, events: mpsc::Sender<Incoming>) -> BoxFuture<'_, anyhow::Result<Instance>> {
            Box::pin(async move {
                if self.tries.fetch_add(1, Ordering::Relaxed) < self.failures {
                    anyhow::bail!("it didn't start");
                }
                self.demo.start(events).await
            })
        }
    }

    fn quick() -> Backoff {
        Backoff { first: Duration::from_millis(5), most: Duration::from_millis(20), settled: Duration::from_secs(60) }
    }

    #[tokio::test]
    async fn an_engine_that_fails_is_started_again_until_it_runs() {
        let engine = Arc::new(Flaky { failures: 3, tries: AtomicU32::new(0), demo: DemoEngine::with(Vec::new()) });
        let (tx, _rx) = mpsc::channel(8);
        let supervisor = Supervisor::spawn(engine, tx, quick());
        let mut status = supervisor.status.clone();
        let running =
            tokio::time::timeout(Duration::from_secs(2), status.wait_for(|s| s.state == State::Running)).await;
        let running = running.expect("the engine never ran").unwrap().clone();
        assert_eq!(running.restarts, 3);
        assert_eq!(running.last_exit.as_deref(), Some("it didn't start"));
        assert_eq!(running.version.as_deref(), Some("jawd's demo engine"));
        let pong = supervisor.call(pb::command::Kind::Ping(pb::Ping {})).await.unwrap();
        assert!(matches!(pong, pb::reply::Kind::Pong(_)));
    }

    /// An engine whose start never finishes, as one that never says hello.
    struct Silent;

    impl Engine for Silent {
        fn kind(&self) -> &'static str {
            "silent"
        }

        fn start(&self, _events: mpsc::Sender<Incoming>) -> BoxFuture<'_, anyhow::Result<Instance>> {
            Box::pin(std::future::pending())
        }
    }

    /// Stops `supervisor`, which must take no longer than a moment.
    async fn stopped(supervisor: &Supervisor) {
        tokio::time::timeout(Duration::from_millis(200), supervisor.stop()).await.expect("it didn't stop at once");
        assert_eq!(supervisor.status().state, State::Stopped);
    }

    #[tokio::test]
    async fn told_to_stop_while_it_waits_to_start_an_engine_again_it_stops_at_once_and_starts_it_no_more() {
        let engine =
            Arc::new(Flaky { failures: u32::MAX, tries: AtomicU32::new(0), demo: DemoEngine::with(Vec::new()) });
        let (tx, _rx) = mpsc::channel(8);
        let pausing = Backoff { first: Duration::from_secs(30), ..quick() };
        let supervisor = Supervisor::spawn(engine.clone(), tx, pausing);
        let mut status = supervisor.status.clone();
        status.wait_for(|s| s.state == State::Restarting).await.unwrap();
        stopped(&supervisor).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(engine.tries.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn told_to_stop_while_a_start_waits_for_its_hello_it_gives_the_start_up() {
        let (tx, _rx) = mpsc::channel(8);
        let supervisor = Supervisor::spawn(Arc::new(Silent), tx, quick());
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(supervisor.status().state, State::Starting);
        stopped(&supervisor).await;
    }

    #[tokio::test]
    async fn told_to_stop_while_its_engine_runs_it_ends_it_and_starts_it_no_more() {
        let engine = Arc::new(Flaky { failures: 0, tries: AtomicU32::new(0), demo: DemoEngine::with(Vec::new()) });
        let (tx, _rx) = mpsc::channel(8);
        let supervisor = Supervisor::spawn(engine.clone(), tx, quick());
        let mut status = supervisor.status.clone();
        status.wait_for(|s| s.state == State::Running).await.unwrap();
        stopped(&supervisor).await;
        let refused = supervisor.call(pb::command::Kind::Ping(pb::Ping {})).await.unwrap_err();
        assert!(matches!(refused, EngineError::Unavailable(_)), "{refused:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!((engine.tries.load(Ordering::Relaxed), supervisor.status().restarts), (1, 0));
    }

    #[tokio::test]
    async fn commands_while_no_engine_runs_say_why() {
        let engine =
            Arc::new(Flaky { failures: u32::MAX, tries: AtomicU32::new(0), demo: DemoEngine::with(Vec::new()) });
        let (tx, _rx) = mpsc::channel(8);
        let supervisor = Supervisor::spawn(engine, tx, quick());
        let mut status = supervisor.status.clone();
        status.wait_for(|s| s.restarts >= 1).await.unwrap();
        let refused = supervisor.call(pb::command::Kind::Ping(pb::Ping {})).await.unwrap_err();
        assert_eq!(refused, EngineError::Unavailable("it didn't start".into()));
    }
}
