//! What the writer does with events: each person's latest answer to an event, which one older than
//! it never replaces; what WhatsApp seals with an event's own secret, its answers and its edits,
//! opened here with the secret the event's raw protobuf keeps, once jaw has the event, so they're
//! read whichever way they came (live, from history, or kept by an earlier jawd that couldn't);
//! and the answers waiting in `pending` for an event jaw hasn't seen yet, ANSWERS_WAITING of each
//! form at most. How many are going, maybe and not going, and what you answered, aren't kept
//! anywhere: windows count them from these rows as they read the event (`read`).

use anyhow::Result;
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::{Amendment, CHAT, MESSAGE, Stamps, kinds, now_ms, unpack};
use crate::convert::{self, secret, user_jid};

/// The most answers of each form, opened and sealed, and the most sealed edits, kept waiting in
/// `pending` for events jaw hasn't seen; past it the oldest are set aside in quarantine, whole, and
/// put back when their event comes, so none is lost and `pending` stays bounded.
pub const ANSWERS_WAITING: i64 = 10_000;
/// The forms what's done to an event waits in: an answer opened, as history brings it, an answer
/// sealed, as it comes live, and an edit, sealed.
pub const WAITING_KINDS: [&str; 3] = ["answer", "rsvp", "secret"];

const LATEST: &str = "SELECT at, id FROM event_answers WHERE message = ?1 AND who = ?2";
const ANSWERED: &str =
    "INSERT INTO event_answers (message, who, answer, guests, at, id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
    ON CONFLICT (message, who) DO UPDATE SET answer = excluded.answer, guests = excluded.guests, at = excluded.at,
        id = excluded.id";
/// Whether a message was deleted for everyone.
const GONE: &str = "SELECT deleted FROM messages WHERE n = ?1";
/// An event jaw still shows, with what its secret and its author are found by.
const EVENT: &str = "SELECT id, raw, mine, sender FROM messages WHERE n = ?1 AND kind = 'event' AND NOT deleted";
/// Every JID of the person JID ?1 is, itself among them.
const JIDS_OF: &str =
    "SELECT j.jid FROM person_jids s JOIN person_jids j ON j.person = s.person WHERE s.jid = ?1 ORDER BY j.n";
const JIDS_OF_N: &str =
    "SELECT j.jid FROM person_jids s JOIN person_jids j ON j.person = s.person WHERE s.n = ?1 ORDER BY j.n";
const WAITING: &str = "SELECT count(*) FROM pending WHERE kind = ?1";
/// The ?2 oldest of what waits in form ?1, each with what names it, and whole, as JSON.
const OLDEST: &str = "SELECT n, chat, target, sender, effect, raw,
        json_object('chat', chat, 'target', target, 'kind', kind, 'at', at, 'sender', sender, 'mine', mine,
            'effect', effect, 'raw', hex(raw))
    FROM pending WHERE kind = ?1 ORDER BY n LIMIT ?2";
// What waits, set aside under source ?2 as name ?3, kept whole, its pending row as JSONB, to wait
// there for its event; a name held already is left as it is.
const SET_ASIDE: &str = "INSERT INTO quarantine (at, source, ref, error, raw) VALUES (?1, ?2, ?3, ?4, jsonb(?5))
    ON CONFLICT (source, ref) DO NOTHING";
/// What's set aside under source ?1 as name ?2, as JSON.
const HELD: &str = "SELECT json(raw) FROM quarantine WHERE source = ?1 AND ref = ?2";
const SET_ASIDE_DONE: &str = "DELETE FROM pending WHERE n = ?1";
/// Why an answer, or an edit, waits in quarantine.
const ANSWER_ASIDE: &str =
    "more answers waited for events jaw has not seen than it keeps in pending, so this one waits here for its event";
const EDIT_ASIDE: &str =
    "more edits waited for events jaw has not seen than it keeps in pending, so this one waits here for its event";
// What's set aside for events, answers and edits, named from ?1 to ?2, back in pending.
const BACK: &str = "INSERT INTO pending (chat, target, kind, at, raw, sender, mine, effect)
    SELECT raw ->> '$.chat', raw ->> '$.target', raw ->> '$.kind', raw ->> '$.at', unhex(raw ->> '$.raw'),
        raw ->> '$.sender', raw ->> '$.mine', raw ->> '$.effect'
    FROM quarantine WHERE source IN ('answer', 'edit') AND ref >= ?1 AND ref < ?2";
