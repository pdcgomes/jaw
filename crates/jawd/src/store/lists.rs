//! What interfaces list besides a chat's messages: each folder's chats, the folder tree with its
//! counts, the favourite slots, and rosters. Every list is read by keyset on an index of its own,
//! in the order its sort says, so a page costs the same at any depth and nothing is sorted in a
//! temporary b-tree: All and Archived on chats_by_sort, Unread, Groups and Communities on partial
//! indexes of their chats, a folder of yours on folder_chats_by_sort, and a smart folder on the
//! partial index its rule's most selective field has, if it has one, with the rest of its rule
//! checked row by row. Rosters are read the same way, on people_roster or folder_people_by_sort.

pub mod communities;

use std::collections::HashSet;

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::read::{CHAT_JOINS, CHAT_SELECT, ChatCursor, ChatRow, chat_row, shown};

/// A list of chats: a built-in folder, a smart one, or one of yours, each by its n.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum List {
    All,
    Archived,
    Unread,
    Groups,
    Communities,
    Folder(i64),
    Smart(i64),
}

impl List {
    /// The list a folder's ID on the wire names.
    pub fn parse(id: &str) -> Option<List> {
        Some(match id {
            "all" => List::All,
            "archived" => List::Archived,
            "unread" => List::Unread,
            "groups" => List::Groups,
            "communities" => List::Communities,
            _ => match id.split_once(':') {
                Some(("folder", n)) => List::Folder(n.parse().ok().filter(|n| *n > 0)?),
                Some(("smart", n)) => List::Smart(n.parse().ok().filter(|n| *n > 0)?),
                _ => return None,
            },
        })
    }

    /// The built-in folder's name in jaw.db, for those that are one.
    fn builtin(&self) -> Option<&'static str> {
        Some(match self {
            List::All => "all",
            List::Archived => "archived",
            List::Unread => "unread",
            List::Groups => "groups",
            List::Communities => "communities",
            List::Folder(_) | List::Smart(_) => return None,
        })
    }
}

/// A folder's ID on the wire: a built-in folder's own, `smart:` or `folder:` and its n.
pub fn folder_id(n: i64, builtin: Option<&str>, smart: bool) -> String {
    match (builtin, smart) {
        (Some(builtin), _) => builtin.to_string(),
        (None, true) => format!("smart:{n}"),
        (None, false) => format!("folder:{n}"),
    }
}

/// A smart folder's rule: every field present must hold.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Unread messages, or marked unread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unread: Option<bool>,
    /// Unread messages mentioning you.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mentions: Option<bool>,
    /// `direct`, `group` (with announcement groups) or `community` (with their groups).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// A community's ID: it and its groups.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    /// Seconds: in a one-to-one chat, their message is the newest and came at least this long ago.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting: Option<i64>,
    /// In a favourite slot, or in a folder of yours that is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favourites: Option<bool>,
}

/// The longest wait a rule can ask for, ten years.
const WAITING_MOST: i64 = 10 * 366 * 24 * 3600;

impl Rule {
    /// The rule `value` says, or why it isn't one.
    pub fn of(value: &Value) -> Result<Rule> {
        let rule: Rule = serde_json::from_value(value.clone())?;
        for (name, flag) in [
            ("unread", rule.unread),
            ("mentions", rule.mentions),
            ("muted", rule.muted),
            ("favourites", rule.favourites),
        ] {
            if flag == Some(false) {
                bail!("a rule's {name} can only be true; leave it out to not ask for it");
            }
        }
        if let Some(kind) = &rule.kind
            && !matches!(kind.as_str(), "direct" | "group" | "community")
        {
            bail!("a rule's kind is direct, group or community, not {kind}");
        }
        if let Some(waiting) = rule.waiting
            && !(1..=WAITING_MOST).contains(&waiting)
        {
            bail!("a rule's waiting is a number of seconds, more than none and at most ten years");
        }
        if rule == Rule::default() {
            bail!(
                "a smart folder's rule asks for something: unread, mentions, kind, community, muted, waiting or favourites"
            );
        }
        Ok(rule)
    }

    fn parse(text: &str) -> Rule {
        serde_json::from_str::<Value>(text).ok().and_then(|value| Rule::of(&value).ok()).unwrap_or_default()
    }

    /// Its fields as RULED binds them, as ?5 to ?12, `now` the last.
    fn bound(&self, now: i64) -> [rusqlite::types::Value; 8] {
        use rusqlite::types::Value as V;
        let flag = |f: Option<bool>| V::Integer(i64::from(f.unwrap_or(false)));
        let text = |t: &Option<String>| t.clone().map_or(V::Null, V::Text);
        [
            flag(self.unread),
            flag(self.mentions),
            text(&self.kind),
            text(&self.community),
            flag(self.muted),
            self.waiting.map_or(V::Null, V::Integer),
            flag(self.favourites),
            V::Integer(now),
        ]
    }

    /// Which partial index drives it: mentions and unread are the selective fields.
    fn drive(&self) -> Drive {
        if self.mentions == Some(true) {
            Drive::Mentioned
        } else if self.unread == Some(true) {
            Drive::Unread
        } else {
            Drive::Sorted
        }
    }
}

#[derive(Clone, Copy)]
enum Drive {
    Sorted,
    Unread,
    Mentioned,
}

/// A smart folder's rule, with its fields bound as ?5 to ?12: unread, mentions, kind, community,
/// muted, waiting in seconds, favourites, and now in milliseconds.
const RULED: &str = "
    AND (?5 = 0 OR c.unread > 0 OR c.marked_unread <> 0)
    AND (?6 = 0 OR c.mentions > 0)
    AND (?7 IS NULL OR iif(?7 = 'direct', c.kind = 'direct', iif(?7 = 'group', c.kind IN ('group', 'announcement'),
        c.kind = 'community' OR c.community IS NOT NULL)))
    AND (?8 IS NULL OR c.community = ?8 OR c.id = ?8)
    AND (?9 = 0 OR c.muted_until > ?12)
    AND (?10 IS NULL OR c.kind = 'direct' AND c.last_at <= ?12 - ?10 * 1000 AND EXISTS (SELECT 1 FROM messages lm
        WHERE lm.n = c.last_message AND lm.mine = 0 AND lm.kind NOT IN ('system', 'call')))
    AND (?11 = 0 OR EXISTS (SELECT 1 FROM favourites s WHERE s.chat = c.n)
        OR EXISTS (SELECT 1 FROM folder_chats fc JOIN favourites s ON s.folder = fc.folder WHERE fc.chat = c.n))";

