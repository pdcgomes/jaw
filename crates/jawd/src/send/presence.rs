//! Presence: you're online while some interface has focus, as `Attention` keeps it, and away otherwise, so the phone keeps
//! its notifications while you're not looking. Focus has to hold for `Timing::online_after` before
//! WhatsApp hears you're online, and be gone for `Timing::away_after` before it hears you're away,
//! so switching windows doesn't flicker. Each time the connection comes back, WhatsApp is told
//! again, since it forgets; and whatever waits to send hears whether WhatsApp can be reached. Others'
//! typing is heard only while WhatsApp can be reached and has been told you're online, since only
//! then does it say it (`Inner::hearing`).

use std::sync::Arc;
use std::time::Duration;

use jaw_proto::{Connection, ConnectionState, Push};
use tokio::sync::{broadcast, watch};

use super::{Inner, engine};
use crate::engine::pb;

/// Whether WhatsApp can hear from jawd now: with the phone offline it can, as messages still
/// arrive and send then.
pub(super) fn connected(connection: &Connection) -> bool {
    matches!(connection.state, ConnectionState::Online | ConnectionState::CatchingUp | ConnectionState::PhoneOffline)
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
    let mut following = true;
    loop {
        let hears = online && told == Some(true);
        inner.hearing.send_if_modified(|was| std::mem::replace(was, hears) != hears);
        let want = following && *focused.borrow_and_update();
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
            // With nothing left to say whether an interface is in front, none is; the connection
            // is still followed, since sends wait for it.
            changed = focused.changed(), if following => following = changed.is_ok(),
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
