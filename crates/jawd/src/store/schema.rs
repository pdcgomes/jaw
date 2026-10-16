//! jaw.db's schema, as numbered migrations run in order at start; `PRAGMA user_version` says how
//! many have run. A merged migration is frozen: `migrations.lock` has a line for each, its number,
//! its kind and the SHA-256 of its text, which a test checks. Consecutive schema steps run in one
//! IMMEDIATE transaction, and a data step, whose cost grows with the rows, in one of its own. The
//! schema must be the one the migrations make, before and after, or jawd refuses to write.

use std::ops::Range;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, TransactionBehavior};
use sha2::{Digest, Sha256};

pub(super) const MIGRATIONS: &[&str] = &[
    // 1: what the engine sends, kept before WhatsApp hears it arrived, and history as it came.
    "
    CREATE TABLE inbox (
        seq INTEGER PRIMARY KEY,
        kind TEXT NOT NULL,
        at INTEGER NOT NULL,
        payload BLOB NOT NULL
    );
    CREATE TABLE history_raw (
        chat TEXT NOT NULL,
        id TEXT NOT NULL,
        at INTEGER NOT NULL,
        raw BLOB NOT NULL,
        PRIMARY KEY (chat, id)
    ) WITHOUT ROWID;
    ",
    // 2: what interfaces show, converted from the raw protobufs, which stay the source of truth.
    // Every row an interface watches has `v`, the seq of the event that last changed it.
    "
    CREATE TABLE people (
        n INTEGER PRIMARY KEY,
        name TEXT,
        push_name TEXT,
        verified_name TEXT,
        about TEXT,
        about_at INTEGER,
        avatar TEXT,
        avatar_id TEXT,
        last_seen INTEGER,
        v INTEGER NOT NULL
    );
    -- Each JID a person is known by, their phone number's and their LID's, without a device.
    -- Messages, reactions, receipts, votes and members name the JID WhatsApp gave, so joining two
    -- people moves their JIDs and leaves those rows alone.
    CREATE TABLE person_jids (
        n INTEGER PRIMARY KEY,
        jid TEXT NOT NULL UNIQUE,
        person INTEGER NOT NULL REFERENCES people (n)
    );
    CREATE INDEX person_jids_by_person ON person_jids (person);

    CREATE TABLE chats (
        n INTEGER PRIMARY KEY,
        id TEXT NOT NULL UNIQUE,
        kind TEXT NOT NULL,
        name TEXT,
        person INTEGER REFERENCES people (n) ON DELETE SET NULL,
        community TEXT,
        pinned INTEGER NOT NULL DEFAULT 0,
        archived INTEGER NOT NULL DEFAULT 0,
        muted_until INTEGER NOT NULL DEFAULT 0,
        marked_unread INTEGER NOT NULL DEFAULT 0,
        unread INTEGER NOT NULL DEFAULT 0,
        mentions INTEGER NOT NULL DEFAULT 0,
        ephemeral INTEGER NOT NULL DEFAULT 0,
        draft TEXT,
        avatar TEXT,
        frecency REAL NOT NULL DEFAULT 0,
        opened_at INTEGER,
        last_at INTEGER NOT NULL DEFAULT 0,
        last_message INTEGER,
        sort TEXT GENERATED ALWAYS AS (iif(pinned > 0, '1~', '0~') || printf('%013d', last_at)) STORED,
        v INTEGER NOT NULL
    );
    CREATE INDEX chats_by_sort ON chats (archived, sort, n);
    CREATE INDEX chats_by_person ON chats (person) WHERE person IS NOT NULL;

    -- n only goes up, so messages are appended wherever in time they belong. t is the message's
    -- time in milliseconds times 2^20 plus a count: FTS5's rowid, so search reads its matches
    -- newest first and stops at a page.
    CREATE TABLE messages (
        n INTEGER PRIMARY KEY,
        chat INTEGER NOT NULL REFERENCES chats (n) ON DELETE CASCADE,
        id TEXT NOT NULL,
        at INTEGER NOT NULL,
        t INTEGER NOT NULL UNIQUE,
        sender INTEGER REFERENCES person_jids (n),
        mine INTEGER NOT NULL DEFAULT 0,
        kind TEXT NOT NULL,
        status TEXT,
        text TEXT,
        spans TEXT,
        -- NULL when it's the text, as it mostly is.
        summary TEXT,
        reply TEXT,
        detail TEXT,
        forwarded INTEGER NOT NULL DEFAULT 0,
        edited_at INTEGER,
        deleted INTEGER NOT NULL DEFAULT 0,
        expires_at INTEGER,
        starred INTEGER NOT NULL DEFAULT 0,
        view_once INTEGER NOT NULL DEFAULT 0,
        v INTEGER NOT NULL,
        raw BLOB
    );
    CREATE UNIQUE INDEX messages_by_id ON messages (chat, id);
    -- Every index entry ends with the rowid, so this is the (chat, time, rowid) keyset.
    CREATE INDEX messages_by_time ON messages (chat, at);
    CREATE INDEX messages_by_sender ON messages (sender) WHERE sender IS NOT NULL;
    CREATE INDEX messages_expiring ON messages (expires_at) WHERE expires_at IS NOT NULL;

    CREATE VIRTUAL TABLE messages_fts USING fts5 (
        text,
        content = 'messages',
        content_rowid = 't',
        tokenize = 'unicode61 remove_diacritics 2'
    );
    CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages WHEN new.text IS NOT NULL BEGIN
        INSERT INTO messages_fts (rowid, text) VALUES (new.t, new.text);
    END;
    CREATE TRIGGER messages_fts_delete AFTER DELETE ON messages WHEN old.text IS NOT NULL BEGIN
        INSERT INTO messages_fts (messages_fts, rowid, text) VALUES ('delete', old.t, old.text);
    END;
    CREATE TRIGGER messages_fts_update AFTER UPDATE OF text ON messages WHEN old.text IS NOT new.text BEGIN
        INSERT INTO messages_fts (messages_fts, rowid, text) SELECT 'delete', old.t, old.text WHERE old.text IS NOT NULL;
        INSERT INTO messages_fts (rowid, text) SELECT new.t, new.text WHERE new.text IS NOT NULL;
    END;

    -- One row per SD or HD copy. The keys to download it are in the message's raw protobuf.
    CREATE TABLE media (
        message INTEGER NOT NULL REFERENCES messages (n) ON DELETE CASCADE,
        variant TEXT NOT NULL,
        mime TEXT,
        bytes INTEGER,
        w INTEGER,
        h INTEGER,
        seconds REAL,
        name TEXT,
        sha256 BLOB,
        thumb BLOB,
        waveform BLOB,
        path TEXT,
        preview_small TEXT,
        preview_large TEXT,
        state TEXT NOT NULL DEFAULT 'remote',
        progress REAL,
        opened_at INTEGER,
        PRIMARY KEY (message, variant)
    ) WITHOUT ROWID;
    CREATE INDEX media_by_hash ON media (sha256) WHERE sha256 IS NOT NULL;

    CREATE TABLE reactions (
        message INTEGER NOT NULL REFERENCES messages (n) ON DELETE CASCADE,
        sender INTEGER NOT NULL REFERENCES person_jids (n),
        emoji TEXT NOT NULL,
        at INTEGER NOT NULL,
        PRIMARY KEY (message, sender)
    ) WITHOUT ROWID;
    CREATE INDEX reactions_by_sender ON reactions (sender);

    CREATE TABLE receipts (
        message INTEGER NOT NULL REFERENCES messages (n) ON DELETE CASCADE,
        recipient INTEGER NOT NULL REFERENCES person_jids (n),
        delivered_at INTEGER,
        read_at INTEGER,
        played_at INTEGER,
        PRIMARY KEY (message, recipient)
    ) WITHOUT ROWID;
    CREATE INDEX receipts_by_recipient ON receipts (recipient);

    CREATE TABLE poll_options (
        message INTEGER NOT NULL REFERENCES messages (n) ON DELETE CASCADE,
        position INTEGER NOT NULL,
        name TEXT NOT NULL,
        -- SHA-256 of the name, which is how votes name an option.
        hash BLOB NOT NULL,
        PRIMARY KEY (message, position)
    ) WITHOUT ROWID;
    CREATE TABLE poll_votes (
        message INTEGER NOT NULL REFERENCES messages (n) ON DELETE CASCADE,
        voter INTEGER NOT NULL REFERENCES person_jids (n),
        position INTEGER NOT NULL,
        at INTEGER NOT NULL,
        PRIMARY KEY (message, voter, position)
    ) WITHOUT ROWID;
    CREATE INDEX poll_votes_by_voter ON poll_votes (voter);

    CREATE TABLE group_members (
        chat INTEGER NOT NULL REFERENCES chats (n) ON DELETE CASCADE,
        member INTEGER NOT NULL REFERENCES person_jids (n),
        role TEXT NOT NULL DEFAULT 'member',
        PRIMARY KEY (chat, member)
    ) WITHOUT ROWID;
    CREATE INDEX group_members_by_member ON group_members (member);

    -- jaw's own folders, a tree; an item is a chat or a person and can sit in several.
    CREATE TABLE folders (
        n INTEGER PRIMARY KEY,
        parent INTEGER REFERENCES folders (n) ON DELETE CASCADE,
        name TEXT NOT NULL,
        position INTEGER NOT NULL DEFAULT 0,
        smart TEXT,
        favourite INTEGER UNIQUE CHECK (favourite BETWEEN 1 AND 9),
        v INTEGER NOT NULL
    );
    CREATE INDEX folders_by_parent ON folders (parent, position);
    CREATE TABLE folder_items (
        folder INTEGER NOT NULL REFERENCES folders (n) ON DELETE CASCADE,
        chat INTEGER REFERENCES chats (n) ON DELETE CASCADE,
        person INTEGER REFERENCES people (n) ON DELETE CASCADE,
        position INTEGER NOT NULL DEFAULT 0,
        CHECK ((chat IS NULL) <> (person IS NULL))
    );
    CREATE UNIQUE INDEX folder_items_chats ON folder_items (folder, chat) WHERE chat IS NOT NULL;
    CREATE UNIQUE INDEX folder_items_people ON folder_items (folder, person) WHERE person IS NOT NULL;
    CREATE INDEX folder_items_by_chat ON folder_items (chat) WHERE chat IS NOT NULL;
    CREATE INDEX folder_items_by_person ON folder_items (person) WHERE person IS NOT NULL;

    -- Edits, reactions and votes for messages not seen yet, applied when their message arrives.
    CREATE TABLE pending (
        n INTEGER PRIMARY KEY,
        chat TEXT NOT NULL,
        target TEXT NOT NULL,
        kind TEXT NOT NULL,
        at INTEGER NOT NULL,
        raw BLOB NOT NULL
    );
    CREATE INDEX pending_by_target ON pending (chat, target);

    -- What's waiting to be sent, by the ID jawd made for it.
    CREATE TABLE outbox (
        id TEXT PRIMARY KEY,
        chat TEXT NOT NULL,
        kind TEXT NOT NULL,
        raw BLOB NOT NULL,
        state TEXT NOT NULL DEFAULT 'queued',
        attempts INTEGER NOT NULL DEFAULT 0,
        next_at INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        error TEXT
    );
    CREATE INDEX outbox_by_chat ON outbox (chat, created_at);

    -- The feed: one row per changed row, written in the change's transaction. `key` is the row's
    -- n in its table, and `chat` the chat a message belongs to, so the feed can be sorted into
    -- windows without reading the rows, even ones since removed.
    CREATE TABLE events (
        seq INTEGER PRIMARY KEY,
        at INTEGER NOT NULL,
        type TEXT NOT NULL,
        key INTEGER NOT NULL,
        chat INTEGER
    );
    ",
    // 3: how many messages each chat has, kept as they come and go, so a window's total is read,
    // not counted.
    "
    ALTER TABLE chats ADD COLUMN message_count INTEGER NOT NULL DEFAULT 0;
    UPDATE chats SET message_count = (SELECT count(*) FROM messages WHERE chat = chats.n);
    ",
    // 4: history's text waits in fts_backlog to be indexed in short steps of its own, so FTS5's
    // work never lands in a transaction a live message waits behind. It's keyed by the message's
    // n, which only goes up, so the backlog adds no pages to a commit beyond its last few. A
    // message still in the backlog isn't in the index, so the triggers leave the index alone for it.
    "
    CREATE TABLE fts_backlog (n INTEGER PRIMARY KEY);
    -- FTS5 merges its segments in the writer's idle moments, a few pages at a time, rather than at
    -- the commit of whichever transaction happens to fill a level.
    INSERT INTO messages_fts (messages_fts, rank) VALUES ('automerge', 0);
    INSERT INTO messages_fts (messages_fts, rank) VALUES ('crisismerge', 64);
    DROP TRIGGER messages_fts_insert;
    DROP TRIGGER messages_fts_delete;
    DROP TRIGGER messages_fts_update;
    CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages
    WHEN new.text IS NOT NULL AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = new.n) BEGIN
        INSERT INTO messages_fts (rowid, text) VALUES (new.t, new.text);
    END;
    CREATE TRIGGER messages_fts_delete AFTER DELETE ON messages BEGIN
        INSERT INTO messages_fts (messages_fts, rowid, text) SELECT 'delete', old.t, old.text
            WHERE old.text IS NOT NULL AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = old.n);
        DELETE FROM fts_backlog WHERE n = old.n;
    END;
    CREATE TRIGGER messages_fts_update AFTER UPDATE OF text ON messages
    WHEN old.text IS NOT new.text AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = old.n) BEGIN
        INSERT INTO messages_fts (messages_fts, rowid, text) SELECT 'delete', old.t, old.text WHERE old.text IS NOT NULL;
        INSERT INTO messages_fts (rowid, text) SELECT new.t, new.text WHERE new.text IS NOT NULL;
    END;

    -- Pinned chats sort among themselves by when they were pinned, the latest first.
    DROP INDEX chats_by_sort;
    ALTER TABLE chats DROP COLUMN sort;
    ALTER TABLE chats ADD COLUMN sort TEXT GENERATED ALWAYS AS
        (iif(pinned > 0, '1~' || printf('%013d', pinned), '0~' || printf('%013d', last_at))) VIRTUAL;
    CREATE INDEX chats_by_sort ON chats (archived, sort, n);
    -- The picture's ID WhatsApp gave, so it's fetched again only when it changes.
    ALTER TABLE chats ADD COLUMN avatar_id TEXT;
    ALTER TABLE reactions ADD COLUMN mine INTEGER NOT NULL DEFAULT 0;
    -- What a pending amendment does, already converted, and who did it, so it's applied when its
    -- message arrives without parsing its raw protobuf again.
    ALTER TABLE pending ADD COLUMN sender TEXT;
    ALTER TABLE pending ADD COLUMN mine INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE pending ADD COLUMN effect TEXT NOT NULL DEFAULT '';
    -- What sync remembers between runs, such as the account's JIDs and which app state is synced.
    CREATE TABLE sync_state (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
    -- Each app-state mutation's latest value as WhatsApp sent it, by its index as a JSON array,
    -- whether jaw uses it yet or not.
    CREATE TABLE app_state (key TEXT PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;
    ",
    // 5, a schema step: the review's rules for what sync writes. The inbox's seq never goes back, so
    // the converter, which reads past the last seq it took, never misses a row once the inbox has
    // drained; a change that fails to convert is kept in quarantine; a message deleted for me stays
    // deleted; unread is counted from where each chat was read; what a history blob carries
    // besides its messages is kept; and two chats found to be one person's are merged. The inbox's
    // rebuild costs 7 ms per 1,000 rows, and the inbox holds what hasn't been converted yet, a few
    // rows at most once jawd has run.
    "
    -- Class: raw. The inbox's seq never goes back, even once the inbox has drained, so the
    -- converter, which reads past the last seq it took, never misses a row. A rebuild of a small
    -- table: nothing refers to it, and sqlite_sequence follows the rename at the largest seq copied.
    CREATE TABLE inbox_rebuilt (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        kind TEXT NOT NULL,
        at INTEGER NOT NULL,
        payload BLOB NOT NULL
    ) STRICT;
    INSERT INTO inbox_rebuilt (seq, kind, at, payload) SELECT seq, kind, at, payload FROM inbox;
    DROP TABLE inbox;
    ALTER TABLE inbox_rebuilt RENAME TO inbox;

    -- Class: jaw's own. A change that failed, kept with its error and what it came from instead of
    -- being dropped: `source` and `ref` name the raw row it was converted from (`inbox` and its
    -- seq, `history_raw` and its chat and ID), or the change itself, whose raw protobuf is then
    -- kept in `raw`.
    CREATE TABLE quarantine (
        n INTEGER PRIMARY KEY,
        at INTEGER NOT NULL,
        source TEXT NOT NULL,
        ref TEXT NOT NULL,
        error TEXT NOT NULL,
        raw BLOB
    ) STRICT;
    CREATE UNIQUE INDEX quarantine_by_ref ON quarantine (source, ref);

    -- Class: jaw's own. Messages deleted for me, so a history copy or a message sent again doesn't
    -- bring them back.
    CREATE TABLE tombstones (
        chat INTEGER NOT NULL REFERENCES chats (n) ON DELETE CASCADE,
        id TEXT NOT NULL,
        at INTEGER NOT NULL,
        PRIMARY KEY (chat, id)
    ) STRICT, WITHOUT ROWID;

    -- Class: raw. Each history blob as it came, with its conversations' messages taken out, since
    -- each message's raw is kept with it: names, settings, statuses and whatever else jaw doesn't
    -- read yet, which can't be fetched again.
    CREATE TABLE history_kept (
        n INTEGER PRIMARY KEY,
        id TEXT NOT NULL UNIQUE,
        at INTEGER NOT NULL,
        raw BLOB NOT NULL
    ) STRICT;

    -- Where each chat was read up to, as a message time: messages from others after it are unread.
    -- `read_known` says WhatsApp or you said so; until then history's messages count as read as
    -- they come. `phone_unread` and `phone_mentions` are the phone's own counts as of `read_at`,
    -- kept when history didn't bring enough of the chat's messages to find where it was read, and
    -- dropped once the position moves on. Class: fetched from WhatsApp.
    ALTER TABLE chats ADD COLUMN read_at INTEGER;
    ALTER TABLE chats ADD COLUMN read_known INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE chats ADD COLUMN phone_unread INTEGER;
    ALTER TABLE chats ADD COLUMN phone_mentions INTEGER;

    -- Class: jaw's own. A chat being merged into another, the same person's under another JID,
    -- its messages moved a few at a time; writes for it go to `keep` meanwhile.
    CREATE TABLE chat_merges (
        gone INTEGER PRIMARY KEY REFERENCES chats (n) ON DELETE CASCADE,
        keep INTEGER NOT NULL REFERENCES chats (n) ON DELETE CASCADE
    ) STRICT;
    CREATE INDEX chat_merges_by_keep ON chat_merges (keep);

    -- What waits for a message is applied in the order it was done, read from the index.
    DROP INDEX pending_by_target;
    CREATE INDEX pending_by_target_at ON pending (chat, target, at);
    ",
    // 6, a data step: messages and people rebuilt so their n, which the feed and find hold, is never
    // used again once a row is deleted (AUTOINCREMENT, and the sequence started past every key an
    // event still names), as STRICT tables with their booleans and JSON checked; a message keeps the
    // engine's whole event for a live one beside its WebMessageInfo, and whether it mentions you;
    // each chat's message count is kept by triggers; and each chat's read position starts where
    // its newest message is, with the counts it had. Measured at 6.6 s per million messages on the
    // reference Mac, its commit and foreign_key_check included.
    "
    -- Class: derived from raw.
    CREATE TABLE messages_rebuilt (
        n INTEGER PRIMARY KEY AUTOINCREMENT,
        chat INTEGER NOT NULL REFERENCES chats (n) ON DELETE CASCADE,
        id TEXT NOT NULL,
        at INTEGER NOT NULL,
        t INTEGER NOT NULL UNIQUE,
        sender INTEGER REFERENCES person_jids (n),
        mine INTEGER NOT NULL DEFAULT 0 CHECK (mine IN (0, 1)),
        kind TEXT NOT NULL,
        status TEXT,
        text TEXT,
        spans TEXT CHECK (json_valid(spans)),
        summary TEXT,
        reply TEXT CHECK (json_valid(reply)),
        detail TEXT CHECK (json_valid(detail)),
        forwarded INTEGER NOT NULL DEFAULT 0,
        edited_at INTEGER,
        deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
        expires_at INTEGER,
        starred INTEGER NOT NULL DEFAULT 0 CHECK (starred IN (0, 1)),
        view_once INTEGER NOT NULL DEFAULT 0 CHECK (view_once IN (0, 1)),
        mentions_me INTEGER NOT NULL DEFAULT 0 CHECK (mentions_me IN (0, 1)),
        v INTEGER NOT NULL,
        -- WhatsApp's WebMessageInfo, compressed: a history copy's, or one made from the engine's.
        raw BLOB,
        -- For a live message, the engine's whole event (what whatsmeow said about it, the protobuf
        -- as it came and what only the engine could decrypt), compressed; a history copy never
        -- replaces it.
        envelope BLOB
    ) STRICT;
    INSERT INTO messages_rebuilt (n, chat, id, at, t, sender, mine, kind, status, text, spans, summary, reply,
        detail, forwarded, edited_at, deleted, expires_at, starred, view_once, v, raw)
    SELECT n, chat, id, at, t, sender, mine, kind, status, text, spans, summary, reply, detail, forwarded,
        edited_at, deleted, expires_at, starred, view_once, v, raw
    FROM messages ORDER BY n;
    DROP TABLE messages;
    ALTER TABLE messages_rebuilt RENAME TO messages;
    INSERT INTO sqlite_sequence (name, seq) SELECT 'messages', 0
        WHERE NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE name = 'messages');
    UPDATE sqlite_sequence SET seq = max(seq, coalesce((SELECT max(key) FROM events WHERE type = 'message'), 0))
        WHERE name = 'messages';
    CREATE UNIQUE INDEX messages_by_id ON messages (chat, id);
    CREATE INDEX messages_by_time ON messages (chat, at);
    CREATE INDEX messages_by_sender ON messages (sender) WHERE sender IS NOT NULL;
    CREATE INDEX messages_expiring ON messages (expires_at) WHERE expires_at IS NOT NULL;
    CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages
    WHEN new.text IS NOT NULL AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = new.n) BEGIN
        INSERT INTO messages_fts (rowid, text) VALUES (new.t, new.text);
    END;
    CREATE TRIGGER messages_fts_delete AFTER DELETE ON messages BEGIN
        INSERT INTO messages_fts (messages_fts, rowid, text) SELECT 'delete', old.t, old.text
            WHERE old.text IS NOT NULL AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = old.n);
        DELETE FROM fts_backlog WHERE n = old.n;
    END;
    CREATE TRIGGER messages_fts_update AFTER UPDATE OF text ON messages
    WHEN old.text IS NOT new.text AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = old.n) BEGIN
        INSERT INTO messages_fts (messages_fts, rowid, text) SELECT 'delete', old.t, old.text WHERE old.text IS NOT NULL;
        INSERT INTO messages_fts (rowid, text) SELECT new.t, new.text WHERE new.text IS NOT NULL;
    END;
    CREATE TRIGGER messages_counted AFTER INSERT ON messages BEGIN
        UPDATE chats SET message_count = message_count + 1 WHERE n = new.chat;
    END;
    CREATE TRIGGER messages_uncounted AFTER DELETE ON messages BEGIN
        UPDATE chats SET message_count = message_count - 1 WHERE n = old.chat;
    END;
    CREATE TRIGGER messages_moved AFTER UPDATE OF chat ON messages WHEN new.chat IS NOT old.chat BEGIN
        UPDATE chats SET message_count = message_count - 1 WHERE n = old.chat;
        UPDATE chats SET message_count = message_count + 1 WHERE n = new.chat;
    END;
    UPDATE chats SET message_count = (SELECT count(*) FROM messages WHERE chat = chats.n);

    -- Class: fetched from WhatsApp.
    CREATE TABLE people_rebuilt (
        n INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT,
        push_name TEXT,
        verified_name TEXT,
        about TEXT,
        about_at INTEGER,
        avatar TEXT,
        avatar_id TEXT,
        last_seen INTEGER,
        v INTEGER NOT NULL
    ) STRICT;
    INSERT INTO people_rebuilt (n, name, push_name, verified_name, about, about_at, avatar, avatar_id, last_seen, v)
    SELECT n, name, push_name, verified_name, about, about_at, avatar, avatar_id, last_seen, v FROM people ORDER BY n;
    DROP TABLE people;
    ALTER TABLE people_rebuilt RENAME TO people;
    INSERT INTO sqlite_sequence (name, seq) SELECT 'people', 0
        WHERE NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE name = 'people');
    UPDATE sqlite_sequence SET seq = max(seq, coalesce((SELECT max(key) FROM events WHERE type = 'person'), 0))
        WHERE name = 'people';

    UPDATE chats SET read_at = last_at, read_known = 1, phone_unread = nullif(unread, 0),
        phone_mentions = nullif(mentions, 0)
    WHERE last_message IS NOT NULL;
    ",
];

