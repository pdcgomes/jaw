//! Chat actions from jaw: a chat pinned, archived, muted, or marked unread or read, and a message
//! starred, from any interface. Each is done to its row at once, with a new version and its event,
//! and the app-state patches that tell your other devices go into the outbox in the same
//! transaction, to go after the chat's earlier sends, as a deletion for you does. What's already so
//! changes nothing and queues nothing, so asking again after a lost answer, or two interfaces asking
//! at once, end the same. Each patch queued is its index's latest until its echo comes back
//! (`app_state_awaiting`), and WhatsApp sends app state back in the order it took it, so what it
//! sends for that index meanwhile came before the patch: it's kept raw but not applied
//! (`held_back`), since the patch sets every device as the row already is. A patch WhatsApp refuses
//! for good, or that runs out of tries, while it's its index's latest, is undone here (`undo`): what
//! it set goes back to how a chat is when nothing has set it, since fetching its collection again
//! brings only what WhatsApp holds, and the fetch then sets it as the phone has it.

use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::outbox::{OUTBOX_MOST, SHOWN_DELETED, app_state_row, waiting};
use super::{Amendment, Chat, Doing, Read, Stamps, apply_amendment, chat_jid, located, now_ms, set_read, upsert_chat};

/// The most chats pinned at once, as WhatsApp's apps allow; archived chats don't count.
pub const PINNED_MOST: u64 = 3;

const CHAT_STATE: &str =
    "SELECT n, kind, pinned, archived, muted_until, marked_unread, unread FROM chats WHERE id = ?1";
/// Pinned chats sort first, so the unarchived ones are a short range of chats_by_sort. A chat being
/// merged away is left out: the chat it goes into has its pin, so the pair counts once.
const PINNED_ELSEWHERE: &str = "SELECT count(*) FROM chats WHERE archived = 0 AND sort >= '1~' AND n <> ?1
    AND NOT EXISTS (SELECT 1 FROM chat_merges m WHERE m.gone = chats.n)";
const STARRED: &str = "SELECT starred FROM messages WHERE n = ?1";
const AWAITED_AT: &str = "SELECT at FROM app_state_awaiting WHERE key = ?1";
const AWAIT: &str = "INSERT INTO app_state_awaiting (key, at, outbox) VALUES (?1, ?2, ?3)
    ON CONFLICT (key) DO UPDATE SET at = excluded.at, outbox = excluded.outbox, echo_by = NULL";
/// What an index waits for: the time jaw's latest patch for it carries, where that patch's echo has
/// to be once WhatsApp has taken it, and whether its outbox row is still there.
const AWAITED: &str = "SELECT a.at, a.echo_by, EXISTS (SELECT 1 FROM outbox o WHERE o.n = a.outbox)
    FROM app_state_awaiting a WHERE a.key = ?1";
const WAITED: &str = "DELETE FROM app_state_awaiting WHERE key = ?1";
/// The waits whose echo had to be at or before an inbox row the converter has drained.
const PASSED: &str = "DELETE FROM app_state_awaiting WHERE echo_by IS NOT NULL AND echo_by <= ?1";

/// A chat action as the writer takes it: the chat as the interface named it, what to do, and where
/// to say what it came to. Each patch is an AppStatePatch the caller made, carrying the time it's
/// done at.
#[derive(Clone, Debug)]
pub struct Act {
    pub chat: String,
    pub does: Does,
    pub outcome: Outcome,
}

#[derive(Clone, Debug)]
pub enum Does {
    /// Something set on the chat.
    Set(Setting),
    /// Message `id` in the chat starred, or its star taken off.
    Star { id: String, starred: bool, patch: Patch },
}

#[derive(Clone, Debug)]
pub enum Setting {
    /// Pinned at `at`, the time its patch carries, which is positive, or unpinned when `None`.
    Pin { at: Option<i64>, patch: Patch },
    /// Archived or brought back. Archiving a pinned chat unpins it, as WhatsApp's apps do, and so
    /// does bringing one back while PINNED_MOST others are pinned, with `unpin` after `patch`;
    /// either needs `unpin`, which is otherwise unused.
    Archive { archived: bool, patch: Patch, unpin: Option<Patch> },
    /// Muted until `until`, in milliseconds (`FOREVER` for ever), or unmuted at 0.
    Mute { until: i64, patch: Patch },
    /// Marked unread; or the mark taken off and the chat read here up to `newest`, its newest
    /// message, which owes WhatsApp its read receipts. With none of its messages here, nothing is
    /// read until WhatsApp's echo of the patch reads it, as any device's read does, so a refused
    /// patch leaves the phone's count as it was.
    MarkUnread { unread: bool, newest: Option<String>, patch: Patch },
}

/// An app-state patch to queue: its outbox row's own ID, the AppStatePatch, encoded, and the index
/// and time it carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub id: String,
    pub raw: Vec<u8>,
    /// Its index as `app_state` keys it: the index's strings as JSON.
    pub key: String,
    /// Its SyncActionValue's timestamp.
    pub at: i64,
}

/// What a chat action came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Acted {
    /// Done, its patches queued as these outbox rows, in the order they go; `chat` is the chat's JID
    /// as jaw keeps it, and `chat_n` its n.
    Queued { chat: String, chat_n: i64, outbox: Vec<i64> },
    /// It was so already: nothing was written or queued.
    Already,
    /// The chat, or the message, isn't there.
    Gone,
    /// The message a star names is deleted for everyone, or being, so nothing was written.
    Deleted,
    /// The chat is a community's own row, whose groups carry these.
    Community,
    /// Pinning it would pin more than PINNED_MOST chats.
    Pinned,
    /// The outbox holds as many as it may, so nothing was written.
    Full { waiting: u64, failed: u64 },
}

/// Where a chat action says what it came to. The last attempt at its transaction is the one that
/// stands.
#[derive(Clone, Debug, Default)]
pub struct Outcome(Arc<Mutex<Option<Acted>>>);

impl Outcome {
    pub fn take(&self) -> Option<Acted> {
        self.0.lock().expect("a chat action's outcome").take()
    }

    fn put(&self, acted: Acted) {
        *self.0.lock().expect("a chat action's outcome") = Some(acted);
    }
}

/// What a refused patch set, to put back: the chat's field, or the message's star.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Undo {
    Pin { chat: String },
    Archive { chat: String },
    Mute { chat: String },
    MarkUnread { chat: String },
    Star { chat: String, id: String },
}

/// A chat as a chat action finds it.
struct State {
    n: i64,
    community: bool,
    pinned: i64,
    archived: bool,
    muted_until: i64,
    marked: bool,
    unread: i64,
}

fn state(db: &Connection, jid: &str) -> Result<Option<State>> {
    Ok(db
        .prepare_cached(CHAT_STATE)?
        .query_row([jid], |r| {
            Ok(State {
                n: r.get(0)?,
                community: r.get::<_, String>(1)? == "community",
                pinned: r.get(2)?,
                archived: r.get(3)?,
                muted_until: r.get(4)?,
                marked: r.get(5)?,
                unread: r.get(6)?,
            })
        })
        .optional()?)
}

/// Where a chat action came from, for the quarantine.
pub(super) fn provenance(act: &Act) -> (&'static str, String, Option<&[u8]>) {
    let (what, patch) = match &act.does {
        Does::Set(Setting::Pin { patch, .. }) => ("pin", patch),
        Does::Set(Setting::Archive { patch, .. }) => ("archive", patch),
        Does::Set(Setting::Mute { patch, .. }) => ("mute", patch),
        Does::Set(Setting::MarkUnread { patch, .. }) => ("mark", patch),
        Does::Star { patch, .. } => ("star", patch),
    };
    ("chat action", format!("{} {what} {}", act.chat, patch.id), Some(patch.raw.as_slice()))
}

pub(super) fn apply(db: &Connection, stamps: &mut Stamps, act: &Act) -> Result<bool> {
    let jid = chat_jid(db, &act.chat)?;
    let (acted, changed) = match &act.does {
        Does::Set(setting) => set(db, stamps, &jid, setting)?,
        Does::Star { id, starred, patch } => star(db, stamps, &jid, id, *starred, patch)?,
    };
    act.outcome.put(acted);
    Ok(changed)
}