const BACK_DONE: &str = "DELETE FROM quarantine WHERE source IN ('answer', 'edit') AND ref >= ?1 AND ref < ?2";
// The answers and edits set aside for chat ?3, named from ?1 to ?2, named and kept for chat ?4
// instead; one whose new name one set aside for ?4 has already keeps its own row, its n after the
// name.
const SET_ASIDE_MOVED: &str = "UPDATE quarantine SET
        ref = ?4 || substr(ref, length(?3) + 1) || iif(EXISTS (SELECT 1 FROM quarantine q
            WHERE q.source = quarantine.source AND q.ref = ?4 || substr(quarantine.ref, length(?3) + 1)), ' ' || n, ''),
        raw = jsonb_set(raw, '$.chat', ?4)
    WHERE source IN ('answer', 'edit') AND ref >= ?1 AND ref < ?2
    RETURNING raw ->> '$.target'";
// The events people ?1 and ?2 have both answered, by any of their JIDs, with their chats.
const ANSWERED_BY_BOTH: &str = "SELECT g.message, m.chat FROM person_jids gj
    JOIN event_answers g ON g.who = gj.n
    JOIN messages m ON m.n = g.message
    WHERE gj.person = ?2 AND EXISTS (SELECT 1 FROM person_jids kj JOIN event_answers k ON k.who = kj.n
        WHERE kj.person = ?1 AND k.message = g.message)";
const RECOUNTED: &str = "UPDATE messages SET v = ?2 WHERE n = ?1";
const CHAT_SHOWS: &str = "UPDATE chats SET v = ?3 WHERE n = ?1 AND last_message = ?2";

/// Applies `who`'s answer to event `n`, made at `at` as message `id`: it replaces their last only
/// if it's newer, by time and then ID, so an answer arriving again or late changes nothing. Says
/// how many rows of the event changed, 0 or 1.
pub(super) fn answer(db: &Connection, n: i64, who: i64, said: (&str, u32), id: &str, at: i64) -> Result<usize> {
    let latest: Option<(i64, String)> =
        db.prepare_cached(LATEST)?.query_row([n, who], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    if latest.as_ref().is_some_and(|(was, was_id)| (*was, was_id.as_str()) >= (at, id)) {
        return Ok(0);
    }
    let (answer, guests) = said;
    db.prepare_cached(ANSWERED)?.execute(params![n, who, answer, guests, at, id])?;
    Ok(1)
}

/// An answer or an edit kept sealed, as `pending` keeps one: its raw protobuf, who sent it, whether
/// it's yours and when it came.
pub(super) struct Sealed<'a> {
    pub raw: &'a [u8],
    pub sender: Option<&'a str>,
    pub mine: bool,
    pub at: i64,
}

/// An event as its row keeps it: its ID, its raw protobuf, whether it's yours and who sent it.
type Event = (String, Option<Vec<u8>>, bool, Option<i64>);

/// What became of something sealed with an event's secret, once jaw has the message it's for.
pub(super) enum Opening {
    /// What it says, as an amendment, and when it was made.
    Opened(Amendment, i64),
    /// The message was deleted for everyone, so nothing is left for it to change.
    Gone,
    /// It stays sealed, kept for a later jawd.
    Sealed,
}

/// What `sealed`, an answer to event `n` or an edit of it, says, opened with the event's secret,
/// and when it was made. An edit opens only from the event's author, as only they can edit it.
/// It stays sealed when `n` isn't an event, its raw protobuf keeps no secret, or what's sealed
/// doesn't open with it.
pub(super) fn opened(db: &Connection, n: i64, sealed: &Sealed) -> Result<Opening> {
    if db.prepare_cached(GONE)?.query_row([n], |r| r.get::<_, bool>(0))? {
        return Ok(Opening::Gone);
    }
    Ok(match open(db, n, sealed)? {
        Some((what, at)) => Opening::Opened(what, at),
        None => Opening::Sealed,
    })
}