/// The most chats a smart folder counts; a count of this many means at least this many.
pub const SMART_COUNTED: i64 = 1_000;
/// The most groups of a community a list brings with it.
const GROUPS_MOST: i64 = 1_000;

/// The smart folder `n`'s rule, if it is one.
fn rule_of(db: &Connection, n: i64) -> Result<Option<Rule>> {
    let rule: Option<String> = db
        .prepare_cached("SELECT rule FROM folders WHERE n = ?1 AND rule IS NOT NULL")?
        .query_row([n], |r| r.get(0))
        .optional()?;
    Ok(rule.map(|rule| Rule::parse(&rule)))
}

/// Whether the folder a list names is there.
pub fn exists(db: &Connection, list: &List) -> Result<bool> {
    Ok(match list {
        List::Folder(n) => {
            db.prepare_cached("SELECT 1 FROM folders WHERE n = ?1 AND builtin IS NULL AND rule IS NULL")?.exists([n])?
        }
        List::Smart(n) => rule_of(db, *n)?.is_some(),
        _ => true,
    })
}

fn rows(db: &Connection, sql: &str, params: impl rusqlite::Params) -> Result<Vec<ChatRow>> {
    Ok(db.prepare_cached(sql)?.query_map(params, chat_row)?.collect::<rusqlite::Result<_>>()?)
}

/// Up to `limit` chats of `list` after `after`, in the list's order, from the top without one.
pub fn chats(db: &Connection, list: &List, after: Option<&ChatCursor>, limit: usize, now: i64) -> Result<Vec<ChatRow>> {
    let (sort, n) = after.map_or(("2", i64::MAX), |after| (after.sort.as_str(), after.n));
    let limit = limit as i64;
    match list {
        List::All | List::Archived => {
            let sql = format!(
                "{CHAT_SELECT} FROM chats c {CHAT_JOINS} WHERE c.archived = ?1 AND (c.sort, c.n) < (?2, ?3)
                ORDER BY c.sort DESC, c.n DESC LIMIT ?4"
            );
            rows(db, &sql, params![*list == List::Archived, sort, n, limit])
        }
        List::Unread => {
            let sql = format!(
                "{CHAT_SELECT} FROM chats c INDEXED BY chats_unread {CHAT_JOINS}
                WHERE c.archived = 0 AND (c.unread > 0 OR c.marked_unread <> 0) AND (c.sort, c.n) < (?1, ?2)
                ORDER BY c.sort DESC, c.n DESC LIMIT ?3"
            );
            rows(db, &sql, params![sort, n, limit])
        }
        List::Groups => {
            let sql = format!(
                "{CHAT_SELECT} FROM chats c INDEXED BY chats_groups {CHAT_JOINS}
                WHERE c.archived = 0 AND c.kind IN ('group', 'announcement') AND (c.sort, c.n) < (?1, ?2)
                ORDER BY c.sort DESC, c.n DESC LIMIT ?3"
            );
            rows(db, &sql, params![sort, n, limit])
        }
        List::Communities => {
            let sql = format!(
                "{CHAT_SELECT} FROM chats c INDEXED BY chats_communities {CHAT_JOINS}
                WHERE c.archived = 0 AND (c.kind = 'community' OR c.community IS NOT NULL) AND (c.sort, c.n) < (?1, ?2)
                ORDER BY c.sort DESC, c.n DESC LIMIT ?3"
            );
            rows(db, &sql, params![sort, n, limit])
        }
        List::Folder(folder) => {
            let sql = format!(
                "{CHAT_SELECT} FROM folder_chats fc CROSS JOIN chats c ON c.n = fc.chat {CHAT_JOINS}
                WHERE fc.folder = ?1 AND (fc.sort, fc.chat) < (?2, ?3) ORDER BY fc.sort DESC, fc.chat DESC LIMIT ?4"
            );
            rows(db, &sql, params![folder, sort, n, limit])
        }
        List::Smart(folder) => {
            let Some(rule) = rule_of(db, *folder)? else { return Ok(Vec::new()) };
            let sql = match rule.drive() {
                Drive::Sorted => format!(
                    "{CHAT_SELECT} FROM chats c {CHAT_JOINS} WHERE c.archived = 0 AND (c.sort, c.n) < (?2, ?3) {RULED}
                    ORDER BY c.sort DESC, c.n DESC LIMIT ?4"
                ),
                Drive::Unread => format!(
                    "{CHAT_SELECT} FROM chats c INDEXED BY chats_unread {CHAT_JOINS}
                    WHERE c.archived = 0 AND (c.unread > 0 OR c.marked_unread <> 0) AND (c.sort, c.n) < (?2, ?3) {RULED}
                    ORDER BY c.sort DESC, c.n DESC LIMIT ?4"
                ),
                Drive::Mentioned => format!(
                    "{CHAT_SELECT} FROM chats c INDEXED BY chats_mentioned {CHAT_JOINS}
                    WHERE c.archived = 0 AND c.mentions > 0 AND (c.sort, c.n) < (?2, ?3) {RULED}
                    ORDER BY c.sort DESC, c.n DESC LIMIT ?4"
                ),
            };
            let mut bound: Vec<rusqlite::types::Value> =
                vec![rusqlite::types::Value::Null, sort.to_string().into(), n.into(), limit.into()];
            bound.extend(rule.bound(now));
            rows(db, &sql, rusqlite::params_from_iter(bound))
        }
    }
}