/// Sets `setting` on chat `jid`, unless it's so already, and queues its patches.
fn set(db: &Connection, stamps: &mut Stamps, jid: &str, setting: &Setting) -> Result<(Acted, bool)> {
    let Some(chat) = state(db, jid)? else { return Ok((Acted::Gone, false)) };
    if chat.community {
        return Ok((Acted::Community, false));
    }
    let mut row = Chat { id: jid.to_string(), ..Chat::default() };
    let mut patches = Vec::new();
    let mut read = None;
    match setting {
        Setting::Pin { at, patch } => {
            if let Some(at) = at.filter(|&at| at <= 0) {
                bail!("a chat is pinned at a positive time, not {at}");
            }
            if at.is_some() == (chat.pinned > 0) {
                return Ok((Acted::Already, false));
            }
            if at.is_some() && pinned_elsewhere(db, chat.n)? >= PINNED_MOST {
                return Ok((Acted::Pinned, false));
            }
            row.pinned = Some(at.unwrap_or(0));
            patches.push(patch);
        }
        Setting::Archive { archived, patch, unpin } => {
            if *archived == chat.archived {
                return Ok((Acted::Already, false));
            }
            row.archived = Some(*archived);
            patches.push(patch);
            if chat.pinned > 0 && (*archived || pinned_elsewhere(db, chat.n)? >= PINNED_MOST) {
                let Some(unpin) = unpin else {
                    bail!("archiving or bringing back {jid} unpins it, so it needs its unpin patch");
                };
                row.pinned = Some(0);
                patches.push(unpin);
            }
        }
        Setting::Mute { until, patch } => {
            if *until == chat.muted_until {
                return Ok((Acted::Already, false));
            }
            row.muted_until = Some(*until);
            patches.push(patch);
        }
        Setting::MarkUnread { unread: true, patch, .. } => {
            if chat.marked {
                return Ok((Acted::Already, false));
            }
            row.marked_unread = Some(true);
            patches.push(patch);
        }
        Setting::MarkUnread { unread: false, newest, patch } => {
            if !chat.marked && chat.unread == 0 {
                return Ok((Acted::Already, false));
            }
            row.marked_unread = Some(false);
            read = newest.as_ref().map(|id| Read {
                chat: jid.to_string(),
                ids: vec![id.clone()],
                here: true,
                ..Read::default()
            });
            patches.push(patch);
        }
    }
    if let Some(full) = full(db, patches.len())? {
        return Ok((full, false));
    }
    let mut changed = upsert_chat(db, stamps, &row)?;
    if let Some(read) = &read {
        changed |= set_read(db, stamps, read)?;
    }
    let outbox = patches.iter().map(|patch| queue(db, jid, patch)).collect::<Result<Vec<_>>>()?;
    Ok((Acted::Queued { chat: jid.to_string(), chat_n: chat.n, outbox }, changed))
}

/// Queues `patch` for chat `jid` as an outbox row, and keeps it as its index's latest, which what
/// WhatsApp sends for the index waits behind until the patch's echo, known by its time, comes.
fn queue(db: &Connection, jid: &str, patch: &Patch) -> Result<i64> {
    let outbox = app_state_row(db, jid, &patch.id, &patch.raw)?;
    let awaited: Option<i64> = db.prepare_cached(AWAITED_AT)?.query_row([&patch.key], |r| r.get(0)).optional()?;
    if awaited == Some(patch.at) {
        bail!(
            "{} awaits jaw's patch at {} already, and no two of its patches for an index carry one time",
            patch.key,
            patch.at
        );
    }
    db.prepare_cached(AWAIT)?.execute(params![patch.key, patch.at, outbox])?;
    Ok(outbox)
}

