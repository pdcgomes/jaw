//! Chat actions from jaw: a chat pinned, archived, muted, or marked unread or read, and a message
//! starred, from any interface. Each is done to its row at once, with a new version and its event,
//! and the app-state patches that tell your other devices go into the outbox in the same
//! transaction, to go after the chat's earlier sends, as a deletion for you does. What's already so
//! changes nothing and queues nothing, so asking again after a lost answer, or two interfaces asking
//! at once, end the same. A patch WhatsApp refuses for good is undone here (`undo`): what it set goes
//! back to how a chat is when nothing has set it, since fetching its collection again brings only
//! what WhatsApp holds, and the fetch then sets it as the phone has it.

use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension};

use super::outbox::{OUTBOX_MOST, app_state_row, waiting};
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
    /// message, which owes WhatsApp its read receipts, or, with none of its messages here, read
    /// as WhatsApp's echo of the patch reads it, at `at`, the time the patch carries.
    MarkUnread { unread: bool, newest: Option<String>, at: i64, patch: Patch },
}

/// An app-state patch to queue: its outbox row's own ID, and the AppStatePatch, encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub id: String,
    pub raw: Vec<u8>,
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
        Setting::MarkUnread { unread: false, newest, at, patch } => {
            if !chat.marked && chat.unread == 0 {
                return Ok((Acted::Already, false));
            }
            row.marked_unread = Some(false);
            read = Some(match newest {
                Some(id) => Read { chat: jid.to_string(), ids: vec![id.clone()], here: true, ..Read::default() },
                None => Read { chat: jid.to_string(), at: *at, ..Read::default() },
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
    let outbox =
        patches.iter().map(|patch| app_state_row(db, jid, &patch.id, &patch.raw)).collect::<Result<Vec<_>>>()?;
    Ok((Acted::Queued { chat: jid.to_string(), chat_n: chat.n, outbox }, changed))
}

/// Stars message `id` in chat `jid`, or takes its star off, unless it's so already, and queues its
/// patch.
fn star(
    db: &Connection,
    stamps: &mut Stamps,
    jid: &str,
    id: &str,
    starred: bool,
    patch: &Patch,
) -> Result<(Acted, bool)> {
    let Some((chat, n)) = located(db, stamps, jid, id)? else { return Ok((Acted::Gone, false)) };
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
    let outbox = app_state_row(db, jid, &patch.id, &patch.raw)?;
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
    use crate::store::schema;
    use crate::store::{Amend, Change, Message, Person};

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

    fn patch(id: &str) -> Patch {
        Patch { id: id.into(), raw: id.as_bytes().to_vec() }
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
    /// what it came to.
    fn refused(db: &Connection, chat: &str, does: Does) -> anyhow::Error {
        let outcome = Outcome::default();
        let mut stamps = Stamps::load(db).unwrap();
        stamps.begin();
        let act = Change::ChatAction(Act { chat: chat.into(), does, outcome: outcome.clone() });
        let error = super::super::apply(db, &mut stamps, &act).expect_err("a caller's mistake fails");
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
            Does::Set(Setting::MarkUnread { unread, newest: Some("m2".into()), at: AT + 20, patch: patch(id) })
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

    #[test]
    fn marking_read_a_chat_with_only_the_phones_count_reads_it_at_the_patchs_time_as_whatsapps_echo_will() {
        let db = db();
        written(&db, &[Change::Read(Read { chat: ANA.into(), at: AT, phone: Some((3, 0)), ..Read::default() })]);
        let (.., marked, unread, v) = chat(&db, ANA);
        assert_eq!((marked, unread), (false, 3), "the phone's count, with none of its messages here");
        let at = AT + 1_500;
        let read = |id: &str| Does::Set(Setting::MarkUnread { unread: false, newest: None, at, patch: patch(id) });
        assert!(matches!(acted(&db, ANA, read("r1")), Acted::Queued { .. }));
        let (.., unread, read_v) = chat(&db, ANA);
        assert_eq!(unread, 0, "the phone's count cleared at once");
        assert!(read_v > v, "with a new version");
        let event: String = db.query_row("SELECT type FROM events WHERE seq = ?1", [read_v], |r| r.get(0)).unwrap();
        assert_eq!(event, "chat", "and its event");
        let read_at: i64 = db.query_row("SELECT read_at FROM chats WHERE id = ?1", [ANA], |r| r.get(0)).unwrap();
        assert_eq!(read_at, at, "read at the patch's time");
        let echo = action::SyncActionValue {
            timestamp: Some(at),
            mark_chat_as_read_action: Some(action::MarkChatAsReadAction {
                read: Some(true),
                message_range: Some(action::SyncActionMessageRange {
                    last_message_timestamp: Some(at / 1000),
                    ..Default::default()
                }),
            }),
            ..Default::default()
        };
        let index = ["markChatAsRead".to_string(), ANA.to_string()];
        written(&db, &crate::convert::app_state::mutation(&index, &echo.encode_to_vec()).changes);
        assert_eq!(chat(&db, ANA).5, read_v, "WhatsApp's echo of the patch changes nothing");
        assert_eq!(acted(&db, ANA, read("r2")), Acted::Already, "asked again, it's so already");
        assert_eq!(queued(&db).len(), 1, "and no second patch");
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
            Does::Set(Setting::MarkUnread { unread: false, newest, at: AT + 100, patch: patch(id) })
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

    /// Each statement a chat action runs, as SQLite plans it on a fresh database with no statistics:
    /// each reads an index and sorts nothing.
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