/// Up to `limit` chats of `list` just above `cursor`, in the list's order, so the nearest is last.
pub fn chats_above(db: &Connection, list: &List, cursor: &ChatCursor, limit: usize, now: i64) -> Result<Vec<ChatRow>> {
    let (sort, n, limit) = (cursor.sort.as_str(), cursor.n, limit as i64);
    let mut rows = match list {
        List::All | List::Archived => {
            let sql = format!(
                "{CHAT_SELECT} FROM chats c {CHAT_JOINS} WHERE c.archived = ?1 AND (c.sort, c.n) > (?2, ?3)
                ORDER BY c.sort, c.n LIMIT ?4"
            );
            rows(db, &sql, params![*list == List::Archived, sort, n, limit])?
        }
        List::Unread => {
            let sql = format!(
                "{CHAT_SELECT} FROM chats c INDEXED BY chats_unread {CHAT_JOINS}
                WHERE c.archived = 0 AND (c.unread > 0 OR c.marked_unread <> 0) AND (c.sort, c.n) > (?1, ?2)
                ORDER BY c.sort, c.n LIMIT ?3"
            );
            rows(db, &sql, params![sort, n, limit])?
        }
        List::Groups => {
            let sql = format!(
                "{CHAT_SELECT} FROM chats c INDEXED BY chats_groups {CHAT_JOINS}
                WHERE c.archived = 0 AND c.kind IN ('group', 'announcement') AND (c.sort, c.n) > (?1, ?2)
                ORDER BY c.sort, c.n LIMIT ?3"
            );
            rows(db, &sql, params![sort, n, limit])?
        }
        List::Communities => {
            let sql = format!(
                "{CHAT_SELECT} FROM chats c INDEXED BY chats_communities {CHAT_JOINS}
                WHERE c.archived = 0 AND (c.kind = 'community' OR c.community IS NOT NULL) AND (c.sort, c.n) > (?1, ?2)
                ORDER BY c.sort, c.n LIMIT ?3"
            );
            rows(db, &sql, params![sort, n, limit])?
        }
        List::Folder(folder) => {
            let sql = format!(
                "{CHAT_SELECT} FROM folder_chats fc CROSS JOIN chats c ON c.n = fc.chat {CHAT_JOINS}
                WHERE fc.folder = ?1 AND (fc.sort, fc.chat) > (?2, ?3) ORDER BY fc.sort, fc.chat LIMIT ?4"
            );
            rows(db, &sql, params![folder, sort, n, limit])?
        }
        List::Smart(folder) => {
            let Some(rule) = rule_of(db, *folder)? else { return Ok(Vec::new()) };
            let sql = match rule.drive() {
                Drive::Sorted => format!(
                    "{CHAT_SELECT} FROM chats c {CHAT_JOINS} WHERE c.archived = 0 AND (c.sort, c.n) > (?2, ?3) {RULED}
                    ORDER BY c.sort, c.n LIMIT ?4"
                ),
                Drive::Unread => format!(
                    "{CHAT_SELECT} FROM chats c INDEXED BY chats_unread {CHAT_JOINS}
                    WHERE c.archived = 0 AND (c.unread > 0 OR c.marked_unread <> 0) AND (c.sort, c.n) > (?2, ?3) {RULED}
                    ORDER BY c.sort, c.n LIMIT ?4"
                ),
                Drive::Mentioned => format!(
                    "{CHAT_SELECT} FROM chats c INDEXED BY chats_mentioned {CHAT_JOINS}
                    WHERE c.archived = 0 AND c.mentions > 0 AND (c.sort, c.n) > (?2, ?3) {RULED}
                    ORDER BY c.sort, c.n LIMIT ?4"
                ),
            };
            let mut bound: Vec<rusqlite::types::Value> =
                vec![rusqlite::types::Value::Null, sort.to_string().into(), n.into(), limit.into()];
            bound.extend(rule.bound(now));
            rows(db, &sql, rusqlite::params_from_iter(bound))?
        }
    };
    rows.reverse();
    Ok(rows)
}