fn open(db: &Connection, n: i64, sealed: &Sealed) -> Result<Option<(Amendment, i64)>> {
    let event: Option<Event> =
        db.prepare_cached(EVENT)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional()?;
    let Some((id, Some(raw), mine, sender)) = event else { return Ok(None) };
    let Some(info) = unpack(&raw).ok().and_then(|plain| WebMessageInfo::decode(plain.as_slice()).ok()) else {
        return Ok(None);
    };
    let Some(key) = secret::of(&info) else { return Ok(None) };
    let Some(change) = unpack(sealed.raw).ok().and_then(|plain| WebMessageInfo::decode(plain.as_slice()).ok()) else {
        return Ok(None);
    };
    let Some(said) = change.message.as_ref().map(convert::unwrap_message) else { return Ok(None) };
    let (use_case, iv, payload) = if let Some(x) = said.enc_event_response_message.as_ref() {
        (secret::EVENT_RESPONSE, x.enc_iv.as_deref(), x.enc_payload.as_deref())
    } else if let Some(x) = said
        .secret_encrypted_message
        .as_ref()
        .filter(|x| x.secret_enc_type == Some(e2e::secret_encrypted_message::SecretEncType::EventEdit as i32))
    {
        (secret::EVENT_EDIT, x.enc_iv.as_deref(), x.enc_payload.as_deref())
    } else {
        return Ok(None);
    };
    let (Some(iv), Some(payload)) = (iv, payload) else { return Ok(None) };
    let me = kinds::account(db)?.jids;
    let authors = match (mine, sender) {
        (true, _) => me.clone(),
        (false, Some(sender)) => jids(db, JIDS_OF_N, sender)?,
        (false, None) => Vec::new(),
    };
    let editing = use_case == secret::EVENT_EDIT;
    let who = match (sealed.mine, sealed.sender) {
        (true, _) => me,
        (false, Some(sender)) => {
            let sender = user_jid(sender);
            let mut known = jids(db, JIDS_OF, sender.as_str())?;
            if !known.contains(&sender) {
                known.insert(0, sender);
            }
            known
        }
        (false, None) => Vec::new(),
    };
    // Only an event's author edits it: the editor must be someone the author is.
    if editing && !who.iter().any(|jid| authors.contains(jid)) {
        return Ok(None);
    }
    let opened = authors.iter().find_map(|author| {
        who.iter().find_map(|who| secret::open(key, use_case, secret::Between { id: &id, author, who }, iv, payload))
    });
    let Some(plain) = opened else { return Ok(None) };
    let own_id = change.key.id.as_deref().unwrap_or_default();
    Ok(if editing {
        e2e::Message::decode(plain.as_slice()).ok().and_then(|m| convert::event_edit(&m)).map(|edit| (edit, sealed.at))
    } else {
        e2e::EventResponseMessage::decode(plain.as_slice())
            .ok()
            .map(|said| convert::answer_of(&said, own_id, sealed.at))
    })
}

