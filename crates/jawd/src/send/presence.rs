//! Presence: you're online while some interface has focus, and away otherwise, so the phone keeps
//! its notifications while you're not looking. Focus has to hold for `Timing::online_after` before
//! WhatsApp hears you're online, and be gone for `Timing::away_after` before it hears you're away,
//! so switching windows doesn't flicker. Each time the connection comes back, WhatsApp is told
//! again, since it forgets; and whatever waits to send hears whether WhatsApp can be reached.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use jaw_proto::{Connection, ConnectionState, Push};
use tokio::sync::{broadcast, watch};

use super::{Inner, engine};
use crate::engine::pb;

/// Which connections have focus, and whether any does.
pub(super) struct Presence {
    focused: std::sync::Mutex<HashMap<u64, bool>>,
    next: AtomicU64,
    any: watch::Sender<bool>,
}

impl Presence {
    pub(super) fn new() -> (Presence, watch::Receiver<bool>) {
        let (any, focused) = watch::channel(false);
        (Presence { focused: Default::default(), next: AtomicU64::new(1), any }, focused)
    }

    fn set(&self, id: u64, focused: Option<bool>) {
        let mut all = self.focused.lock().expect("focus");
        match focused {
            Some(focused) => all.insert(id, focused),
            None => all.remove(&id),
        };
        let any = all.values().any(|&focused| focused);
        self.any.send_if_modified(|was| std::mem::replace(was, any) != any);
    }
}

/// One connection's focus; it counts for nothing once it's dropped.
pub struct Viewer {
    inner: Arc<Inner>,
    id: u64,
}

impl Viewer {
    pub fn focus(&self, focused: bool) {
        self.inner.presence.set(self.id, Some(focused));
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        self.inner.presence.set(self.id, None);
    }
}

pub(super) fn viewer(inner: &Arc<Inner>, focused: bool) -> Viewer {
    let id = inner.presence.next.fetch_add(1, Ordering::Relaxed);
    let viewer = Viewer { inner: inner.clone(), id };
    viewer.focus(focused);
    viewer
}

/// Whether WhatsApp can hear from jawd now.
pub(super) fn connected(connection: &Connection) -> bool {
    matches!(connection.state, ConnectionState::Online | ConnectionState::CatchingUp)
}

/// Tells WhatsApp you're online or away as focus comes and goes, once each has held long enough,
/// and again each time the connection comes back, waking whatever waits for it.
pub(super) async fn follow(
    inner: Arc<Inner>,
    mut pushes: broadcast::Receiver<Push>,
    mut focused: watch::Receiver<bool>,
    mut online: bool,
) {
    let mut told: Option<bool> = None;
    loop {
        let want = *focused.borrow_and_update();
        let wait = match told {
            _ if !online => None,
            Some(told) if told == want => None,
            Some(true) => Some(inner.timing.away_after),
            _ if want => Some(inner.timing.online_after),
            _ => Some(Duration::ZERO),
        };
        tokio::select! {
            _ = tokio::time::sleep(wait.unwrap_or_default()), if wait.is_some() => {
                match engine(&inner, pb::command::Kind::Presence(pb::Presence { available: want })).await {
                    Ok(_) => told = Some(want),
                    // It's said again when the connection next comes back.
                    Err(e) => {
                        eprintln!("jawd: can't tell WhatsApp you're {}: {e}", if want { "online" } else { "away" });
                        online = false;
                    }
                }
            }
            changed = focused.changed() => if changed.is_err() { return },
            push = pushes.recv() => match push {
                Ok(push) if push.method == "connection" => {
                    let now = serde_json::from_value::<Connection>(push.params).is_ok_and(|c| connected(&c));
                    inner.connected.send_if_modified(|was| std::mem::replace(was, now) != now);
                    if now && !online {
                        told = None;
                        inner.online.notify_waiters();
                    }
                    online = now;
                }
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            },
        }
    }
}