/// A smart folder's chats counted, up to SMART_COUNTED, with their unread messages and mentions.
fn smart_counts(db: &Connection, rule: &Rule, now: i64) -> Result<(i64, i64, i64)> {
    let sql = match rule.drive() {
        Drive::Sorted => format!(
            "SELECT count(*), coalesce(sum(unread), 0), coalesce(sum(mentions), 0) FROM
                (SELECT c.unread, c.mentions FROM chats c WHERE c.archived = 0 {RULED} LIMIT ?4)"
        ),
        Drive::Unread => format!(
            "SELECT count(*), coalesce(sum(unread), 0), coalesce(sum(mentions), 0) FROM
                (SELECT c.unread, c.mentions FROM chats c INDEXED BY chats_unread
                WHERE c.archived = 0 AND (c.unread > 0 OR c.marked_unread <> 0) {RULED} LIMIT ?4)"
        ),
        Drive::Mentioned => format!(
            "SELECT count(*), coalesce(sum(unread), 0), coalesce(sum(mentions), 0) FROM
                (SELECT c.unread, c.mentions FROM chats c INDEXED BY chats_mentioned
                WHERE c.archived = 0 AND c.mentions > 0 {RULED} LIMIT ?4)"
        ),
    };
    use rusqlite::types::Value as V;
    let mut bound = vec![V::Null, V::Null, V::Null, V::Integer(SMART_COUNTED)];
    bound.extend(rule.bound(now));
    Ok(db
        .prepare_cached(&sql)?
        .query_row(rusqlite::params_from_iter(bound), |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?)
}

/// How many chats `list` holds: as its counts keep it, or for a smart folder counted up to
/// SMART_COUNTED.
pub fn chat_count(db: &Connection, list: &List, now: i64) -> Result<u64> {
    let count: Option<i64> = match list {
        List::Folder(n) => db
            .prepare_cached("SELECT chats FROM folder_counts WHERE folder = ?1")?
            .query_row([n], |r| r.get(0))
            .optional()?,
        List::Smart(n) => match rule_of(db, *n)? {
            Some(rule) => Some(smart_counts(db, &rule, now)?.0),
            None => None,
        },
        builtin => db
            .prepare_cached(
                "SELECT k.chats FROM folders f JOIN folder_counts k ON k.folder = f.n WHERE f.builtin = ?1",
            )?
            .query_row([builtin.builtin()], |r| r.get(0))
            .optional()?,
    };
    Ok(count.unwrap_or(0).max(0) as u64)
}

/// How many rows `list` has: its chats, and each community it holds only for its groups.
pub fn row_count(db: &Connection, list: &List, now: i64) -> Result<u64> {
    Ok(chat_count(db, list, now)? + brought(db, list, now)?)
}

/// How many communities `list` holds only for their groups, the rows it has besides its chats:
/// counted from the communities jaw has, tens at most, each looking up its groups.
fn brought(db: &Connection, list: &List, now: i64) -> Result<u64> {
    const KEPT: &str = "SELECT count(*) FROM chats k INDEXED BY chats_known_communities
        WHERE k.kind = 'community' AND k.archived <> ?1
        AND EXISTS (SELECT 1 FROM chats c WHERE c.community = k.id AND c.archived = ?1)";
    const UNREAD: &str = "SELECT count(*) FROM chats k INDEXED BY chats_known_communities
        WHERE k.kind = 'community' AND NOT (k.archived = 0 AND (k.unread > 0 OR k.marked_unread <> 0))
        AND EXISTS (SELECT 1 FROM chats c WHERE c.community = k.id AND c.archived = 0
            AND (c.unread > 0 OR c.marked_unread <> 0))";
    const GROUPS: &str = "SELECT count(*) FROM chats k INDEXED BY chats_known_communities
        WHERE k.kind = 'community'
        AND EXISTS (SELECT 1 FROM chats c WHERE c.community = k.id AND c.archived = 0
            AND c.kind IN ('group', 'announcement'))";
    const FILED: &str = "SELECT count(*) FROM chats k INDEXED BY chats_known_communities
        WHERE k.kind = 'community' AND NOT EXISTS (SELECT 1 FROM folder_chats f WHERE f.folder = ?1 AND f.chat = k.n)
        AND EXISTS (SELECT 1 FROM chats c JOIN folder_chats f ON f.folder = ?1 AND f.chat = c.n WHERE c.community = k.id)";
    let count: i64 = match list {
        List::All | List::Archived | List::Communities => {
            db.prepare_cached(KEPT)?.query_row([*list == List::Archived], |r| r.get(0))?
        }
        List::Unread => db.prepare_cached(UNREAD)?.query_row([], |r| r.get(0))?,
        List::Groups => db.prepare_cached(GROUPS)?.query_row([], |r| r.get(0))?,
        List::Folder(folder) => db.prepare_cached(FILED)?.query_row([folder], |r| r.get(0))?,
        List::Smart(folder) => {
            let Some(rule) = rule_of(db, *folder)? else { return Ok(0) };
            let sql = format!(
                "SELECT count(*) FROM chats k INDEXED BY chats_known_communities
                WHERE k.kind = 'community' AND NOT EXISTS (SELECT 1 FROM chats c WHERE c.n = k.n AND c.archived = 0 {RULED})
                AND EXISTS (SELECT 1 FROM chats c WHERE c.community = k.id AND c.archived = 0 {RULED})"
            );
            use rusqlite::types::Value as V;
            let mut bound = vec![V::Null, V::Null, V::Null, V::Null];
            bound.extend(rule.bound(now));
            db.prepare_cached(&sql)?.query_row(rusqlite::params_from_iter(bound), |r| r.get(0))?
        }
    };
    Ok(count.max(0) as u64)
}

/// The groups of community `community` (a JID) that `list` holds, which come with it in a window.
pub fn community_groups(db: &Connection, list: &List, community: &str, now: i64) -> Result<Vec<ChatRow>> {
    match list {
        List::All | List::Archived => {
            let sql = format!(
                "{CHAT_SELECT} FROM chats c {CHAT_JOINS} WHERE c.community = ?2 AND c.archived = ?1
                ORDER BY c.last_at DESC LIMIT ?3"
            );
            rows(db, &sql, params![*list == List::Archived, community, GROUPS_MOST])
        }
        List::Unread => {
            let sql = format!(
                "{CHAT_SELECT} FROM chats c {CHAT_JOINS}
                WHERE c.community = ?1 AND c.archived = 0 AND (c.unread > 0 OR c.marked_unread <> 0)
                ORDER BY c.last_at DESC LIMIT ?2"
            );
            rows(db, &sql, params![community, GROUPS_MOST])
        }
        List::Groups => {
            let sql = format!(
                "{CHAT_SELECT} FROM chats c {CHAT_JOINS}
                WHERE c.community = ?1 AND c.archived = 0 AND c.kind IN ('group', 'announcement')
                ORDER BY c.last_at DESC LIMIT ?2"
            );
            rows(db, &sql, params![community, GROUPS_MOST])
        }
        List::Communities => {
            let sql = format!(
                "{CHAT_SELECT} FROM chats c {CHAT_JOINS} WHERE c.community = ?1 AND c.archived = 0
                ORDER BY c.last_at DESC LIMIT ?2"
            );
            rows(db, &sql, params![community, GROUPS_MOST])
        }
        List::Folder(folder) => {
            let sql = format!(
                "{CHAT_SELECT} FROM chats c {CHAT_JOINS}
                WHERE c.community = ?2 AND EXISTS (SELECT 1 FROM folder_chats f WHERE f.folder = ?1 AND f.chat = c.n)
                ORDER BY c.last_at DESC LIMIT ?3"
            );
            rows(db, &sql, params![folder, community, GROUPS_MOST])
        }
        List::Smart(folder) => {
            let Some(rule) = rule_of(db, *folder)? else { return Ok(Vec::new()) };
            let sql = format!(
                "{CHAT_SELECT} FROM chats c {CHAT_JOINS} WHERE c.community = ?2 AND c.archived = 0 {RULED}
                ORDER BY c.last_at DESC LIMIT ?3"
            );
            use rusqlite::types::Value as V;
            let mut bound = vec![V::Null, V::Text(community.to_string()), V::Integer(GROUPS_MOST), V::Null];
            bound.extend(rule.bound(now));
            rows(db, &sql, rusqlite::params_from_iter(bound))
        }
    }
}

/// What a window needs of a group of a community it shows as its one row: where the group sorts,
/// its unread messages and mentions, and when its newest message came.
#[derive(Debug)]
pub struct Member {
    pub n: i64,
    pub cursor: ChatCursor,
    pub unread: i64,
    pub mentions: i64,
    pub last_at: i64,
}

/// What community_members reads of a group `c`.
const MEMBER: &str = "SELECT c.n, c.sort, c.unread, c.mentions, c.last_at";

fn members(db: &Connection, sql: &str, params: impl rusqlite::Params) -> Result<Vec<Member>> {
    Ok(db
        .prepare_cached(sql)?
        .query_map(params, |r| {
            Ok(Member {
                n: r.get(0)?,
                cursor: ChatCursor { sort: r.get(1)?, n: r.get(0)? },
                unread: r.get(2)?,
                mentions: r.get(3)?,
                last_at: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

/// What a window needs of the groups of community `community` that `list` holds, for one it shows
/// as its one row: as community_groups finds them, without what their own rows would show.
pub fn community_members(db: &Connection, list: &List, community: &str, now: i64) -> Result<Vec<Member>> {
    match list {
        List::All | List::Archived => {
            let sql = format!("{MEMBER} FROM chats c WHERE c.community = ?2 AND c.archived = ?1 LIMIT ?3");
            members(db, &sql, params![*list == List::Archived, community, GROUPS_MOST])
        }
        List::Unread => {
            let sql = format!(
                "{MEMBER} FROM chats c
                WHERE c.community = ?1 AND c.archived = 0 AND (c.unread > 0 OR c.marked_unread <> 0) LIMIT ?2"
            );
            members(db, &sql, params![community, GROUPS_MOST])
        }
        List::Groups => {
            let sql = format!(
                "{MEMBER} FROM chats c
                WHERE c.community = ?1 AND c.archived = 0 AND c.kind IN ('group', 'announcement') LIMIT ?2"
            );
            members(db, &sql, params![community, GROUPS_MOST])
        }
        List::Communities => {
            let sql = format!("{MEMBER} FROM chats c WHERE c.community = ?1 AND c.archived = 0 LIMIT ?2");
            members(db, &sql, params![community, GROUPS_MOST])
        }
        List::Folder(folder) => {
            let sql = format!(
                "{MEMBER} FROM chats c
                WHERE c.community = ?2 AND EXISTS (SELECT 1 FROM folder_chats f WHERE f.folder = ?1 AND f.chat = c.n)
                LIMIT ?3"
            );
            members(db, &sql, params![folder, community, GROUPS_MOST])
        }
        List::Smart(folder) => {
            let Some(rule) = rule_of(db, *folder)? else { return Ok(Vec::new()) };
            let sql = format!("{MEMBER} FROM chats c WHERE c.community = ?2 AND c.archived = 0 {RULED} LIMIT ?3");
            use rusqlite::types::Value as V;
            let mut bound = vec![V::Null, V::Text(community.to_string()), V::Integer(GROUPS_MOST), V::Null];
            bound.extend(rule.bound(now));
            members(db, &sql, rusqlite::params_from_iter(bound))
        }
    }
}

/// Which of the chats `ns` (as they are in `found`) `list` holds now.
pub fn held(db: &Connection, list: &List, found: &[ChatRow], now: i64) -> Result<HashSet<i64>> {
    let ns: Vec<i64> = found.iter().map(|row| row.n).collect();
    let json = serde_json::to_string(&ns)?;
    Ok(match list {
        List::Folder(folder) => db
            .prepare_cached(
                "SELECT chat FROM folder_chats WHERE folder = ?1 AND chat IN (SELECT value FROM json_each(?2))",
            )?
            .query_map(params![folder, json], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?,
        List::Smart(folder) => {
            let Some(rule) = rule_of(db, *folder)? else { return Ok(HashSet::new()) };
            let sql = format!(
                "SELECT c.n FROM chats c WHERE c.n IN (SELECT value FROM json_each(?1)) AND c.archived = 0 {RULED}"
            );
            use rusqlite::types::Value as V;
            let mut bound = vec![V::Text(json), V::Null, V::Null, V::Null];
            bound.extend(rule.bound(now));
            db.prepare_cached(&sql)?
                .query_map(rusqlite::params_from_iter(bound), |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        }
        builtin => found.iter().filter(|row| holds(builtin, row)).map(|row| row.n).collect(),
    })
}

/// Whether a built-in folder holds a chat, as its counts' triggers and its index say.
fn holds(list: &List, row: &ChatRow) -> bool {
    let kept = !row.archived;
    match list {
        List::All => kept,
        List::Archived => row.archived,
        List::Unread => kept && (row.unread > 0 || row.marked_unread),
        List::Groups => kept && matches!(row.kind.as_str(), "group" | "announcement"),
        List::Communities => kept && (row.kind == "community" || row.community.is_some()),
        List::Folder(_) | List::Smart(_) => false,
    }
}

/// A folder in the tree, with its counts.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FolderRow {
    #[serde(skip)]
    pub n: i64,
    pub id: String,
    pub v: i64,
    pub parent: Option<String>,
    pub name: String,
    pub position: i64,
    pub smart: Option<Value>,
    pub unread: i64,
    pub mentions: i64,
    pub count: i64,
    pub favourite: Option<i64>,
}

/// Every folder, built-in, smart and yours, each with its counts: kept for the built-in ones and
/// yours, counted now for smart ones, up to SMART_COUNTED.
pub fn folders(db: &Connection, now: i64) -> Result<Vec<FolderRow>> {
    type Found =
        (i64, Option<String>, Option<i64>, String, i64, Option<String>, i64, Option<(i64, i64, i64)>, Option<i64>);
    let found: Vec<Found> = db
        .prepare_cached(
            "SELECT f.n, f.builtin, f.parent, f.name, f.position, f.rule, f.v, k.unread, k.mentions, k.chats + k.lone,
                s.slot
            FROM folders f LEFT JOIN folder_counts k ON k.folder = f.n LEFT JOIN favourites s ON s.folder = f.n",
        )?
        .query_map([], |r| {
            let counts = match r.get::<_, Option<i64>>(7)? {
                Some(unread) => Some((unread, r.get(8)?, r.get(9)?)),
                None => None,
            };
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, counts, r.get(10)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut rows = Vec::with_capacity(found.len());
    for (n, builtin, parent, name, position, rule, v, counts, favourite) in found {
        let (unread, mentions, count) = match (&rule, counts) {
            (Some(rule), _) => {
                let (count, unread, mentions) = smart_counts(db, &Rule::parse(rule), now)?;
                (unread, mentions, count)
            }
            (None, Some(counts)) => counts,
            (None, None) => (0, 0, 0),
        };
        rows.push(FolderRow {
            n,
            id: folder_id(n, builtin.as_deref(), rule.is_some()),
            v,
            parent: parent.map(|parent| format!("folder:{parent}")),
            name,
            position,
            smart: rule.and_then(|rule| serde_json::from_str(&rule).ok()),
            unread,
            mentions,
            count,
            favourite,
        });
    }
    Ok(rows)
}

/// A favourite slot and what it goes to: a folder by its ID, or a chat as the chat list shows it.
#[derive(Debug)]
pub struct Favourite {
    pub slot: i64,
    pub folder: Option<String>,
    pub chat: Option<ChatRow>,
}

pub fn favourites(db: &Connection) -> Result<Vec<Favourite>> {
    type Found = (i64, Option<i64>, Option<String>, Option<bool>, Option<i64>);
    let found: Vec<Found> = db
        .prepare_cached(
            "SELECT s.slot, s.folder, f.builtin, f.rule IS NOT NULL, s.chat FROM favourites s
            LEFT JOIN folders f ON f.n = s.folder ORDER BY s.slot",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = Vec::with_capacity(found.len());
    for (slot, folder, builtin, smart, chat) in found {
        let chat = match chat {
            Some(n) => super::read::chat(db, n)?,
            None => None,
        };
        let folder = folder.map(|n| folder_id(n, builtin.as_deref(), smart.unwrap_or(false)));
        out.push(Favourite { slot, folder, chat });
    }
    Ok(out)
}

/// Someone in a roster.
#[derive(Clone, Debug)]
pub struct PersonRow {
    pub n: i64,
    /// The newer of the person's version and their roster row's, which their presence moves.
    pub v: i64,
    /// Around first, then by when last seen, then by name, compared bytewise, read from the top.
    pub sort: String,
    pub name: String,
    pub phone: Option<String>,
    pub about: Option<String>,
    pub avatar: Option<String>,
    /// online; None when not around.
    pub presence: Option<String>,
    pub last_seen: Option<i64>,
    /// Whether jawd follows their presence now.
    pub followed: bool,
    /// Their one-to-one chat's JID.
    pub chat: Option<String>,
    /// The folders of yours they're filed in, by n.
    pub folders: Vec<i64>,
}

/// Where a page of a roster starts: after this person.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PersonCursor {
    sort: String,
    n: i64,
}

impl PersonRow {
    pub fn cursor(&self) -> PersonCursor {
        PersonCursor { sort: self.sort.clone(), n: self.n }
    }
}

/// What person_row reads of person `p`. Their chat and its avatar are the one-to-one chat not being
/// merged into another, which a merge gives the other's avatar if it had none, so a merge ending
/// changes neither.
const PERSON_SELECT: &str = "
    SELECT p.n, max(p.v, p.roster_v), p.roster_sort, p.name, p.push_name, p.verified_name, p.about, p.presence,
        p.last_seen, p.followed,
        coalesce(p.avatar, (SELECT a.avatar FROM chats a WHERE a.person = p.n
            AND NOT EXISTS (SELECT 1 FROM chat_merges g WHERE g.gone = a.n) LIMIT 1)),
        (SELECT d.id FROM chats d WHERE d.person = p.n AND NOT EXISTS (SELECT 1 FROM chat_merges g WHERE g.gone = d.n)
            LIMIT 1),
        (SELECT j.jid FROM person_jids j WHERE j.person = p.n AND j.jid LIKE '%@s.whatsapp.net' LIMIT 1),
        (SELECT json_group_array(i.folder) FROM folder_items i WHERE i.person = p.n)";

fn person_row(r: &Row) -> rusqlite::Result<PersonRow> {
    let phone: Option<String> = r.get(12)?;
    let mut folders: Vec<i64> =
        serde_json::from_str(&r.get::<_, Option<String>>(13)?.unwrap_or_default()).unwrap_or_default();
    folders.sort_unstable();
    Ok(PersonRow {
        n: r.get(0)?,
        v: r.get(1)?,
        sort: r.get(2)?,
        name: shown(r.get(3)?, r.get(4)?, r.get(5)?, phone.as_deref()),
        phone: phone.as_deref().and_then(|jid| jid.strip_suffix("@s.whatsapp.net")).map(|user| format!("+{user}")),
        about: r.get::<_, Option<String>>(6)?.filter(|about| !about.is_empty()),
        presence: r.get(7)?,
        last_seen: r.get(8)?,
        followed: r.get(9)?,
        avatar: r.get(10)?,
        chat: r.get(11)?,
        folders,
    })
}

/// The account's own people, whom no roster lists.
fn me(db: &Connection) -> Result<String> {
    let jids: String = db
        .prepare_cached("SELECT value FROM sync_state WHERE key = 'me'")?
        .query_row([], |r| r.get(0))
        .optional()?
        .unwrap_or_default();
    let jids: Vec<&str> = jids.split_whitespace().collect();
    let people: Vec<i64> = db
        .prepare_cached("SELECT person FROM person_jids WHERE jid IN (SELECT value FROM json_each(?1))")?
        .query_map([serde_json::to_string(&jids)?], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(serde_json::to_string(&people)?)
}

/// The sort every row of a roster that shows only who's around comes before.
const AROUND_ONLY: &str = "1";
/// The sort every row of a roster comes before.
const EVERYONE: &str = "3";

fn people(db: &Connection, sql: &str, params: impl rusqlite::Params) -> Result<Vec<PersonRow>> {
    Ok(db.prepare_cached(sql)?.query_map(params, person_row)?.collect::<rusqlite::Result<_>>()?)
}

/// What a roster of `list` holds that its chat list says nothing of: the built-in folders of
/// groups have no one, and the rest hold the people whose one-to-one chats they hold.
fn roster_kind(list: &List) -> Option<&List> {
    match list {
        List::Groups | List::Communities => None,
        other => Some(other),
    }
}

/// Up to `limit` people of `list`'s roster after `after` (from the top without one), around first;
/// with `offline` false, only those around.
pub fn roster(
    db: &Connection,
    list: &List,
    after: Option<&PersonCursor>,
    limit: usize,
    offline: bool,
    now: i64,
) -> Result<Vec<PersonRow>> {
    let (sort, n) = after.map_or(("", i64::MIN), |after| (after.sort.as_str(), after.n));
    roster_page(db, list, sort, n, limit, offline, now, false)
}

/// Up to `limit` people of `list`'s roster just before `cursor`, in the roster's order.
pub fn roster_above(
    db: &Connection,
    list: &List,
    cursor: &PersonCursor,
    limit: usize,
    offline: bool,
    now: i64,
) -> Result<Vec<PersonRow>> {
    // Only those around are above the first who isn't, when only they're asked for.
    let (sort, n) = match offline || cursor.sort.as_str() < AROUND_ONLY {
        true => (cursor.sort.as_str(), cursor.n),
        false => (AROUND_ONLY, i64::MIN),
    };
    let mut rows = roster_page(db, list, sort, n, limit, offline, now, true)?;
    rows.reverse();
    Ok(rows)
}

/// Up to `limit` people from `cursor` on, the one at it first if the roster has them.
pub fn roster_from(
    db: &Connection,
    list: &List,
    cursor: &PersonCursor,
    limit: usize,
    offline: bool,
    now: i64,
) -> Result<Vec<PersonRow>> {
    roster_page(db, list, &cursor.sort, cursor.n.saturating_sub(1), limit, offline, now, false)
}

#[allow(clippy::too_many_arguments)]
fn roster_page(
    db: &Connection,
    list: &List,
    sort: &str,
    n: i64,
    limit: usize,
    offline: bool,
    now: i64,
    up: bool,
) -> Result<Vec<PersonRow>> {
    let Some(list) = roster_kind(list) else { return Ok(Vec::new()) };
    let upto = if offline { EVERYONE } else { AROUND_ONLY };
    let limit = limit as i64;
    use rusqlite::types::Value as V;
    match (list, up) {
        (List::All, false) => {
            let sql = format!(
                "{PERSON_SELECT} FROM people p WHERE (p.name IS NOT NULL OR p.direct_chats > 0)
                AND (p.roster_sort, p.n) > (?1, ?2) AND p.roster_sort < ?3 AND p.n NOT IN (SELECT value FROM json_each(?5))
                ORDER BY p.roster_sort, p.n LIMIT ?4"
            );
            people(db, &sql, params![sort, n, upto, limit, me(db)?])
        }
        (List::All, true) => {
            let sql = format!(
                "{PERSON_SELECT} FROM people p WHERE (p.name IS NOT NULL OR p.direct_chats > 0)
                AND (p.roster_sort, p.n) < (?1, ?2) AND ?3 <> '' AND p.n NOT IN (SELECT value FROM json_each(?5))
                ORDER BY p.roster_sort DESC, p.n DESC LIMIT ?4"
            );
            people(db, &sql, params![sort, n, upto, limit, me(db)?])
        }
        (List::Folder(folder), false) => {
            let sql = format!(
                "{PERSON_SELECT} FROM folder_people fp CROSS JOIN people p ON p.n = fp.person
                WHERE fp.folder = ?5 AND (fp.roster_sort, fp.person) > (?1, ?2) AND fp.roster_sort < ?3
                ORDER BY fp.roster_sort, fp.person LIMIT ?4"
            );
            people(db, &sql, params![sort, n, upto, limit, folder])
        }
        (List::Folder(folder), true) => {
            let sql = format!(
                "{PERSON_SELECT} FROM folder_people fp CROSS JOIN people p ON p.n = fp.person
                WHERE fp.folder = ?5 AND (fp.roster_sort, fp.person) < (?1, ?2) AND ?3 <> ''
                ORDER BY fp.roster_sort DESC, fp.person DESC LIMIT ?4"
            );
            people(db, &sql, params![sort, n, upto, limit, folder])
        }
        (List::Archived, up) => {
            let sql = if up {
                format!(
                    "{PERSON_SELECT} FROM people p WHERE (p.name IS NOT NULL OR p.direct_chats > 0)
                    AND (p.roster_sort, p.n) < (?1, ?2) AND ?3 <> ''
                    AND EXISTS (SELECT 1 FROM chats c WHERE c.person = p.n AND c.archived <> 0)
                    ORDER BY p.roster_sort DESC, p.n DESC LIMIT ?4"
                )
            } else {
                format!(
                    "{PERSON_SELECT} FROM people p WHERE (p.name IS NOT NULL OR p.direct_chats > 0)
                    AND (p.roster_sort, p.n) > (?1, ?2) AND p.roster_sort < ?3
                    AND EXISTS (SELECT 1 FROM chats c WHERE c.person = p.n AND c.archived <> 0)
                    ORDER BY p.roster_sort, p.n LIMIT ?4"
                )
            };
            people(db, &sql, params![sort, n, upto, limit])
        }
        (List::Unread, up) => {
            let sql = if up {
                format!(
                    "{PERSON_SELECT} FROM people p WHERE (p.name IS NOT NULL OR p.direct_chats > 0)
                    AND (p.roster_sort, p.n) < (?1, ?2) AND ?3 <> ''
                    AND EXISTS (SELECT 1 FROM chats c WHERE c.person = p.n AND c.archived = 0
                        AND (c.unread > 0 OR c.marked_unread <> 0))
                    ORDER BY p.roster_sort DESC, p.n DESC LIMIT ?4"
                )
            } else {
                format!(
                    "{PERSON_SELECT} FROM people p WHERE (p.name IS NOT NULL OR p.direct_chats > 0)
                    AND (p.roster_sort, p.n) > (?1, ?2) AND p.roster_sort < ?3
                    AND EXISTS (SELECT 1 FROM chats c WHERE c.person = p.n AND c.archived = 0
                        AND (c.unread > 0 OR c.marked_unread <> 0))
                    ORDER BY p.roster_sort, p.n LIMIT ?4"
                )
            };
            people(db, &sql, params![sort, n, upto, limit])
        }
        (List::Smart(folder), up) => {
            let Some(rule) = rule_of(db, *folder)? else { return Ok(Vec::new()) };
            let sql = if up {
                format!(
                    "{PERSON_SELECT} FROM people p WHERE (p.name IS NOT NULL OR p.direct_chats > 0)
                    AND (p.roster_sort, p.n) < (?1, ?2) AND ?3 <> ''
                    AND EXISTS (SELECT 1 FROM chats c WHERE c.person = p.n AND c.archived = 0 {RULED})
                    ORDER BY p.roster_sort DESC, p.n DESC LIMIT ?4"
                )
            } else {
                format!(
                    "{PERSON_SELECT} FROM people p WHERE (p.name IS NOT NULL OR p.direct_chats > 0)
                    AND (p.roster_sort, p.n) > (?1, ?2) AND p.roster_sort < ?3
                    AND EXISTS (SELECT 1 FROM chats c WHERE c.person = p.n AND c.archived = 0 {RULED})
                    ORDER BY p.roster_sort, p.n LIMIT ?4"
                )
            };
            let mut bound = vec![V::Text(sort.into()), V::Integer(n), V::Text(upto.into()), V::Integer(limit)];
            bound.extend(rule.bound(now));
            people(db, &sql, rusqlite::params_from_iter(bound))
        }
        (List::Groups | List::Communities, _) => Ok(Vec::new()),
    }
}

/// How many people `list`'s roster has, those around alone with `offline` false; counted from its
/// index, up to SMART_COUNTED for the rosters of chat lists that aren't folders of yours.
pub fn roster_count(db: &Connection, list: &List, offline: bool, now: i64) -> Result<u64> {
    let Some(list) = roster_kind(list) else { return Ok(0) };
    let upto = if offline { EVERYONE } else { AROUND_ONLY };
    let count: i64 = match list {
        List::All => db
            .prepare_cached(
                "SELECT count(*) FROM people p WHERE (p.name IS NOT NULL OR p.direct_chats > 0) AND p.roster_sort < ?1
                AND p.n NOT IN (SELECT value FROM json_each(?2))",
            )?
            .query_row(params![upto, me(db)?], |r| r.get(0))?,
        List::Folder(folder) => db
            .prepare_cached("SELECT count(*) FROM folder_people WHERE folder = ?1 AND roster_sort < ?2")?
            .query_row(params![folder, upto], |r| r.get(0))?,
        other => {
            let mut counted = 0;
            let mut after: Option<PersonCursor> = None;
            loop {
                let page = roster(db, other, after.as_ref(), 200, offline, now)?;
                counted += page.len() as i64;
                match page.last() {
                    Some(last) if page.len() == 200 && counted < SMART_COUNTED => after = Some(last.cursor()),
                    _ => break,
                }
            }
            counted.min(SMART_COUNTED)
        }
    };
    Ok(count.max(0) as u64)
}

/// How folders' counts and what they're kept through have drifted from what they're derived from,
/// each drift a sentence; none when they agree. Read in one snapshot, so a write half seen can't
/// look like drift; it reads every chat, folder and item, so it runs in the background.
pub fn drift(db: &Connection) -> Result<Vec<String>> {
    let db = db.unchecked_transaction()?;
    let mut drift = Vec::new();
    let count = |sql: &str| -> Result<i64> { Ok(db.prepare_cached(sql)?.query_row([], |r| r.get(0))?) };
    let builtins = count(
        "SELECT count(*) FROM folders f JOIN folder_counts k ON k.folder = f.n
        WHERE f.builtin IS NOT NULL AND (k.chats, k.unread, k.mentions) IS NOT (SELECT count(c.n), coalesce(sum(c.unread), 0),
            coalesce(sum(c.mentions), 0) FROM chats c WHERE CASE f.builtin
                WHEN 'all' THEN c.archived = 0
                WHEN 'unread' THEN c.archived = 0 AND (c.unread > 0 OR c.marked_unread <> 0)
                WHEN 'groups' THEN c.archived = 0 AND c.kind IN ('group', 'announcement')
                WHEN 'communities' THEN c.archived = 0 AND (c.kind = 'community' OR c.community IS NOT NULL)
                ELSE c.archived <> 0 END)",
    )?;
    if builtins > 0 {
        drift.push(format!("{builtins} built-in folders' counts aren't their chats'"));
    }
    let paths = count(
        "WITH RECURSIVE up (folder, ancestor) AS (
            SELECT n, n FROM folders WHERE builtin IS NULL AND rule IS NULL
            UNION SELECT up.folder, f.parent FROM up JOIN folders f ON f.n = up.ancestor WHERE f.parent IS NOT NULL)
        SELECT (SELECT count(*) FROM (SELECT folder, ancestor FROM up EXCEPT SELECT folder, ancestor FROM folder_paths))
            + (SELECT count(*) FROM (SELECT folder, ancestor FROM folder_paths EXCEPT SELECT folder, ancestor FROM up))",
    )?;
    if paths > 0 {
        drift.push(format!("{paths} of the folders' places in the tree aren't the tree's"));
    }
    let chats = count(
        "WITH reach (folder, chat) AS (
            SELECT p.ancestor, i.chat FROM folder_items i JOIN folder_paths p ON p.folder = i.folder WHERE i.chat IS NOT NULL
            UNION ALL
            SELECT p.ancestor, c.n FROM folder_items i JOIN folder_paths p ON p.folder = i.folder
                JOIN chats c ON c.person = i.person WHERE i.person IS NOT NULL),
        counted (folder, chat, paths) AS (SELECT folder, chat, count(*) FROM reach GROUP BY folder, chat)
        SELECT (SELECT count(*) FROM (SELECT folder, chat, paths FROM counted EXCEPT SELECT folder, chat, paths FROM folder_chats))
            + (SELECT count(*) FROM (SELECT folder, chat, paths FROM folder_chats EXCEPT SELECT folder, chat, paths FROM counted))",
    )?;
    if chats > 0 {
        drift.push(format!("{chats} of the chats folders hold aren't those filed in them"));
    }
    let people = count(
        "WITH counted (folder, person, paths) AS (
            SELECT p.ancestor, i.person, count(*) FROM folder_items i JOIN folder_paths p ON p.folder = i.folder
            WHERE i.person IS NOT NULL GROUP BY p.ancestor, i.person)
        SELECT (SELECT count(*) FROM (SELECT folder, person, paths FROM counted EXCEPT SELECT folder, person, paths FROM folder_people))
            + (SELECT count(*) FROM (SELECT folder, person, paths FROM folder_people EXCEPT SELECT folder, person, paths FROM counted))",
    )?;
    if people > 0 {
        drift.push(format!("{people} of the people folders hold aren't those filed in them"));
    }
    let copied = count(
        "SELECT (SELECT count(*) FROM folder_chats f JOIN chats c ON c.n = f.chat
                WHERE f.sort IS NOT c.sort OR f.unread IS NOT c.unread OR f.mentions IS NOT c.mentions)
            + (SELECT count(*) FROM folder_people f JOIN people p ON p.n = f.person
                WHERE f.roster_sort IS NOT p.roster_sort OR f.lone IS NOT (p.direct_chats = 0))",
    )?;
    if copied > 0 {
        drift.push(format!("{copied} of what folders keep of their chats and people isn't as they are"));
    }
    let direct = count(
        "SELECT count(*) FROM people p WHERE p.direct_chats IS NOT (SELECT count(*) FROM chats c WHERE c.person = p.n)",
    )?;
    if direct > 0 {
        drift.push(format!("{direct} people's counts of their chats aren't their chats"));
    }
    let yours = count(
        "SELECT count(*) FROM folders f JOIN folder_counts k ON k.folder = f.n
        WHERE f.builtin IS NULL AND f.rule IS NULL AND (k.chats, k.unread, k.mentions, k.lone) IS NOT (
            SELECT (SELECT count(*) FROM folder_chats WHERE folder = f.n),
                (SELECT coalesce(sum(unread), 0) FROM folder_chats WHERE folder = f.n),
                (SELECT coalesce(sum(mentions), 0) FROM folder_chats WHERE folder = f.n),
                (SELECT count(*) FROM folder_people WHERE folder = f.n AND lone))",
    )?;
    let uncounted = count(
        "SELECT count(*) FROM folders f WHERE (f.builtin IS NOT NULL OR f.rule IS NULL)
            AND NOT EXISTS (SELECT 1 FROM folder_counts WHERE folder = f.n)",
    )?;
    if yours + uncounted > 0 {
        drift.push(format!("{} of your folders' counts aren't what they hold", yours + uncounted));
    }
    Ok(drift)
}

/// Where person `n` sorts in a roster.
pub fn person_place(db: &Connection, n: i64) -> Result<Option<PersonCursor>> {
    Ok(db
        .prepare_cached("SELECT roster_sort, n FROM people WHERE n = ?1")?
        .query_row([n], |r| Ok(PersonCursor { sort: r.get(0)?, n: r.get(1)? }))
        .optional()?)
}