/// Every JID of someone, as `query` finds them from `of`.
fn jids<P: rusqlite::ToSql>(db: &Connection, query: &str, of: P) -> Result<Vec<String>> {
    Ok(db.prepare_cached(query)?.query_map([of], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

/// Gives each event people `keep` and `gone` have both answered a new version, as joining `gone`
/// into `keep` makes their answers there one person's and so changes its counts, which windows
/// count as they read it. An event only one of them answered counts the same either way.
pub(super) fn joining(db: &Connection, stamps: &mut Stamps, keep: i64, gone: i64) -> Result<()> {
    let mut events: Vec<(i64, i64)> = db
        .prepare_cached(ANSWERED_BY_BOTH)?
        .query_map([keep, gone], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    events.sort_unstable();
    events.dedup();
    for (n, chat) in events {
        let seq = stamps.seq(MESSAGE, Some(n));
        db.prepare_cached(RECOUNTED)?.execute([n, seq])?;
        stamps.record(db, MESSAGE, n, Some(chat), seq)?;
        let seq = stamps.seq(CHAT, Some(chat));
        if db.prepare_cached(CHAT_SHOWS)?.execute(params![chat, n, seq])? > 0 {
            stamps.record(db, CHAT, chat, Some(chat), seq)?;
        }
    }
    Ok(())
}

/// Puts the answers and edits set aside for event `id` in `chat`, which has just come or moved there
/// with a merge, back in `pending`, to be applied with the rest waiting for it.
pub(super) fn set_aside_back(db: &Connection, chat: &str, id: &str) -> Result<()> {
    let (from, to) = (format!("{chat} {id} "), format!("{chat} {id}!"));
    if db.prepare_cached(BACK)?.execute([&from, &to])? > 0 {
        db.prepare_cached(BACK_DONE)?.execute([&from, &to])?;
    }
    Ok(())
}

/// Has the answers and edits set aside for events in chat `gone` wait for them in chat `keep`
/// instead, as what waits in `pending` does once what's addressed to `gone` goes to `keep`, so they
/// come back with their event there. Says the events they're for, by ID.
pub(super) fn set_aside_moved(db: &Connection, keep: &str, gone: &str) -> Result<Vec<String>> {
    let (from, to) = (format!("{gone} "), format!("{gone}!"));
    Ok(db
        .prepare_cached(SET_ASIDE_MOVED)?
        .query_map([&from, &to, gone, keep], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

/// Keeps what waits in `kind`'s form, one of WAITING_KINDS, within ANSWERS_WAITING, setting the
/// oldest past it aside: answers as `answer`, edits as `edit`.
pub(super) fn bounded(db: &Connection, kind: &str) -> Result<()> {
    let waiting: i64 = db.prepare_cached(WAITING)?.query_row([kind], |r| r.get(0))?;
    if waiting > ANSWERS_WAITING {
        let aside = if kind == "secret" { ("edit", EDIT_ASIDE) } else { ("answer", ANSWER_ASIDE) };
        set_aside(db, kind, waiting - ANSWERS_WAITING, aside)?;
    }
    Ok(())
}

/// A row of `pending` to set aside: its n, chat, target, sender, effect and raw protobuf, and all
/// of it as JSON.
type Aside = (i64, String, String, Option<String>, String, Vec<u8>, String);

/// Sets the `over` oldest of what waits in `kind`'s form aside in quarantine, under `source` and
/// saying `why`, each named by its event, then who sent it and its own ID, its message's, which an
/// opened answer keeps in what it does and what's sealed in its raw protobuf, or else by what it
/// holds, hashed: so the same one set aside again is the same row, and two are never one. A name
/// another row holds already takes what this one holds, hashed, after it. Each leaves `pending`
/// only once it's kept there, or a copy of it the same in every field is, so nothing goes that
/// isn't kept: a name pending's n gave would be given again once pending emptied.
fn set_aside(db: &Connection, kind: &str, over: i64, (source, why): (&str, &str)) -> Result<()> {
    let oldest: Vec<Aside> = db
        .prepare_cached(OLDEST)?
        .query_map(params![kind, over], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    for (n, chat, target, sender, effect, raw, whole) in oldest {
        let hashed = format!("#{}", digest(&whole));
        let own = serde_json::from_str::<serde_json::Value>(&effect)
            .ok()
            .and_then(|effect| effect["id"].as_str().map(str::to_string))
            .filter(|id| !id.is_empty())
            .or_else(|| kinds::own_id(&raw));
        let named = format!("{chat} {target} {} {}", sender.unwrap_or_default(), own.as_deref().unwrap_or(&hashed));
        let mut names = vec![named.clone()];
        if own.is_some() {
            names.push(format!("{named} {hashed}"));
        }
        let mut kept = false;
        for name in &names {
            if db.prepare_cached(SET_ASIDE)?.execute(params![now_ms(), source, name, why, whole])? > 0 {
                kept = true;
                break;
            }
            let held: Option<String> =
                db.prepare_cached(HELD)?.query_row(params![source, name], |r| r.get(0)).optional()?;
            if held.is_some_and(|held| same_json(&held, &whole)) {
                kept = true;
                break;
            }
        }
        if kept {
            db.prepare_cached(SET_ASIDE_DONE)?.execute([n])?;
        }
    }
    Ok(())
}

/// The first eight bytes of `text`'s SHA-256, in hex.
fn digest(text: &str) -> String {
    Sha256::digest(text.as_bytes())[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether two JSON texts say the same.
fn same_json(a: &str, b: &str) -> bool {
    matches!((serde_json::from_str::<serde_json::Value>(a), serde_json::from_str::<serde_json::Value>(b)),
        (Ok(a), Ok(b)) if a == b)
}

#[cfg(test)]
mod tests;