/// Whether what WhatsApp sent for index `key`, at inbox row `seq` with its time `at`, is held back:
/// kept raw, but not applied. While jaw's latest patch for the index waits for its echo, whatever
/// WhatsApp sends for the index came before that patch, which sets every device as the row already
/// is. The patch's echo, known by its time, ends the wait; so does a mutation past `echo_by`, where
/// the echo had to be, or one after the patch's outbox row went without going; and each is applied.
pub(super) fn held_back(db: &Connection, key: &str, seq: i64, at: i64) -> Result<bool> {
    let awaited: Option<(i64, Option<i64>, bool)> =
        db.prepare_cached(AWAITED)?.query_row([key], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    let Some((awaited, echo_by, outboxed)) = awaited else { return Ok(false) };
    let after = match echo_by {
        Some(echo_by) => seq > echo_by,
        None => !outboxed,
    };
    if at != awaited && !after {
        return Ok(true);
    }
    db.prepare_cached(WAITED)?.execute([key])?;
    Ok(false)
}

/// Inbox row `seq` is drained, and the converter has gone past every row before it: an echo that
/// had to be at or before it would have ended its wait, so a wait still on there is over. A patch
/// still to go has no `echo_by`, and waits however far the converter has got.
pub(super) fn passed(db: &Connection, seq: i64) -> Result<()> {
    db.prepare_cached(PASSED)?.execute([seq])?;
    Ok(())
}

/// Stars message `id` in chat `jid`, or takes its star off, unless it's so already, and queues its
/// patch. A message deleted for everyone, or being, isn't starred: the request checks on a reader,
/// but a deletion can commit after that read, and while a chat merges, the copy it read may not be
/// the one found here.
fn star(
    db: &Connection,
    stamps: &mut Stamps,
    jid: &str,
    id: &str,
    starred: bool,
    patch: &Patch,
) -> Result<(Acted, bool)> {
    let Some((chat, n)) = located(db, stamps, jid, id)? else { return Ok((Acted::Gone, false)) };
    if starred && db.prepare_cached(SHOWN_DELETED)?.query_row([n], |r| r.get::<_, bool>(0))? {
        return Ok((Acted::Deleted, false));
    }
    let was: bool = db.prepare_cached(STARRED)?.query_row([n], |r| r.get(0))?;
    if was == starred {
        return Ok((Acted::Already, false));
    }
    if let Some(full) = full(db, 1)? {
        return Ok((full, false));
    }
    let amendment = Amendment::Star { starred };
    let changed =
        apply_amendment(db, stamps, chat, n, Doing { what: &amendment, sender: None, mine: false, at: now_ms() })?;
    let outbox = queue(db, jid, patch)?;
    Ok((Acted::Queued { chat: jid.to_string(), chat_n: chat, outbox: vec![outbox] }, changed))
}

/// How many chats but `n` are pinned and not archived, a person's two chats counting once while
/// they merge.
fn pinned_elsewhere(db: &Connection, n: i64) -> Result<u64> {
    let count: i64 = db.prepare_cached(PINNED_ELSEWHERE)?.query_row([n], |r| r.get(0))?;
    Ok(count.max(0) as u64)
}

/// The outbox's answer when it has no room for `rows` more.
fn full(db: &Connection, rows: usize) -> Result<Option<Acted>> {
    let (waiting, failed) = waiting(db)?;
    Ok((waiting + rows as u64 > OUTBOX_MOST).then_some(Acted::Full { waiting, failed }))
}

/// Puts back what a refused patch set: the chat's field as it is when nothing has set it, or the
/// message unstarred. A chat or message that's gone has nothing to put back, and none is made.
pub(super) fn undo(db: &Connection, stamps: &mut Stamps, undo: &Undo) -> Result<bool> {
    let (chat, unset) = match undo {
        Undo::Star { chat, id } => {
            let Some((chat, n)) = located(db, stamps, &chat_jid(db, chat)?, id)? else { return Ok(false) };
            let unstarred = Amendment::Star { starred: false };
            return apply_amendment(
                db,
                stamps,
                chat,
                n,
                Doing { what: &unstarred, sender: None, mine: false, at: now_ms() },
            );
        }
        Undo::Pin { chat } => (chat, Chat { pinned: Some(0), ..Chat::default() }),
        Undo::Archive { chat } => (chat, Chat { archived: Some(false), ..Chat::default() }),
        Undo::Mute { chat } => (chat, Chat { muted_until: Some(0), ..Chat::default() }),
        Undo::MarkUnread { chat } => (chat, Chat { marked_unread: Some(false), ..Chat::default() }),
    };
    let jid = chat_jid(db, chat)?;
    if state(db, &jid)?.is_none() {
        return Ok(false);
    }
    upsert_chat(db, stamps, &Chat { id: jid, ..unset })
}

#[cfg(test)]
mod tests {
    use prost::Message as _;
    use wa_proto::wa_web_protobuf_sync_action as action;

    use super::*;
    use crate::convert::app_state::FOREVER;
    use crate::store::outbox::{Step, Written};
    use crate::store::schema;
    use crate::store::{Amend, Change, Drained, Message, Person};

    const ANA: &str = "351910000002@s.whatsapp.net";
    const ANA_LID: &str = "100000000002@lid";
    const RUI: &str = "351910000003@s.whatsapp.net";
    const COMMUNITY: &str = "120363000000000009@g.us";
    const AT: i64 = 1_792_000_000_000;

    fn db() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        db
    }

    /// Makes `changes` in one transaction as the writer does, settle and all.
    fn written(db: &Connection, changes: &[Change]) {
        let mut stamps = Stamps::load(db).unwrap();
        stamps.begin();
        for change in changes {
            super::super::apply(db, &mut stamps, change).unwrap();
        }
        super::super::settle(db, &mut stamps).unwrap();
    }

    /// A patch standing in for one, for an index of its own.
    fn patch(id: &str) -> Patch {
        Patch { id: id.into(), raw: id.as_bytes().to_vec(), key: serde_json::to_string(&[id]).unwrap(), at: AT }
    }

    fn pin(at: Option<i64>, id: &str) -> Does {
        Does::Set(Setting::Pin { at, patch: patch(id) })
    }

    /// An archive or its undoing, with the unpin patch the handler builds for every one.
    fn archive(archived: bool, id: &str) -> Does {
        Does::Set(Setting::Archive { archived, patch: patch(id), unpin: Some(patch(&format!("{id} unpin"))) })
    }

    /// Does `does` to `chat` as the writer does, and says what it came to.
    fn acted(db: &Connection, chat: &str, does: Does) -> Acted {
        let outcome = Outcome::default();
        written(db, &[Change::ChatAction(Act { chat: chat.into(), does, outcome: outcome.clone() })]);
        outcome.take().expect("a chat action says what it came to")
    }

    /// Does `does` to `chat` as the writer does, for a caller's mistake: it fails, saying nothing of
    /// what it came to, and what it did is rolled back, as the writer's savepoint is.
    fn refused(db: &Connection, chat: &str, does: Does) -> anyhow::Error {
        let outcome = Outcome::default();
        let mut stamps = Stamps::load(db).unwrap();
        stamps.begin();
        let act = Change::ChatAction(Act { chat: chat.into(), does, outcome: outcome.clone() });
        let tx = db.unchecked_transaction().unwrap();
        let error = super::super::apply(&tx, &mut stamps, &act).expect_err("a caller's mistake fails");
        drop(tx);
        assert_eq!(outcome.take(), None);
        error
    }

    fn chats(db: &Connection, ids: &[&str]) {
        let changes: Vec<Change> =
            ids.iter().map(|id| Change::Chat(Chat { id: id.to_string(), ..Chat::default() })).collect();
        written(db, &changes);
    }

    /// Chat `id`'s pinned, archived, muted_until, marked_unread, unread and v.
    fn chat(db: &Connection, id: &str) -> (i64, bool, i64, bool, i64, i64) {
        db.query_row(
            "SELECT pinned, archived, muted_until, marked_unread, unread, v FROM chats WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap()
    }

    /// The app-state rows queued, oldest first, as (jid, id, raw).
    fn queued(db: &Connection) -> Vec<(String, String, Vec<u8>)> {
        db.prepare("SELECT jid, id, raw FROM outbox WHERE kind = 'app_state' ORDER BY n")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// Puts `rows` reactions in the outbox, waiting to go.
    fn outbox_holding(db: &Connection, rows: u64) {
        for n in 0..rows {
            db.execute(
                "INSERT INTO outbox (jid, id, kind, raw, next_at, created_at) VALUES (?1, ?2, 'reaction', x'', 0, 0)",
                rusqlite::params![ANA, format!("w{n}")],
            )
            .unwrap();
        }
    }

    /// Pins `pinned` from jaw, then has `archived` pinned and archived, as the phone may leave one.
    fn pinned_and_one_archived(db: &Connection, pinned: &[&str], archived: &str) {
        chats(db, pinned);
        for id in pinned {
            assert!(matches!(acted(db, id, pin(Some(AT), id)), Acted::Queued { .. }), "{id}");
        }
        written(
            db,
            &[Change::Chat(Chat { id: archived.into(), pinned: Some(AT), archived: Some(true), ..Chat::default() })],
        );
    }

    /// How many chats are pinned and not archived.
    fn pinned_unarchived(db: &Connection) -> i64 {
        db.query_row("SELECT count(*) FROM chats WHERE pinned > 0 AND archived = 0", [], |r| r.get(0)).unwrap()
    }

    /// Ana's chats under her number and under her LID, both pinned, the second being merged into the
    /// first, and one more chat pinned: three pinned rows for two conversations.
    fn pinned_while_merging(db: &Connection) {
        let pinned = |id: &str| Change::Chat(Chat { id: id.into(), pinned: Some(AT), ..Chat::default() });
        written(db, &[pinned(ANA), pinned(ANA_LID), pinned(RUI)]);
        written(db, &[Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]);
        let merging: i64 = db.query_row("SELECT count(*) FROM chat_merges", [], |r| r.get(0)).unwrap();
        assert_eq!((merging, pinned_unarchived(db)), (1, 3), "Ana's two chats merging, all three pinned");
    }

    fn from_ana(id: &str, at: i64) -> Change {
        Change::Message(Message {
            chat: ANA.into(),
            id: id.into(),
            at,
            sender: Some(ANA.into()),
            kind: "text",
            text: Some(id.into()),
            summary: id.into(),
            live: true,
            ..Message::default()
        })
    }

    #[test]
    fn a_pin_is_written_with_its_time_a_new_version_and_its_patch_and_asked_again_does_nothing() {
        let db = db();
        chats(&db, &[ANA]);
        let (.., v) = chat(&db, ANA);
        let Acted::Queued { chat: kept, outbox, .. } = acted(&db, ANA, pin(Some(AT), "p1")) else {
            panic!("not queued")
        };
        assert_eq!((kept.as_str(), outbox.len()), (ANA, 1));
        let (pinned, .., pinned_v) = chat(&db, ANA);
        assert_eq!(pinned, AT, "pinned when jawd said, the time its patch carries");
        assert!(pinned_v > v, "with a new version");
        let event: String = db.query_row("SELECT type FROM events WHERE seq = ?1", [pinned_v], |r| r.get(0)).unwrap();
        assert_eq!(event, "chat", "and its event, in the same transaction");
        assert_eq!(queued(&db), [(ANA.to_string(), "p1".to_string(), b"p1".to_vec())]);
        assert_eq!(acted(&db, ANA, pin(Some(AT + 5), "p2")), Acted::Already, "pinned already");
        let (pinned, .., again_v) = chat(&db, ANA);
        assert_eq!((pinned, again_v, queued(&db).len()), (AT, pinned_v, 1), "nothing written or queued");
        assert!(matches!(acted(&db, ANA, pin(None, "p3")), Acted::Queued { .. }));
        assert_eq!(chat(&db, ANA).0, 0);
        assert_eq!(acted(&db, ANA, pin(None, "p4")), Acted::Already);
        assert_eq!(queued(&db).len(), 2);
    }

    #[test]
    fn at_most_three_chats_are_pinned_and_archived_ones_dont_count() {
        let db = db();
        let ids = [
            "351910000011@s.whatsapp.net",
            "351910000012@s.whatsapp.net",
            "351910000013@s.whatsapp.net",
            "351910000014@s.whatsapp.net",
            "351910000015@s.whatsapp.net",
        ];
        chats(&db, &ids);
        written(
            &db,
            &[Change::Chat(Chat { id: ids[4].into(), pinned: Some(AT), archived: Some(true), ..Chat::default() })],
        );
        for (n, id) in ids[..3].iter().enumerate() {
            assert!(matches!(acted(&db, id, pin(Some(AT + n as i64), id)), Acted::Queued { .. }), "{id}");
        }
        assert_eq!(acted(&db, ids[3], pin(Some(AT + 9), "fourth")), Acted::Pinned);
        assert_eq!((chat(&db, ids[3]).0, queued(&db).len()), (0, 3), "not pinned, and nothing queued");
        assert!(matches!(acted(&db, ids[0], pin(None, "unpin")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, ids[3], pin(Some(AT + 10), "now")), Acted::Queued { .. }), "a place came free");
    }

    #[test]
    fn archiving_a_pinned_chat_unpins_it_with_a_second_patch_after_the_first() {
        let db = db();
        chats(&db, &[ANA, RUI]);
        written(&db, &[Change::Chat(Chat { id: ANA.into(), pinned: Some(AT), ..Chat::default() })]);
        let Acted::Queued { outbox, .. } = acted(&db, ANA, archive(true, "a1")) else { panic!("not queued") };
        assert_eq!(outbox.len(), 2);
        let (pinned, archived, ..) = chat(&db, ANA);
        assert_eq!((pinned, archived), (0, true), "archived, and unpinned as WhatsApp's apps do");
        let ids: Vec<String> = queued(&db).into_iter().map(|(_, id, _)| id).collect();
        assert_eq!(ids, ["a1", "a1 unpin"], "the archive first, then the unpin");
        assert_eq!(acted(&db, ANA, archive(true, "a2")), Acted::Already);
        let Acted::Queued { outbox, .. } = acted(&db, RUI, archive(true, "r1")) else { panic!("not queued") };
        assert_eq!(outbox.len(), 1, "a chat that isn't pinned has nothing to unpin");
        assert!(matches!(acted(&db, ANA, archive(false, "a3")), Acted::Queued { .. }));
        assert!(!chat(&db, ANA).1);
    }

    #[test]
    fn bringing_back_a_pinned_chat_while_three_others_are_pinned_unpins_it_with_a_second_patch_after_the_first() {
        let db = db();
        let [a, b, c, d] = [
            "351910000021@s.whatsapp.net",
            "351910000022@s.whatsapp.net",
            "351910000023@s.whatsapp.net",
            "351910000024@s.whatsapp.net",
        ];
        pinned_and_one_archived(&db, &[a, b, d], c);
        let without = Does::Set(Setting::Archive { archived: false, patch: patch("c0"), unpin: None });
        assert!(refused(&db, c, without).to_string().contains("unpin patch"), "a fourth needs its unpin");
        let Acted::Queued { outbox, .. } = acted(&db, c, archive(false, "c1")) else { panic!("not queued") };
        assert_eq!(outbox.len(), 2);
        let (pinned, archived, ..) = chat(&db, c);
        assert_eq!((pinned, archived), (0, false), "brought back, and unpinned, as it would be a fourth");
        let ids: Vec<String> = queued(&db).into_iter().filter(|(jid, ..)| jid == c).map(|(_, id, _)| id).collect();
        assert_eq!(ids, ["c1", "c1 unpin"], "the archive patch first, then the unpin");
        assert_eq!(pinned_unarchived(&db), 3);
    }

    #[test]
    fn bringing_back_a_pinned_chat_with_fewer_than_three_others_pinned_keeps_it_pinned_with_one_patch() {
        let db = db();
        let [a, b, c] = ["351910000021@s.whatsapp.net", "351910000022@s.whatsapp.net", "351910000023@s.whatsapp.net"];
        pinned_and_one_archived(&db, &[a, b], c);
        let Acted::Queued { outbox, .. } = acted(&db, c, archive(false, "c1")) else { panic!("not queued") };
        assert_eq!(outbox.len(), 1, "its unpin patch isn't used");
        let (pinned, archived, ..) = chat(&db, c);
        assert_eq!((pinned, archived), (AT, false), "brought back, and still pinned");
        assert_eq!(pinned_unarchived(&db), 3);
    }

    #[test]
    fn a_chat_being_merged_away_isnt_counted_so_another_can_be_pinned_while_a_persons_two_chats_merge() {
        let db = db();
        pinned_while_merging(&db);
        let [d, e] = ["351910000031@s.whatsapp.net", "351910000032@s.whatsapp.net"];
        chats(&db, &[d, e]);
        assert!(matches!(acted(&db, d, pin(Some(AT), "d1")), Acted::Queued { .. }), "Ana's two chats count once");
        assert_eq!(chat(&db, d).0, AT);
        assert_eq!(acted(&db, e, pin(Some(AT), "e1")), Acted::Pinned, "and they still count");
    }

    #[test]
    fn bringing_back_a_pinned_chat_while_a_persons_two_chats_merge_keeps_its_pin() {
        let db = db();
        pinned_while_merging(&db);
        let c = "351910000033@s.whatsapp.net";
        written(&db, &[Change::Chat(Chat { id: c.into(), pinned: Some(AT), archived: Some(true), ..Chat::default() })]);
        let Acted::Queued { outbox, .. } = acted(&db, c, archive(false, "c1")) else { panic!("not queued") };
        assert_eq!(outbox.len(), 1, "its unpin patch isn't used");
        let (pinned, archived, ..) = chat(&db, c);
        assert_eq!((pinned, archived), (AT, false), "brought back, and still pinned, as a third");
    }

    #[test]
    fn a_mute_sets_until_when_and_the_same_again_does_nothing() {
        let db = db();
        chats(&db, &[ANA]);
        let mute = |until: i64, id: &str| Does::Set(Setting::Mute { until, patch: patch(id) });
        assert!(matches!(acted(&db, ANA, mute(AT, "m1")), Acted::Queued { .. }));
        assert_eq!(chat(&db, ANA).2, AT);
        assert_eq!(acted(&db, ANA, mute(AT, "m2")), Acted::Already);
        assert!(matches!(acted(&db, ANA, mute(crate::convert::app_state::FOREVER, "m3")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, ANA, mute(0, "m4")), Acted::Queued { .. }));
        assert_eq!((chat(&db, ANA).2, queued(&db).len()), (0, 3));
    }

    #[test]
    fn marking_unread_sets_the_mark_and_marking_read_takes_it_off_and_reads_up_to_the_newest() {
        let db = db();
        written(&db, &[from_ana("m1", AT), from_ana("m2", AT + 10)]);
        assert_eq!(chat(&db, ANA).4, 2, "two unread");
        let mark = |unread: bool, id: &str| {
            Does::Set(Setting::MarkUnread { unread, newest: Some("m2".into()), patch: patch(id) })
        };
        assert!(matches!(acted(&db, ANA, mark(true, "u1")), Acted::Queued { .. }));
        assert!(chat(&db, ANA).3);
        assert_eq!(acted(&db, ANA, mark(true, "u2")), Acted::Already);
        let Acted::Queued { chat_n, .. } = acted(&db, ANA, mark(false, "r1")) else { panic!("not queued") };
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!((marked, unread), (false, 0), "the mark gone, and read up to the newest");
        let owes: bool =
            db.query_row("SELECT told_at IS NOT NULL FROM chats WHERE n = ?1", [chat_n], |r| r.get(0)).unwrap();
        assert!(owes, "read here, its read receipts are owed");
        assert_eq!(acted(&db, ANA, mark(false, "r2")), Acted::Already, "nothing marked and nothing unread");
        assert_eq!(queued(&db).len(), 2);
    }

    /// Ana's chat as the phone left it: three unread, none of them here, and marked unread if
    /// `marked`.
    fn counted_on_the_phone(db: &Connection, marked: bool) {
        written(db, &[Change::Read(Read { chat: ANA.into(), at: AT, phone: Some((3, 0)), ..Read::default() })]);
        if marked {
            written(db, &[Change::Chat(Chat { id: ANA.into(), marked_unread: Some(true), ..Chat::default() })]);
        }
        let (.., is_marked, unread, _) = chat(db, ANA);
        assert_eq!((is_marked, unread), (marked, 3), "the phone's count, with none of its messages here");
    }

    #[test]
    fn marking_read_a_chat_jaw_holds_none_of_the_messages_of_takes_the_mark_off_and_leaves_the_read_to_its_echo() {
        let db = db();
        counted_on_the_phone(&db, true);
        let v = chat(&db, ANA).5;
        let at = AT + 1_500;
        assert!(matches!(acted(&db, ANA, mark_read_at(ANA, at, "r1")), Acted::Queued { .. }));
        let (.., marked, unread, read_v) = chat(&db, ANA);
        assert_eq!((marked, unread), (false, 3), "the mark taken off, and the phone's count left");
        assert!(read_v > v, "with a new version");
        arrives(&db, &["markChatAsRead", ANA], &read_value(at), 1);
        let (.., marked, unread, echoed_v) = chat(&db, ANA);
        assert_eq!((marked, unread), (false, 0), "WhatsApp's echo of the patch reads it, clearing the count");
        assert!(echoed_v > read_v, "with a version of its own");
        let read_at: i64 = db.query_row("SELECT read_at FROM chats WHERE id = ?1", [ANA], |r| r.get(0)).unwrap();
        assert_eq!(read_at, at - at % 1_000, "read at the patch's time, to the second its range gives");
        assert_eq!(acted(&db, ANA, mark_read_at(ANA, at + 1, "r2")), Acted::Already, "read now, it's so already");
        assert_eq!(queued(&db).len(), 1, "and no second patch");
    }

    #[test]
    fn marking_read_again_before_the_echo_queues_a_second_patch_and_only_its_echo_reads_the_chat() {
        let db = db();
        counted_on_the_phone(&db, false);
        let index = ["markChatAsRead", ANA];
        let (t1, t2) = (AT + 1_000, AT + 2_000);
        assert!(matches!(acted(&db, ANA, mark_read_at(ANA, t1, "r1")), Acted::Queued { .. }));
        let again = acted(&db, ANA, mark_read_at(ANA, t2, "r2"));
        assert!(matches!(again, Acted::Queued { .. }), "its count still there, it isn't so already: {again:?}");
        assert_eq!(queued(&db).len(), 2);
        let before = chat(&db, ANA);
        arrives(&db, &index, &read_value(t1), 1);
        assert_eq!(chat(&db, ANA), before, "the first patch's echo, taken before the second, is held back");
        arrives(&db, &index, &read_value(t2), 2);
        assert_eq!((chat(&db, ANA).4, awaited(&db, &index)), (0, None), "the second's reads it, and ends the wait");
    }

    #[test]
    fn a_refused_mark_read_of_a_chat_jaw_holds_none_of_the_messages_of_leaves_the_phones_count() {
        let db = db();
        counted_on_the_phone(&db, true);
        let Acted::Queued { outbox, .. } = acted(&db, ANA, mark_read_at(ANA, AT + 1_000, "r1")) else {
            panic!("not queued")
        };
        let undo = Some(Undo::MarkUnread { chat: ANA.into() });
        let refused = Step::Refused { outbox: outbox[0], error: "server returned error 400".into(), undo };
        written(&db, &[Change::Outbox(refused)]);
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!((marked, unread), (false, 3), "the mark as nothing had set it, and the phone's count as it was");
        assert_eq!(awaited(&db, &["markChatAsRead", ANA]), None, "the wait over");
    }

    #[test]
    fn marking_read_is_so_already_only_for_a_chat_with_no_mark_and_nothing_unread() {
        let db = db();
        let untouched = "351910000004@s.whatsapp.net";
        written(
            &db,
            &[from_ana("m1", AT), Change::Chat(Chat { id: RUI.into(), marked_unread: Some(true), ..Chat::default() })],
        );
        chats(&db, &[untouched]);
        let read = |newest: Option<&str>, id: &str| {
            let newest = newest.map(str::to_string);
            Does::Set(Setting::MarkUnread { unread: false, newest, patch: patch(id) })
        };
        assert!(matches!(acted(&db, ANA, read(Some("m1"), "a1")), Acted::Queued { .. }), "unread, though not marked");
        assert_eq!(chat(&db, ANA).4, 0, "and read");
        assert!(matches!(acted(&db, RUI, read(None, "r1")), Acted::Queued { .. }), "marked, though nothing's unread");
        assert!(!chat(&db, RUI).3, "and the mark taken off");
        assert_eq!(acted(&db, untouched, read(None, "u1")), Acted::Already, "neither marked nor unread");
        assert_eq!(queued(&db).len(), 2);
    }

    #[test]
    fn a_star_marks_the_message_with_a_new_version_and_the_same_again_does_nothing() {
        let db = db();
        written(&db, &[from_ana("m1", AT)]);
        let star = |starred: bool, id: &str| Does::Star { id: "m1".into(), starred, patch: patch(id) };
        let message = |db: &Connection| -> (bool, i64) {
            db.query_row("SELECT starred, v FROM messages WHERE id = 'm1'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
        };
        let (_, before) = message(&db);
        assert!(matches!(acted(&db, ANA, star(true, "s1")), Acted::Queued { .. }));
        let (starred, after) = message(&db);
        assert!(starred && after > before);
        assert_eq!(acted(&db, ANA, star(true, "s2")), Acted::Already);
        let missing = Does::Star { id: "nope".into(), starred: true, patch: patch("s3") };
        assert_eq!(acted(&db, ANA, missing), Acted::Gone);
        assert!(matches!(acted(&db, ANA, star(false, "s4")), Acted::Queued { .. }));
        assert_eq!((message(&db).0, queued(&db).len()), (false, 2));
    }

    #[test]
    fn a_star_without_room_in_the_outbox_is_refused_as_full_and_writes_nothing() {
        let db = db();
        written(&db, &[from_ana("m1", AT)]);
        outbox_holding(&db, OUTBOX_MOST);
        let message = |db: &Connection| -> (bool, i64) {
            db.query_row("SELECT starred, v FROM messages WHERE id = 'm1'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
        };
        let before = message(&db);
        let star = Does::Star { id: "m1".into(), starred: true, patch: patch("s1") };
        assert_eq!(acted(&db, ANA, star), Acted::Full { waiting: 1_000, failed: 0 });
        assert_eq!((message(&db), queued(&db).len()), (before, 0), "nothing written or queued");
    }

    #[test]
    fn a_star_of_a_message_deleted_for_everyone_or_being_writes_nothing_but_its_star_can_go() {
        let db = db();
        let yours = Message {
            chat: ANA.into(),
            id: "m1".into(),
            at: AT,
            mine: true,
            kind: "text",
            status: Some("sent"),
            text: Some("m1".into()),
            summary: "m1".into(),
            live: true,
            ..Message::default()
        };
        written(&db, &[Change::Message(yours), from_ana("m2", AT + 1)]);
        let star = |id: &str, starred: bool, by: &str| Does::Star { id: id.into(), starred, patch: patch(by) };
        assert!(matches!(acted(&db, ANA, star("m1", true, "s1")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, ANA, star("m2", true, "s2")), Acted::Queued { .. }));
        // Your deletion of m1 for everyone is queued, and Ana deletes m2 for everyone.
        let deleting = Change::Outbox(Step::Act {
            chat: ANA.into(),
            target: "m1".into(),
            id: "x1".into(),
            effect: Amendment::Revoke,
            at: AT + 2,
            raw: vec![1],
            key: None,
            written: Written::default(),
        });
        let hers = Amend {
            chat: ANA.into(),
            target: "m2".into(),
            sender: Some(ANA.into()),
            mine: false,
            at: AT + 3,
            what: Amendment::Revoke,
            raw: None,
        };
        written(&db, &[deleting, Change::Amend(hers)]);
        let message = |id: &str| -> (bool, bool, i64) {
            db.query_row("SELECT starred, deleted, v FROM messages WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap()
        };
        assert_eq!((message("m1").1, message("m2").1), (false, true), "m1 being deleted, m2 deleted");
        for (id, unstar, again) in [("m1", "u1", "s3"), ("m2", "u2", "s4")] {
            assert!(matches!(acted(&db, ANA, star(id, false, unstar)), Acted::Queued { .. }), "{id}'s star can go");
            let unstarred = message(id);
            assert!(!unstarred.0, "{id} unstarred");
            let rows = queued(&db).len();
            assert_eq!(acted(&db, ANA, star(id, true, again)), Acted::Deleted, "{id}");
            assert_eq!((message(id), queued(&db).len()), (unstarred, rows), "{id}: nothing written or queued");
        }
    }

    #[test]
    fn a_chat_jaw_doesnt_have_or_a_communitys_own_row_is_left_alone() {
        let db = db();
        written(&db, &[Change::Chat(Chat { id: COMMUNITY.into(), kind: Some("community"), ..Chat::default() })]);
        assert_eq!(acted(&db, ANA, pin(Some(AT), "p1")), Acted::Gone);
        assert_eq!(acted(&db, COMMUNITY, pin(Some(AT), "p2")), Acted::Community);
        let chats: i64 = db.query_row("SELECT count(*) FROM chats", [], |r| r.get(0)).unwrap();
        assert_eq!((chats, queued(&db).len()), (1, 0), "no chat made, nothing queued");
    }

    #[test]
    fn without_room_in_the_outbox_for_its_patches_nothing_is_written() {
        let db = db();
        chats(&db, &[ANA, RUI]);
        written(&db, &[Change::Chat(Chat { id: ANA.into(), pinned: Some(AT), ..Chat::default() })]);
        outbox_holding(&db, OUTBOX_MOST - 1);
        let archive =
            Does::Set(Setting::Archive { archived: true, patch: patch("a1"), unpin: Some(patch("a1 unpin")) });
        assert_eq!(acted(&db, ANA, archive), Acted::Full { waiting: 999, failed: 0 }, "two places, one left");
        let (pinned, archived, ..) = chat(&db, ANA);
        assert_eq!((pinned, archived), (AT, false), "nothing written");
        assert!(matches!(acted(&db, RUI, pin(Some(AT), "r1")), Acted::Queued { .. }), "the last place");
        assert_eq!(acted(&db, RUI, pin(None, "r2")), Acted::Full { waiting: 1_000, failed: 0 });
    }

    #[test]
    fn archiving_a_pinned_chat_without_its_unpin_patch_or_pinning_at_a_time_that_isnt_positive_is_refused() {
        let db = db();
        chats(&db, &[ANA, RUI]);
        written(&db, &[Change::Chat(Chat { id: ANA.into(), pinned: Some(AT), ..Chat::default() })]);
        let without = Does::Set(Setting::Archive { archived: true, patch: patch("a1"), unpin: None });
        assert!(refused(&db, ANA, without).to_string().contains("unpin patch"));
        let (pinned, archived, ..) = chat(&db, ANA);
        assert_eq!((pinned, archived), (AT, false), "neither archived nor unpinned");
        for at in [0, -1] {
            assert!(refused(&db, RUI, pin(Some(at), "p1")).to_string().contains("positive"), "pinned at {at}");
        }
        assert_eq!((chat(&db, RUI).0, queued(&db).len()), (0, 0), "not pinned, and nothing queued");
    }

    #[test]
    fn undoing_puts_back_how_a_chat_is_when_nothing_set_it_and_makes_no_chat() {
        let db = db();
        written(&db, &[from_ana("m1", AT)]);
        let set = Chat {
            id: ANA.into(),
            pinned: Some(AT),
            archived: Some(true),
            muted_until: Some(AT),
            marked_unread: Some(true),
            ..Chat::default()
        };
        let star = Amend {
            chat: ANA.into(),
            target: "m1".into(),
            sender: None,
            mine: false,
            at: AT,
            what: Amendment::Star { starred: true },
            raw: None,
        };
        written(&db, &[Change::Chat(set), Change::Amend(star)]);
        let before = chat(&db, ANA).5;
        let undone = |db: &Connection, undos: &[Undo]| {
            let mut stamps = Stamps::load(db).unwrap();
            stamps.begin();
            for one in undos {
                undo(db, &mut stamps, one).unwrap();
            }
            super::super::settle(db, &mut stamps).unwrap();
        };
        undone(
            &db,
            &[
                Undo::Pin { chat: ANA.into() },
                Undo::Archive { chat: ANA.into() },
                Undo::Mute { chat: ANA.into() },
                Undo::MarkUnread { chat: ANA.into() },
                Undo::Star { chat: ANA.into(), id: "m1".into() },
            ],
        );
        let (pinned, archived, muted, marked, _, v) = chat(&db, ANA);
        assert_eq!((pinned, archived, muted, marked), (0, false, 0, false));
        assert!(v > before, "with a new version, so every window is sent it");
        let starred: bool = db.query_row("SELECT starred FROM messages WHERE id = 'm1'", [], |r| r.get(0)).unwrap();
        assert!(!starred);
        undone(&db, &[Undo::Pin { chat: RUI.into() }, Undo::Star { chat: RUI.into(), id: "x".into() }]);
        let chats: i64 = db.query_row("SELECT count(*) FROM chats", [], |r| r.get(0)).unwrap();
        assert_eq!(chats, 1, "a chat that isn't there isn't made");
    }

    /// A chat pinned at `at`, or unpinned, as jaw's patch and the phone's own mutation carry it.
    fn pin_value(pinned: bool, at: i64) -> action::SyncActionValue {
        action::SyncActionValue {
            timestamp: Some(at),
            pin_action: Some(action::PinAction { pinned: Some(pinned) }),
            ..Default::default()
        }
    }

    /// A chat muted until `until`, for ever at FOREVER, or unmuted at 0.
    fn mute_value(until: i64, at: i64) -> action::SyncActionValue {
        let mute = match until {
            0 => action::MuteAction { muted: Some(false), ..Default::default() },
            FOREVER => action::MuteAction { muted: Some(true), mute_end_timestamp: Some(-1), ..Default::default() },
            until => action::MuteAction { muted: Some(true), mute_end_timestamp: Some(until), ..Default::default() },
        };
        action::SyncActionValue { timestamp: Some(at), mute_action: Some(mute), ..Default::default() }
    }

    /// jaw's patch for `index` carrying `value`, as the handler queues it, `id` for its outbox row.
    fn patch_of(index: &[&str], value: &action::SyncActionValue, id: &str) -> Patch {
        Patch {
            id: id.into(),
            raw: value.encode_to_vec(),
            key: serde_json::to_string(index).unwrap(),
            at: value.timestamp.unwrap(),
        }
    }

    /// Chat `chat` pinned from jaw at `at`, or unpinned.
    fn pin_at(chat: &str, pinned: bool, at: i64, id: &str) -> Does {
        let patch = patch_of(&["pin_v1", chat], &pin_value(pinned, at), id);
        Does::Set(Setting::Pin { at: pinned.then_some(at), patch })
    }

    /// A chat read at `at`, naming no message, as jaw's patch for a chat it holds none of the
    /// messages of carries it.
    fn read_value(at: i64) -> action::SyncActionValue {
        let range = action::SyncActionMessageRange { last_message_timestamp: Some(at / 1000), ..Default::default() };
        action::SyncActionValue {
            timestamp: Some(at),
            mark_chat_as_read_action: Some(action::MarkChatAsReadAction {
                read: Some(true),
                message_range: Some(range),
            }),
            ..Default::default()
        }
    }

    /// Chat `chat`, none of whose messages jaw holds, marked read from jaw at `at`.
    fn mark_read_at(chat: &str, at: i64, id: &str) -> Does {
        let patch = patch_of(&["markChatAsRead", chat], &read_value(at), id);
        Does::Set(Setting::MarkUnread { unread: false, newest: None, patch })
    }

    /// What WhatsApp sends for `index`, converted from inbox row `seq` as the drain converts it.
    fn arrives(db: &Connection, index: &[&str], value: &action::SyncActionValue, seq: i64) {
        let index: Vec<String> = index.iter().map(|part| part.to_string()).collect();
        let value = value.encode_to_vec();
        let applied = crate::convert::app_state::mutation(&index, &value);
        let state = Change::AppState { index, value, seq, at: applied.at, changes: applied.changes };
        written(db, &[state, Change::Drained(Drained::Inbox(seq))]);
    }

    /// Engine events waiting in the inbox as rows `seqs`.
    fn inboxed(db: &Connection, seqs: &[i64]) {
        for seq in seqs {
            db.execute("INSERT INTO inbox (seq, kind, at, payload) VALUES (?1, 'app_state', 0, x'')", [seq]).unwrap();
        }
    }

    /// WhatsApp has taken the patch at outbox row `outbox`, as the engine's answer says.
    fn sent(db: &Connection, outbox: i64) {
        written(db, &[Change::Outbox(Step::Sent { outbox })]);
    }

    /// What `index` waits for of jaw's: the time its latest patch carries, that patch's outbox row,
    /// and where its echo has to be.
    fn awaited(db: &Connection, index: &[&str]) -> Option<(i64, i64, Option<i64>)> {
        db.query_row(
            "SELECT at, outbox, echo_by FROM app_state_awaiting WHERE key = ?1",
            [serde_json::to_string(index).unwrap()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .unwrap()
    }

    /// What `app_state` keeps raw for `index`.
    fn kept(db: &Connection, index: &[&str]) -> Option<Vec<u8>> {
        db.query_row("SELECT value FROM app_state WHERE key = ?1", [serde_json::to_string(index).unwrap()], |r| {
            r.get(0)
        })
        .optional()
        .unwrap()
    }

    #[test]
    fn an_echo_from_before_jaws_next_change_is_held_back_so_asking_again_isnt_so_already() {
        let db = db();
        chats(&db, &[ANA]);
        let index = ["pin_v1", ANA];
        let (t1, t2, t3) = (AT, AT + 1, AT + 2);
        assert!(matches!(acted(&db, ANA, pin_at(ANA, true, t1, "p1")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, ANA, pin_at(ANA, false, t2, "p2")), Acted::Queued { .. }));
        let unpinned = chat(&db, ANA);
        arrives(&db, &index, &pin_value(true, t1), 1);
        assert_eq!(chat(&db, ANA), unpinned, "the pin's echo, taken before the unpin, leaves the row as it is");
        assert_eq!(kept(&db, &index), Some(pin_value(true, t1).encode_to_vec()), "and is kept raw");
        let again = acted(&db, ANA, pin_at(ANA, true, t3, "p3"));
        assert!(matches!(again, Acted::Queued { .. }), "pinning again isn't so already: {again:?}");
        let pinned = chat(&db, ANA);
        assert_eq!(pinned.0, t3);
        arrives(&db, &index, &pin_value(false, t2), 2);
        assert_eq!(chat(&db, ANA), pinned, "the unpin's echo changes nothing");
        arrives(&db, &index, &pin_value(true, t3), 3);
        assert_eq!(chat(&db, ANA), pinned, "nor does the third pin's");
        assert_eq!(awaited(&db, &index), None, "which ends the wait");
    }

    #[test]
    fn an_earlier_echo_cant_unpin_a_chat_pinned_again_so_a_fourth_is_still_refused() {
        let db = db();
        let [a, b, c, d] = [
            "351910000041@s.whatsapp.net",
            "351910000042@s.whatsapp.net",
            "351910000043@s.whatsapp.net",
            "351910000044@s.whatsapp.net",
        ];
        chats(&db, &[a, b, c, d]);
        for (n, id) in [a, b, c].into_iter().enumerate() {
            assert!(matches!(acted(&db, id, pin_at(id, true, AT + n as i64, id)), Acted::Queued { .. }), "{id}");
        }
        assert!(matches!(acted(&db, a, pin_at(a, false, AT + 10, "a unpinned")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, a, pin_at(a, true, AT + 11, "a again")), Acted::Queued { .. }));
        let pinned = chat(&db, a);
        let index = ["pin_v1", a];
        arrives(&db, &index, &pin_value(true, AT), 1);
        arrives(&db, &index, &pin_value(false, AT + 10), 2);
        assert_eq!(chat(&db, a), pinned, "its earlier echoes change nothing");
        assert_eq!(acted(&db, d, pin_at(d, true, AT + 12, "d")), Acted::Pinned, "so three are pinned still");
        arrives(&db, &index, &pin_value(true, AT + 11), 3);
        assert_eq!((chat(&db, a), pinned_unarchived(&db)), (pinned, 3), "and stay so once its last echo comes");
    }

    #[test]
    fn what_the_phone_sets_while_jaws_patch_is_on_its_way_is_kept_raw_but_held_back_until_its_echo() {
        let db = db();
        chats(&db, &[ANA]);
        let index = ["mute", ANA];
        let mute = Does::Set(Setting::Mute { until: FOREVER, patch: patch_of(&index, &mute_value(FOREVER, AT), "m1") });
        assert!(matches!(acted(&db, ANA, mute), Acted::Queued { .. }));
        let muted = chat(&db, ANA);
        // The phone unmutes it meanwhile, by a later clock; WhatsApp takes jaw's mute after it all the same.
        let phone = mute_value(0, AT + 5_000);
        arrives(&db, &index, &phone, 1);
        assert_eq!(chat(&db, ANA), muted, "held back, since jaw's mute is what every device ends with");
        assert_eq!(kept(&db, &index), Some(phone.encode_to_vec()), "but kept raw");
        arrives(&db, &index, &mute_value(FOREVER, AT), 2);
        assert_eq!(
            (chat(&db, ANA), awaited(&db, &index)),
            (muted, None),
            "jaw's echo changes nothing and ends the wait"
        );
        arrives(&db, &index, &mute_value(0, AT + 9_000), 3);
        assert_eq!(chat(&db, ANA).2, 0, "the phone's next change is applied as it comes");
    }

    #[test]
    fn once_whatsapp_has_taken_jaws_patch_what_comes_past_where_its_echo_had_to_be_is_applied() {
        let db = db();
        chats(&db, &[ANA]);
        let index = ["pin_v1", ANA];
        let Acted::Queued { outbox, .. } = acted(&db, ANA, pin_at(ANA, true, AT, "p1")) else { panic!("not queued") };
        // WhatsApp answers once the patch's echo is in the inbox, here with a row after it.
        inboxed(&db, &[6, 7]);
        sent(&db, outbox[0]);
        assert_eq!(awaited(&db, &index), Some((AT, outbox[0], Some(7))), "its echo is at 7 or before");
        let pinned = chat(&db, ANA);
        arrives(&db, &index, &pin_value(false, AT - 5), 6);
        assert!(awaited(&db, &index).is_some(), "held back, and the wait goes on");
        arrives(&db, &index, &pin_value(false, AT - 1), 7);
        assert_eq!(chat(&db, ANA), pinned, "at 7 or before, another time came before jaw's patch: held back");
        assert_eq!(awaited(&db, &index), None, "and, the converter past where its echo had to be, the wait is over");
        arrives(&db, &index, &pin_value(false, AT + 1), 8);
        assert_eq!(chat(&db, ANA).0, 0, "past 7, it came after: applied");
    }

    #[test]
    fn what_comes_past_where_an_echo_had_to_be_is_applied_while_a_row_before_it_waits_in_the_inbox() {
        let db = db();
        chats(&db, &[ANA]);
        let index = ["pin_v1", ANA];
        let Acted::Queued { outbox, .. } = acted(&db, ANA, pin_at(ANA, true, AT, "p1")) else { panic!("not queued") };
        inboxed(&db, &[6, 7]);
        sent(&db, outbox[0]);
        let pinned = chat(&db, ANA);
        arrives(&db, &index, &pin_value(false, AT - 5), 6);
        assert_eq!(chat(&db, ANA), pinned, "held back");
        // Row 7, a history notification, stays in the inbox until its blob is in, so the converter
        // goes past it without draining it.
        arrives(&db, &index, &pin_value(false, AT + 1), 8);
        assert_eq!(
            (chat(&db, ANA).0, awaited(&db, &index)),
            (0, None),
            "past 7, it came after: applied, the wait over"
        );
    }

    #[test]
    fn the_converter_passing_where_an_echo_had_to_be_ends_its_wait_but_not_one_still_to_go() {
        let db = db();
        chats(&db, &[ANA, RUI]);
        let (pin, mute, rui) = (["pin_v1", ANA], ["mute", ANA], ["pin_v1", RUI]);
        let Acted::Queued { outbox: pinned, .. } = acted(&db, ANA, pin_at(ANA, true, AT, "p1")) else {
            panic!("not queued")
        };
        let muting =
            Does::Set(Setting::Mute { until: FOREVER, patch: patch_of(&mute, &mute_value(FOREVER, AT + 1), "m1") });
        let Acted::Queued { outbox: muted, .. } = acted(&db, ANA, muting) else { panic!("not queued") };
        assert!(matches!(acted(&db, RUI, pin_at(RUI, true, AT + 2, "r1")), Acted::Queued { .. }));
        inboxed(&db, &[6]);
        sent(&db, pinned[0]);
        inboxed(&db, &[7]);
        sent(&db, muted[0]);
        let echo_by = |index: &[&str]| awaited(&db, index).map(|(.., echo_by)| echo_by);
        assert_eq!((echo_by(&pin), echo_by(&mute), echo_by(&rui)), (Some(Some(6)), Some(Some(7)), Some(None)));
        written(&db, &[Change::Drained(Drained::Inbox(5))]);
        assert_eq!(
            (echo_by(&pin), echo_by(&mute)),
            (Some(Some(6)), Some(Some(7))),
            "before where either echo had to be, both wait"
        );
        written(&db, &[Change::Drained(Drained::Inbox(6))]);
        assert_eq!(
            (echo_by(&pin), echo_by(&mute)),
            (None, Some(Some(7))),
            "at 6, the pin's echo would have ended its wait: it's over"
        );
        written(&db, &[Change::Drained(Drained::Inbox(8))]);
        assert_eq!(echo_by(&mute), None, "past 7, so is the mute's");
        assert_eq!(echo_by(&rui), Some(None), "and Rui's pin, still to go, waits however far the converter has got");
    }

    #[test]
    fn a_patch_failed_for_good_rather_than_refused_still_ends_its_indexs_wait_with_it() {
        let db = db();
        chats(&db, &[ANA]);
        let index = ["pin_v1", ANA];
        let Acted::Queued { outbox, .. } = acted(&db, ANA, pin_at(ANA, true, AT, "p1")) else { panic!("not queued") };
        let failed = |next_at| Step::Failed { outbox: outbox[0], error: "websocket not connected".into(), next_at };
        written(&db, &[Change::Outbox(failed(Some(AT + 1_000)))]);
        assert_eq!(awaited(&db, &index), Some((AT, outbox[0], None)), "one to try again still waits");
        written(&db, &[Change::Outbox(failed(None))]);
        assert_eq!((queued(&db).len(), awaited(&db, &index)), (0, None), "its row gone, unsent, and the wait with it");
        arrives(&db, &index, &pin_value(false, AT - 1), 1);
        assert_eq!(chat(&db, ANA).0, 0, "so what comes is applied");
    }

    #[test]
    fn once_jaws_patch_has_gone_without_going_what_comes_for_its_index_is_applied() {
        let db = db();
        chats(&db, &[ANA]);
        let index = ["pin_v1", ANA];
        let Acted::Queued { outbox, .. } = acted(&db, ANA, pin_at(ANA, true, AT, "p1")) else { panic!("not queued") };
        // No step leaves a wait whose outbox row has gone, so the row is taken out by hand.
        db.execute("DELETE FROM outbox WHERE n = ?1", [outbox[0]]).unwrap();
        arrives(&db, &index, &pin_value(false, AT - 1), 1);
        assert_eq!((chat(&db, ANA).0, awaited(&db, &index)), (0, None), "so what comes is applied, the wait over");
    }

    #[test]
    fn a_refused_patch_is_put_back_only_while_it_is_the_latest_for_its_index() {
        let db = db();
        chats(&db, &[ANA, RUI]);
        let refused = |outbox: i64, chat: &str| {
            let undo = Some(Undo::Pin { chat: chat.into() });
            Change::Outbox(Step::Refused { outbox, error: "server returned error 400".into(), undo })
        };
        let Acted::Queued { outbox, .. } = acted(&db, ANA, pin_at(ANA, true, AT, "a1")) else { panic!("not queued") };
        written(&db, &[refused(outbox[0], ANA)]);
        assert_eq!(
            (chat(&db, ANA).0, awaited(&db, &["pin_v1", ANA])),
            (0, None),
            "the latest: put back, its wait over"
        );
        let Acted::Queued { outbox: first, .. } = acted(&db, RUI, pin_at(RUI, true, AT + 1, "r1")) else {
            panic!("not queued")
        };
        assert!(matches!(acted(&db, RUI, pin_at(RUI, false, AT + 2, "r2")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, RUI, pin_at(RUI, true, AT + 3, "r3")), Acted::Queued { .. }));
        let pinned = chat(&db, RUI);
        written(&db, &[refused(first[0], RUI)]);
        assert_eq!(
            chat(&db, RUI),
            pinned,
            "one a later patch superseded puts nothing back: the later one's intent stands"
        );
        assert_eq!(awaited(&db, &["pin_v1", RUI]).map(|(at, ..)| at), Some(AT + 3), "whose wait goes on");
        let kept: i64 = db.query_row("SELECT count(*) FROM quarantine", [], |r| r.get(0)).unwrap();
        assert_eq!(kept, 2, "and both are kept in quarantine");
    }

    #[test]
    fn a_patch_carrying_the_time_its_index_already_awaits_is_refused() {
        let db = db();
        chats(&db, &[ANA]);
        assert!(matches!(acted(&db, ANA, pin_at(ANA, true, AT, "p1")), Acted::Queued { .. }));
        let error = refused(&db, ANA, pin_at(ANA, false, AT, "p2"));
        assert!(error.to_string().contains("carry one time"), "{error}");
        assert_eq!((chat(&db, ANA).0, queued(&db).len()), (AT, 1), "nothing written or queued");
    }

    /// Each statement a chat action runs, and what the converter and jawd status read of what each
    /// index waits for, as SQLite plans them on a fresh database with no statistics: each reads an
    /// index and sorts nothing. The count is store.count's, of a table with a row an index at most.
    #[test]
    fn every_statement_a_chat_action_runs_reads_an_index_and_sorts_nothing() {
        let db = db();
        let statements = [
            ("a chat's state", CHAT_STATE, "SEARCH chats USING INDEX sqlite_autoindex_chats_1 (id=?)"),
            (
                "chats pinned elsewhere",
                PINNED_ELSEWHERE,
                "SEARCH chats USING INDEX chats_by_sort (archived=? AND sort>?)\nCORRELATED SCALAR SUBQUERY 1\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?)",
            ),
            ("starred", STARRED, "SEARCH messages USING INTEGER PRIMARY KEY (rowid=?)"),
            (
                "shown deleted",
                SHOWN_DELETED,
                "SEARCH t USING INTEGER PRIMARY KEY (rowid=?)\nCORRELATED SCALAR SUBQUERY 1\nSEARCH ra USING INDEX outbox_by_target (target=?)\nCORRELATED SCALAR SUBQUERY 4\nSEARCH ma USING COVERING INDEX chat_merges_by_keep (keep=?)\nCORRELATED SCALAR SUBQUERY 2\nSEARCH mx USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH ga USING INDEX messages_by_id (chat=? AND id=?)\nCORRELATED SCALAR SUBQUERY 3\nSEARCH rb USING INDEX outbox_by_target (target=?)\nCORRELATED SCALAR SUBQUERY 6\nSEARCH mk USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH gk USING INDEX messages_by_id (chat=? AND id=?)\nCORRELATED SCALAR SUBQUERY 5\nSEARCH rc USING INDEX outbox_by_target (target=?)",
            ),
            ("the time an index awaits", AWAITED_AT, "SEARCH app_state_awaiting USING PRIMARY KEY (key=?)"),
            ("an index awaiting a patch", AWAIT, ""),
            (
                "what an index awaits",
                AWAITED,
                "SEARCH a USING PRIMARY KEY (key=?)\nCORRELATED SCALAR SUBQUERY 1\nSEARCH o USING INTEGER PRIMARY KEY (rowid=?)",
            ),
            ("an index's wait over", WAITED, "SEARCH app_state_awaiting USING PRIMARY KEY (key=?)"),
            (
                "the waits the converter has gone past",
                PASSED,
                "SEARCH app_state_awaiting USING INDEX app_state_awaiting_by_echo (echo_by>? AND echo_by<?)",
            ),
            (
                "the indexes awaiting, for jawd status",
                "SELECT count(*) FROM app_state_awaiting",
                "SCAN app_state_awaiting USING COVERING INDEX sqlite_autoindex_app_state_awaiting_2",
            ),
        ];
        for (what, sql, expected) in statements {
            let mut explain = db.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let count = explain.parameter_count();
            let plan: Vec<String> = explain
                .query_map(rusqlite::params_from_iter(std::iter::repeat_n(0, count)), |r| r.get(3))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            assert_eq!(plan.join("\n"), expected, "{what}");
        }
    }
}
