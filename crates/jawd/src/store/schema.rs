//! jaw.db's schema, as numbered migrations run in order in one transaction at start; `PRAGMA
//! user_version` says how many have run.

use anyhow::{Context, Result, bail};
use rusqlite::Connection;

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
    // 5: the review's rules for what sync writes. The inbox's seq never goes back, so the converter,
    // which reads past the last seq it took, never misses a row once the inbox has drained; a
    // change that fails to convert is kept in quarantine; and a message deleted for me stays deleted.
    "
    -- The inbox's seq never goes back, even once the inbox has drained, so the converter, which
    -- reads past the last seq it took, never misses a row. A rebuild of a small table: nothing
    -- refers to it, and sqlite_sequence follows the rename at the largest seq copied.
    CREATE TABLE inbox_rebuilt (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        kind TEXT NOT NULL,
        at INTEGER NOT NULL,
        payload BLOB NOT NULL
    ) STRICT;
    INSERT INTO inbox_rebuilt (seq, kind, at, payload) SELECT seq, kind, at, payload FROM inbox;
    DROP TABLE inbox;
    ALTER TABLE inbox_rebuilt RENAME TO inbox;

    -- A change that failed, kept with its error and what it came from instead of being dropped:
    -- `source` and `ref` name the raw row it was converted from (`inbox` and its seq, `history_raw`
    -- and its chat and ID), or the change itself, whose raw protobuf is then kept in `raw`.
    CREATE TABLE quarantine (
        n INTEGER PRIMARY KEY,
        at INTEGER NOT NULL,
        source TEXT NOT NULL,
        ref TEXT NOT NULL,
        error TEXT NOT NULL,
        raw BLOB
    ) STRICT;
    CREATE UNIQUE INDEX quarantine_by_ref ON quarantine (source, ref);

    -- Messages deleted for me, so a history copy or a message sent again doesn't bring them back.
    CREATE TABLE tombstones (
        chat INTEGER NOT NULL REFERENCES chats (n) ON DELETE CASCADE,
        id TEXT NOT NULL,
        at INTEGER NOT NULL,
        PRIMARY KEY (chat, id)
    ) STRICT, WITHOUT ROWID;
    ",
];

pub fn migrate(db: &mut Connection) -> Result<()> {
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let version = usize::try_from(version).unwrap_or(usize::MAX);
    if version > MIGRATIONS.len() {
        bail!("jaw.db is from a newer jawd (schema {version}, this jawd knows {})", MIGRATIONS.len());
    }
    if version == MIGRATIONS.len() {
        return Ok(());
    }
    let tx = db.transaction()?;
    for (n, migration) in MIGRATIONS.iter().enumerate().skip(version) {
        tx.execute_batch(migration).with_context(|| format!("jaw.db's migration {}", n + 1))?;
    }
    tx.pragma_update(None, "user_version", MIGRATIONS.len() as i64)?;
    tx.commit()?;
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
        }
    }
}
