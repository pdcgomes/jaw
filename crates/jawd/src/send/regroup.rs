//! Group ticks after members leave: a member change only notes who left, and this looks at your
//! messages they were sent again in the background lane, a few at a time in short transactions, so
//! a change of hundreds of members never holds up a live message. Each step leaves the ticks right
//! for what it has looked at. It also lets go of who a message was sent to once it has got as far
//! as it can, 128 recipients a step.

use std::sync::Arc;
use std::time::Duration;

use super::Inner;
use crate::store::outbox::{self, Step};
use crate::store::{Change, Lane};
use crate::work::Priority;

/// How many members who left, and how many settled messages, are taken at once, and how many steps
/// each gets before the next lot.
const MEMBERS_AT_ONCE: usize = 64;
const MESSAGES_AT_ONCE: usize = 64;
const STEPS_EACH: usize = 4;
/// How long after a commit to look, so a burst of commits is looked at once.
const SETTLE: Duration = Duration::from_millis(250);

pub(super) fn start(inner: &Arc<Inner>) {
    let weak = Arc::downgrade(inner);
    let mut committed = inner.store.committed();
    tokio::spawn(async move {
        loop {
            let Some(inner) = weak.upgrade() else { return };
            let work = inner
                .store
                .reads()
                .query(Priority::Background, |db| {
                    Ok((outbox::regroups(db, MEMBERS_AT_ONCE)?, outbox::settled_messages(db, MESSAGES_AT_ONCE)?))
                })
                .await;
            let (left, settled) = match work {
                Ok(work) => work,
                Err(e) => {
                    eprintln!("jawd: can't read which members left groups: {e:#}");
                    Default::default()
                }
            };
            if left.is_empty() && settled.is_empty() {
                drop(inner);
                if committed.changed().await.is_err() {
                    return;
                }
                tokio::time::sleep(SETTLE).await;
                continue;
            }
            let regroup = left.iter().map(|&(chat, member)| Step::Regroup { chat, member });
            let let_go = settled.iter().map(|&message| Step::LetGo { message });
            let steps =
                regroup.chain(let_go).flat_map(|step| std::iter::repeat_n(Change::Outbox(step), STEPS_EACH)).collect();
            if let Err(e) = inner.store.apply(Lane::Background, steps).await {
                eprintln!("jawd: can't look at group ticks again after members left: {e:#}");
                tokio::time::sleep(SETTLE).await;
            }
        }
    });
}