/// jaw.db's `application_id`, "jaw" and a 1, so another file is never taken for it.
pub(super) const APPLICATION_ID: i64 = 0x6A61_7701;

/// Each migration's number, kind and SHA-256, a line each.
const LOCK: &str = include_str!("migrations.lock");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Costs the same however big the database is.
    Schema,
    /// Costs time in proportion to the rows.
    Data,
}

struct Locked {
    kind: Kind,
    sha256: String,
}

/// The lock's lines in order of number. Every number from 1 has exactly one line, so two branches
/// that each took the next number fail here once both are merged.
fn locked(lock: &str) -> Result<Vec<Locked>> {
    let mut lines: Vec<(usize, Locked)> = Vec::new();
    for line in lock.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [number, kind, sha256] = fields[..] else {
            bail!("migrations.lock's line {line:?} isn't a number, a kind and a SHA-256")
        };
        let number: usize = number.parse().with_context(|| format!("migrations.lock's line {line:?}"))?;
        let kind = match kind {
            "schema" => Kind::Schema,
            "data" => Kind::Data,
            other => bail!("migration {number}'s kind is {other:?}, not schema or data"),
        };
        if lines.iter().any(|(n, _)| *n == number) {
            bail!("two migrations have the number {number}: renumber the newer one as the next free number");
        }
        lines.push((number, Locked { kind, sha256: sha256.to_ascii_lowercase() }));
    }
    lines.sort_by_key(|(n, _)| *n);
    for (at, (number, _)) in lines.iter().enumerate() {
        if *number != at + 1 {
            bail!("migrations.lock has no migration {}", at + 1);
        }
    }
    Ok(lines.into_iter().map(|(_, locked)| locked).collect())
}

fn sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of the schema's text, whitespace aside, so a schema changed by hand, by an edited
/// migration or by another branch's migration of the same number is told apart.
fn fingerprint(db: &Connection) -> Result<String> {
    let mut rows = db.prepare(
        "SELECT type, name, tbl_name, sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
    )?;
    let mut hash = Sha256::new();
    let mut rows = rows.query([])?;
    while let Some(row) = rows.next()? {
        let sql: Option<String> = row.get(3)?;
        let sql = sql.map(|sql| sql.split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_default();
        for field in [row.get::<_, String>(0)?, row.get(1)?, row.get(2)?, sql] {
            hash.update(field.as_bytes());
            hash.update([0]);
        }
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// The fingerprint of the schema the first `version` migrations make.
fn expected(version: usize) -> Result<String> {
    let db = Connection::open_in_memory()?;
    for migration in &MIGRATIONS[..version] {
        db.execute_batch(migration)?;
    }
    fingerprint(&db)
}

/// Refuses a file that isn't jaw's: one with another application's ID, or one with tables and no
/// schema version. A jaw.db from before the ID has none and is given it.
fn mine(db: &Connection, version: usize) -> Result<()> {
    let id: i64 = db.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if id == APPLICATION_ID {
        return Ok(());
    }
    if id != 0 {
        bail!("jaw.db isn't jaw's database: its application_id is {id:#x}");
    }
    if version == 0 {
        let tables: i64 = db.query_row("SELECT count(*) FROM sqlite_schema", [], |r| r.get(0))?;
        if tables > 0 {
            bail!("jaw.db isn't jaw's database: it has {tables} tables and no schema version");
        }
    }
    Ok(())
}

/// The transactions the migrations after `from` run in: each run of schema steps together, and
/// each data step alone.
fn steps(kinds: &[Kind], from: usize) -> Vec<Range<usize>> {
    let mut steps = Vec::new();
    let mut at = from;
    while at < kinds.len() {
        let end = match kinds[at] {
            Kind::Data => at + 1,
            Kind::Schema => (at..kinds.len()).find(|&n| kinds[n] == Kind::Data).unwrap_or(kinds.len()),
        };
        steps.push(at..end);
        at = end;
    }
    steps
}

pub fn migrate(db: &mut Connection) -> Result<()> {
    let locked = locked(LOCK)?;
    if locked.len() != MIGRATIONS.len() {
        bail!("migrations.lock has {} migrations and schema.rs {}", locked.len(), MIGRATIONS.len());
    }
    if let Some(n) = MIGRATIONS.iter().zip(&locked).position(|(migration, locked)| sha256(migration) != locked.sha256) {
        bail!("jaw.db's migration {} isn't the text it was merged with, so this jawd won't run it", n + 1);
    }
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let version = usize::try_from(version).unwrap_or(usize::MAX);
    if version > MIGRATIONS.len() {
        bail!("jaw.db is from a newer jawd (schema {version}, this jawd knows {})", MIGRATIONS.len());
    }
    mine(db, version)?;
    if version > 0 && fingerprint(db)? != expected(version)? {
        bail!(
            "jaw.db's schema isn't the one jaw's migrations make at version {version}, so jawd won't write to it: \
             it was changed by hand, by a migration edited since, or by another build's migration {version}"
        );
    }
    let kinds: Vec<Kind> = locked.iter().map(|l| l.kind).collect();
    for Range { start: at, end } in steps(&kinds, version) {
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (n, migration) in MIGRATIONS.iter().enumerate().take(end).skip(at) {
            tx.execute_batch(migration).with_context(|| format!("jaw.db's migration {}", n + 1))?;
        }
        tx.pragma_update(None, "application_id", APPLICATION_ID)?;
        tx.pragma_update(None, "user_version", end as i64)?;
        if fingerprint(&tx)? != expected(end)? {
            bail!("jaw.db's migrations to version {end} didn't make the schema they make on an empty database");
        }
        tx.commit()?;
    }
    let id: i64 = db.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if id != APPLICATION_ID {
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.pragma_update(None, "application_id", APPLICATION_ID)?;
        tx.commit()?;
    }
    super::check(db, "application_id", &APPLICATION_ID.to_string())?;
    super::check(db, "user_version", &MIGRATIONS.len().to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_run_once_and_refuse_a_newer_database() {
        let mut db = Connection::open_in_memory().unwrap();
        migrate(&mut db).unwrap();
        migrate(&mut db).unwrap();
        let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
        db.pragma_update(None, "user_version", MIGRATIONS.len() as i64 + 1).unwrap();
        assert!(migrate(&mut db).unwrap_err().to_string().contains("newer jawd"));
    }

    #[test]
    fn rebuilding_messages_and_people_keeps_every_row_and_never_reuses_a_key_an_event_names() {
        let mut db = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..5] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", 5).unwrap();
        db.execute_batch(
            "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 1), (2, 'Rui', 1);
            INSERT INTO person_jids (n, jid, person) VALUES (1, 'ana@s.whatsapp.net', 1), (2, 'rui@s.whatsapp.net', 2);
            INSERT INTO chats (n, id, kind, person, unread, mentions, last_at, last_message, v)
                VALUES (1, 'ana@s.whatsapp.net', 'direct', 1, 2, 1, 30, 3, 1);
            INSERT INTO messages (n, chat, id, at, t, sender, kind, text, spans, v)
                VALUES (1, 1, 'a', 10, 10, 1, 'text', 'olá Ana', '[{\"t\":\"text\",\"s\":\"olá Ana\"}]', 1),
                    (2, 1, 'b', 20, 20, 1, 'text', 'second', NULL, 1), (3, 1, 'c', 30, 30, 2, 'text', 'third', NULL, 1);
            INSERT INTO reactions (message, sender, emoji, at) VALUES (1, 2, '👍', 11);
            INSERT INTO events (seq, at, type, key, chat) VALUES (1, 0, 'message', 9, 1), (2, 0, 'person', 7, NULL);",
        )
        .unwrap();
        db.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrate(&mut db).unwrap();
        let count = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(count("SELECT count(*) FROM messages"), 3);
        assert_eq!(
            count("SELECT count(*) FROM reactions"),
            1,
            "foreign keys didn't take the reaction with the old table"
        );
        assert_eq!(count("SELECT message_count FROM chats"), 3);
        assert_eq!(count("SELECT count(*) FROM messages_fts WHERE messages_fts MATCH 'ola'"), 1);
        assert_eq!(count("SELECT read_at FROM chats"), 30);
        assert_eq!(count("SELECT phone_unread + phone_mentions FROM chats"), 3, "the counts it had stand in");
        db.execute(
            "INSERT INTO messages (chat, id, at, t, kind, text, v) VALUES (1, 'd', 40, 40, 'text', 'fourth', 2)",
            [],
        )
        .unwrap();
        assert_eq!(count("SELECT n FROM messages WHERE id = 'd'"), 10, "past the deleted message an event names");
        assert_eq!(count("SELECT message_count FROM chats"), 4, "counted by the trigger");
        assert_eq!(count("SELECT count(*) FROM messages_fts WHERE messages_fts MATCH 'fourth'"), 1);
        db.execute("INSERT INTO people (v) VALUES (2)", []).unwrap();
        assert_eq!(count("SELECT max(n) FROM people"), 8, "past the person an event names");
        let wrong = db
            .execute("INSERT INTO messages (chat, id, at, t, kind, mine, v) VALUES (1, 'e', 50, 50, 'text', 2, 3)", []);
        assert!(wrong.is_err(), "a boolean that isn't one is refused");
    }

    #[test]
    fn a_database_at_every_earlier_version_keeps_its_rows() {
        for from in 1..MIGRATIONS.len() {
            let mut db = Connection::open_in_memory().unwrap();
            for migration in &MIGRATIONS[..from] {
                db.execute_batch(migration).unwrap();
            }
            db.pragma_update(None, "user_version", from as i64).unwrap();
            db.execute("INSERT INTO inbox (kind, at, payload) VALUES ('message', 1, x'01')", []).unwrap();
            db.execute("INSERT INTO history_raw (chat, id, at, raw) VALUES ('c', 'i', 2, x'02')", []).unwrap();
            migrate(&mut db).unwrap();
            let kept: (i64, i64) = db
                .query_row("SELECT (SELECT count(*) FROM inbox), (SELECT count(*) FROM history_raw)", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .unwrap();
            assert_eq!(kept, (1, 1), "from version {from}");
            let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
            assert_eq!(version, MIGRATIONS.len() as i64);
            assert_eq!(fingerprint(&db).unwrap(), expected(MIGRATIONS.len()).unwrap(), "from version {from}");
        }
    }

    #[test]
    fn each_merged_migration_is_still_the_text_it_was_merged_with() {
        let locked = locked(LOCK).unwrap();
        assert_eq!(locked.len(), MIGRATIONS.len(), "each migration has its line in migrations.lock");
        for (n, (migration, locked)) in MIGRATIONS.iter().zip(&locked).enumerate() {
            assert_eq!(
                sha256(migration),
                locked.sha256,
                "migration {} isn't the text it was merged with; a merged migration is never edited, so add a new one",
                n + 1
            );
        }
    }

    #[test]
    fn two_migrations_with_one_number_or_a_gap_are_refused() {
        let line = |n: usize| format!("{n} schema {}\n", "0".repeat(64));
        let twice = [line(1), line(2), line(2)].concat();
        assert!(locked(&twice).err().unwrap().to_string().contains("two migrations have the number 2"));
        let gap = [line(1), line(3)].concat();
        assert!(locked(&gap).err().unwrap().to_string().contains("no migration 2"));
        assert!(locked(&format!("1 later {}", "0".repeat(64))).is_err(), "a kind that isn't schema or data");
        assert_eq!(locked(&[line(2), line(1)].concat()).unwrap().len(), 2, "in any order");
    }

    #[test]
    fn a_data_step_runs_in_a_transaction_of_its_own_between_runs_of_schema_steps() {
        use Kind::{Data, Schema};
        assert_eq!(locked(LOCK).unwrap()[2].kind, Data, "migration 3 counts every chat's messages");
        let kinds = [Schema, Schema, Data, Schema, Schema, Data, Data];
        assert_eq!(steps(&kinds, 0), [0..2, 2..3, 3..5, 5..6, 6..7]);
        assert_eq!(steps(&kinds, 2), [2..3, 3..5, 5..6, 6..7]);
        assert_eq!(steps(&kinds, 4), [4..5, 5..6, 6..7]);
        assert!(steps(&kinds, 7).is_empty());
    }

    #[test]
    fn a_new_database_is_jaws_and_another_file_is_refused() {
        let mut db = Connection::open_in_memory().unwrap();
        migrate(&mut db).unwrap();
        let id: i64 = db.pragma_query_value(None, "application_id", |row| row.get(0)).unwrap();
        assert_eq!(id, APPLICATION_ID);

        let mut other = Connection::open_in_memory().unwrap();
        other.pragma_update(None, "application_id", 0x1234).unwrap();
        assert!(migrate(&mut other).unwrap_err().to_string().contains("isn't jaw's"));
        let mut unversioned = Connection::open_in_memory().unwrap();
        unversioned.execute_batch("CREATE TABLE notes (text TEXT)").unwrap();
        assert!(migrate(&mut unversioned).unwrap_err().to_string().contains("isn't jaw's"));
    }

    #[test]
    fn a_jaw_db_from_before_the_application_id_is_given_it() {
        let mut db = Connection::open_in_memory().unwrap();
        for migration in MIGRATIONS {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", MIGRATIONS.len() as i64).unwrap();
        migrate(&mut db).unwrap();
        let id: i64 = db.pragma_query_value(None, "application_id", |row| row.get(0)).unwrap();
        assert_eq!(id, APPLICATION_ID);
    }

    #[test]
    fn a_schema_changed_by_hand_is_refused() {
        let mut db = Connection::open_in_memory().unwrap();
        migrate(&mut db).unwrap();
        db.execute_batch("CREATE INDEX messages_by_kind ON messages (kind)").unwrap();
        let refused = migrate(&mut db).unwrap_err().to_string();
        assert!(refused.contains("isn't the one jaw's migrations make at version"), "{refused}");
        db.execute_batch("DROP INDEX messages_by_kind").unwrap();
        migrate(&mut db).unwrap();
    }
}
