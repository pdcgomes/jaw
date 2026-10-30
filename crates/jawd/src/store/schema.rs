//! jaw.db's schema, as numbered migrations run in order at start; `PRAGMA user_version` says how
//! many have run. A merged migration is frozen: `migrations.lock` has a line for each, its number,
//! its kind and the SHA-256 of its text, which a test checks. Consecutive schema steps run in one
//! IMMEDIATE transaction, and a data step, whose cost grows with the rows, in one of its own. The
//! schema must be the one the migrations make, before and after, or jawd refuses to write.

use std::ffi::CString;
use std::fs::File;
use std::ops::Range;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::fallible_iterator::FallibleIterator;
use rusqlite::{Batch, Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};

/// jaw.db's migrations, in order. A table's class says what may be done to it (see "Migrations" in
/// the design); migrations from 5 on declare it beside each table, and those made before are:
/// - raw: `inbox`, `history_raw`, `app_state` (the app state not applied yet), `pending` (what
///   waits for a message, with its raw);
/// - fetched from WhatsApp: `person_jids`, `group_members`, `poll_options`, and `media`'s
///   description of each file (its `path`, `state`, `progress` and previews are jaw's own);
/// - jaw's own: `folders`, `folder_items`, `outbox`, `sync_state`, and `reactions`, `receipts` and
///   `poll_votes`, which keep no raw once applied, so can't be rebuilt from it;
/// - derived: `events` (the feed), `fts_backlog` and `messages_fts` (search).
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
    // reference Mac, its commit and foreign_key_check included. chats is rebuilt first, the same
    // way, since a chat merged into another is removed and the feed names chats by n; first,
    // because the triggers on messages name it, and a table can't be renamed into place while a
    // trigger names one that isn't there.
    "
    -- Class: fetched from WhatsApp, with jaw's own draft, frecency and opened_at, and its counts
    -- derived from messages.
    CREATE TABLE chats_rebuilt (
        n INTEGER PRIMARY KEY AUTOINCREMENT,
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
        v INTEGER NOT NULL,
        message_count INTEGER NOT NULL DEFAULT 0,
        sort TEXT GENERATED ALWAYS AS
            (iif(pinned > 0, '1~' || printf('%013d', pinned), '0~' || printf('%013d', last_at))) VIRTUAL,
        avatar_id TEXT,
        read_at INTEGER,
        read_known INTEGER NOT NULL DEFAULT 0,
        phone_unread INTEGER,
        phone_mentions INTEGER
    );
    INSERT INTO chats_rebuilt (n, id, kind, name, person, community, pinned, archived, muted_until, marked_unread,
        unread, mentions, ephemeral, draft, avatar, frecency, opened_at, last_at, last_message, v, message_count,
        avatar_id, read_at, read_known, phone_unread, phone_mentions)
    SELECT n, id, kind, name, person, community, pinned, archived, muted_until, marked_unread, unread, mentions,
        ephemeral, draft, avatar, frecency, opened_at, last_at, last_message, v, message_count, avatar_id, read_at,
        read_known, phone_unread, phone_mentions
    FROM chats ORDER BY n;
    DROP TABLE chats;
    ALTER TABLE chats_rebuilt RENAME TO chats;
    INSERT INTO sqlite_sequence (name, seq) SELECT 'chats', 0
        WHERE NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE name = 'chats');
    UPDATE sqlite_sequence SET seq = max(seq, coalesce((SELECT max(key) FROM events WHERE type = 'chat'), 0),
        coalesce((SELECT max(chat) FROM events), 0))
        WHERE name = 'chats';
    CREATE INDEX chats_by_person ON chats (person) WHERE person IS NOT NULL;
    CREATE INDEX chats_by_sort ON chats (archived, sort, n);

    -- Class: derived from raw, except what amendments did to it (edits, deletions for everyone,
    -- receipts' status, stars), which keeps no raw once applied: a rebuild from raw alone would
    -- lose that, so until amendments keep theirs, take it as jaw's own.
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
    // 7: sending, a schema step: the outbox rebuilt beside the messages it sends, which nothing
    // ever wrote to before, and how far WhatsApp has been told each chat is read, as columns of
    // jaw's own on chats beside where sync keeps the read position. Adding nullable columns costs
    // the same however many chats there are; their CHECK reads each row once, all NULL.
    "
    -- Class: jaw's own. What's waiting to be sent, each beside its message row, which shows it as
    -- pending; a row goes once the server has the message. Migration 2's outbox named its chat by
    -- JID and nothing ever wrote to it; a row of it naming no message jaw has is kept, to send.
    CREATE TABLE outbox_rebuilt (
        n INTEGER PRIMARY KEY AUTOINCREMENT,
        -- The message row that shows it; NULL once it's deleted for you, which, as on the phone,
        -- doesn't stop it going.
        message INTEGER UNIQUE REFERENCES messages (n) ON DELETE SET NULL,
        -- Where it goes and the ID it goes with, kept for when its row is gone.
        jid TEXT NOT NULL,
        id TEXT NOT NULL,
        kind TEXT NOT NULL,
        -- WAWebProtobufsE2E.Message, as it's sent.
        raw BLOB NOT NULL,
        -- queued: sent as soon as its chat's earlier sends have gone and WhatsApp can be reached;
        -- unconfirmed: refused when it may already have gone, so waiting for a receipt until
        -- next_at; failed: waiting for send.retry.
        state TEXT NOT NULL DEFAULT 'queued' CHECK (state IN ('queued', 'unconfirmed', 'failed')),
        attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
        next_at INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        error TEXT
    ) STRICT;
    INSERT INTO outbox_rebuilt (message, jid, id, kind, raw, state, attempts, next_at, created_at, error)
        SELECT (SELECT m.n FROM chats c JOIN messages m ON m.chat = c.n WHERE c.id = o.chat AND m.id = o.id),
            o.chat, o.id, o.kind, o.raw, o.state, o.attempts, o.next_at, o.created_at, o.error
        FROM outbox o ORDER BY o.created_at;
    DROP TABLE outbox;
    ALTER TABLE outbox_rebuilt RENAME TO outbox;

    -- Class: jaw's own columns on chats, which is otherwise fetched from WhatsApp, as draft and
    -- frecency are; any rebuild of chats copies them.
    -- `read_n`: with `read_at`, where the chat was read up to as a message's place (at, n). NULL
    -- when it was said as a time, which reads every message up to `read_at`, however many share
    -- its millisecond.
    -- `told_at` and `told_n`: how far WhatsApp has been told the chat is read; the read receipts
    -- owed are for the messages from others after it, up to the position. NULL while WhatsApp
    -- knows as much as jaw: up to the position, but for the phone's own unread messages while its
    -- counts stand.
    ALTER TABLE chats ADD COLUMN read_n INTEGER;
    ALTER TABLE chats ADD COLUMN told_at INTEGER;
    ALTER TABLE chats ADD COLUMN told_n INTEGER CHECK ((told_n IS NULL) = (told_at IS NULL) AND (told_n IS NULL
        OR read_at IS NOT NULL AND (told_at, told_n) <= (read_at, coalesce(read_n, 9223372036854775807))));
    ",
    // 8: sending's other tables, a schema step. All jaw's own: never dropped or rebuilt from raw,
    // and copied as they are by any rebuild.
    "
    -- Who a message of yours in a group went to: the group's members when you sent it, you apart.
    -- It's delivered once every one of them who's still a member has it, and read once every one
    -- has read it; members who joined after don't count, and those who left don't hold it up. A
    -- message's rows are let go of once it has got as far as it can, read or, for a voice message,
    -- played (settled, below). So message.info's list of who had it and read it when comes from
    -- receipts, which last as long as the message, never from here.
    CREATE TABLE recipients (
        message INTEGER NOT NULL REFERENCES messages (n) ON DELETE CASCADE,
        member INTEGER NOT NULL REFERENCES person_jids (n),
        -- The message's chat, for finding a member's messages in a group; a chat's rows go with its
        -- messages, so it names chats without a key of its own.
        chat INTEGER NOT NULL,
        PRIMARY KEY (message, member)
    ) STRICT, WITHOUT ROWID;
    -- By member first, so a foreign key's look from person_jids is a search too.
    CREATE INDEX recipients_by_member ON recipients (member, chat, message);

    -- Messages of yours that have got as far as they can, whose recipients are still to be let go
    -- of, 128 at a time in the background: one message's rows touch a page of
    -- recipients_by_member for every few members, too many for one short transaction in a big group.
    CREATE TABLE settled (
        message INTEGER PRIMARY KEY REFERENCES messages (n) ON DELETE CASCADE
    ) STRICT;

    -- Members who left a group, whose messages from you there are looked at again in the
    -- background, a few at a time, from after the last one looked at.
    CREATE TABLE regroup (
        chat INTEGER NOT NULL REFERENCES chats (n) ON DELETE CASCADE,
        member INTEGER NOT NULL REFERENCES person_jids (n),
        after INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (chat, member)
    ) STRICT, WITHOUT ROWID;
    CREATE INDEX regroup_by_member ON regroup (member);

    -- The key an interface gave each message it sent, for a day, so asking again with the same key
    -- answers with the same message instead of sending a second; kept with the message's chat and
    -- ID once it's deleted for you, since the message still goes.
    CREATE TABLE send_keys (
        key TEXT PRIMARY KEY CHECK (length(key) BETWEEN 1 AND 128),
        message INTEGER REFERENCES messages (n) ON DELETE SET NULL,
        jid TEXT NOT NULL,
        id TEXT NOT NULL,
        at INTEGER NOT NULL
    ) STRICT, WITHOUT ROWID;
    CREATE INDEX send_keys_by_at ON send_keys (at);
    CREATE INDEX send_keys_by_message ON send_keys (message);
    ",
    // 9: actions of yours on messages, a schema step: two nullable columns on the outbox, which
    // holds at most a thousand rows, their CHECK reading each once, and indexes on it.
    "
    -- Class: jaw's own columns on the outbox. A reaction, an edit or a deletion for everyone goes
    -- to WhatsApp as a message of its own, with an ID of its own, and is done to the message's row
    -- only once WhatsApp has it; until then windows show it from here. A deletion for you is done
    -- at once, and its row here tells your other devices, by app state.
    -- `target`: the message an action is done to; NULL once that message is deleted for you, which,
    -- as on the phone, doesn't stop the action going.
    -- `effect`: what the action does to its message once WhatsApp has it, the writer's amendment
    -- as JSON; NULL for a message to send and for app state.
    ALTER TABLE outbox ADD COLUMN target INTEGER REFERENCES messages (n) ON DELETE SET NULL;
    ALTER TABLE outbox ADD COLUMN effect TEXT CHECK (json_valid(effect));
    -- Each message's actions, for windows and for the foreign key; and a receipt names an action
    -- by its ID.
    CREATE INDEX outbox_by_target ON outbox (target);
    CREATE INDEX outbox_by_id ON outbox (id);
    ",
    // 10, a schema step: the failed messages of yours an earlier run left in the outbox that
    // nothing can send again go, as the writer removes each from now on in the transaction that
    // makes it so. It reads only the outbox, which holds at most a thousand rows waiting, and for
    // each failed one its message by n and the outbox by target, so it costs the same however many
    // messages there are: 2.1 ms on the reference Mac for a full outbox of failed rows beside
    // 200,000 messages, and 2.2 ms beside a million. It drops no table, and nothing refers to the
    // outbox, so it needs neither a data step's backup nor its check of every foreign key.
    "
    -- Class: jaw's own rows of the outbox, none of which send.retry could reach or would send: a
    -- failed message deleted for you, whose row has no message, deleted for everyone, or with your
    -- deletion of it kept for a late receipt. Each held a place in the outbox's thousand for good.
    DELETE FROM outbox WHERE state = 'failed' AND kind NOT IN ('reaction', 'edit', 'revoke', 'app_state')
        AND (message IS NULL
            OR EXISTS (SELECT 1 FROM messages m WHERE m.n = outbox.message AND m.deleted)
            OR EXISTS (SELECT 1 FROM outbox a WHERE a.target = outbox.message AND a.kind = 'revoke'
                AND a.state = 'failed'));
    ",
    // 11, a data step: the (chat, time, rowid) keyset index holds each message's mine, kind and
    // mentions_me after its key, so a chat's unread messages and mentions, counted again before
    // every transaction that changes them commits, are counted from the index alone rather than
    // from up to a thousand of the chat's messages, and so are the receipts a read owes. It replaces
    // the index rather than adding one, so a message written dirties no page more. Measured as jawd
    // runs it, on its writer's connection, at 0.93 to 1.46 s per million messages on the reference
    // Mac, its foreign_key_check, fingerprint and commit included, at a load of 27 to 30; the backup
    // before it took 2.2 to 3.1 s of a 595 MB jaw.db. A plain connection, with SQLite's 2 MB cache
    // and a checkpoint in its commit, takes 5 to 8.4 s.
    "
    -- Class: an index, derived from messages' own columns; it holds nothing they don't, so it can
    -- always be built again from them.
    DROP INDEX messages_by_time;
    -- n is the rowid every entry ends with; naming it keeps the keyset's order with what the
    -- counts read after it.
    CREATE INDEX messages_by_time ON messages (chat, at, n, mine, kind, mentions_me);
    ",
    // 12: what interfaces keep of how they were laid out, a schema step: one new table, which costs
    // the same however big jaw.db is, and which nothing refers to.
    "
    -- Class: jaw's own. What each interface keeps of how it was laid out, by the name it gives in
    -- hello (jaw for the terminal) and what it is: under 'layout', its split panes, in the format
    -- jaw-proto's layout module gives. A row is replaced only by one with a newer version, which
    -- the interface gives, so asking again, or late, changes nothing. Never dropped or rebuilt; any
    -- rebuild copies it as it is. jawd keeps 16 rows at most.
    CREATE TABLE interface_state (
        interface TEXT NOT NULL CHECK (octet_length(interface) BETWEEN 1 AND 64),
        key TEXT NOT NULL CHECK (octet_length(key) BETWEEN 1 AND 64),
        value TEXT NOT NULL CHECK (json_valid(value) AND octet_length(value) <= 16384),
        v INTEGER NOT NULL CHECK (v > 0),
        -- When the row was last replaced, in milliseconds since the epoch.
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (interface, key)
    ) STRICT, WITHOUT ROWID;
    ",
    // 13, a schema step: each frontend's settings, one new table, which costs the same however big
    // jaw.db is.
    "
    -- Class: jaw's own. Each frontend's settings, in a domain named by the client its hello gives, a
    -- row for each setting chosen: never dropped or rebuilt, and copied as they are by any rebuild. A
    -- setting without a row has its default. `value` is the setting's JSON, and `at` when it was
    -- chosen, in milliseconds.
    CREATE TABLE settings (
        domain TEXT NOT NULL CHECK (octet_length(domain) BETWEEN 1 AND 64),
        key TEXT NOT NULL CHECK (octet_length(key) BETWEEN 1 AND 64),
        value TEXT NOT NULL CHECK (json_valid(value) AND octet_length(value) <= 4096),
        at INTEGER NOT NULL,
        PRIMARY KEY (domain, key)
    ) STRICT, WITHOUT ROWID;
    ",
    // 14, a schema step: links. Each URL a message gives is kept in `links`, with its host, its
    // preview's title and description, and the message's chat, time and sender, by triggers on
    // messages that read what the converter made of the message, so every write that changes a
    // message (a copy arriving, an edit, a deletion, a merge moving it) changes its links in the
    // same statement; and a links index for search keeps one document per message, a new
    // message's waiting with its text in fts_backlog to be indexed in the writer's steps of its own.
    // The messages jaw.db already holds are linked by a backfill, a short background transaction at
    // a time from the row it leaves in `backfills`. It makes new tables, views, triggers and one
    // row, so it costs the same however many messages there are.
    "
    -- Class: derived from raw. Each URL a message's spans mark, once, in the order it first comes,
    -- then its link preview's when the spans don't mark it, each made absolute as the spans make
    -- one (`https://` before a bare one) and with the preview's title and description on the URL
    -- it's for; and the message's chat, time, sender and whether it's yours, which follow it. The
    -- triggers below write and keep them; nothing else does.
    CREATE TABLE links (
        n INTEGER PRIMARY KEY AUTOINCREMENT,
        message INTEGER NOT NULL REFERENCES messages (n) ON DELETE CASCADE,
        -- Its place among the message's links, from 0.
        place INTEGER NOT NULL CHECK (place >= 0),
        -- The message's chat, as recipients names it: a chat's links go with its messages.
        chat INTEGER NOT NULL,
        at INTEGER NOT NULL,
        sender INTEGER REFERENCES person_jids (n),
        mine INTEGER NOT NULL CHECK (mine IN (0, 1)),
        url TEXT NOT NULL,
        host TEXT NOT NULL,
        title TEXT,
        description TEXT,
        UNIQUE (message, url),
        UNIQUE (message, place)
    ) STRICT;
    -- Newest first across chats, in one chat and from one person, as the library pages them; each
    -- entry ends with the rowid, so each is a keyset.
    CREATE INDEX links_by_time ON links (at);
    CREATE INDEX links_by_chat ON links (chat, at);
    CREATE INDEX links_by_sender ON links (sender, at) WHERE sender IS NOT NULL;

    -- What `links` is made of, each view read for one message, or a range of them, through the
    -- messages' primary key: the URLs a message's spans mark, with each one's place among its
    -- spans; its link preview's URL; and its URLs, each once in the order it first comes, the
    -- preview's last unless the spans mark it, with whether the preview is for it.
    CREATE VIEW link_spans (message, key, url) AS
    SELECT m.n, e.key, iif(lower(substr(e.value ->> 'url', 1, 7)) = 'http://'
            OR lower(substr(e.value ->> 'url', 1, 8)) = 'https://', e.value ->> 'url', 'https://' || (e.value ->> 'url'))
    FROM messages m, json_each(m.spans) e
    WHERE NOT m.deleted AND e.value ->> 't' = 'link';
    CREATE VIEW link_previews (message, url) AS
    SELECT m.n, iif(lower(substr(m.detail ->> '$.link.url', 1, 7)) = 'http://'
            OR lower(substr(m.detail ->> '$.link.url', 1, 8)) = 'https://', m.detail ->> '$.link.url',
            'https://' || (m.detail ->> '$.link.url'))
    FROM messages m
    WHERE NOT m.deleted AND m.detail ->> '$.link.url' IS NOT NULL;
    CREATE VIEW message_urls (message, place, url, previewed) AS
    SELECT s.message, (SELECT count(DISTINCT d.url) FROM link_spans d WHERE d.message = s.message AND d.key < s.key),
        s.url, EXISTS (SELECT 1 FROM link_previews p WHERE p.message = s.message AND p.url = s.url)
    FROM link_spans s
    WHERE NOT EXISTS (SELECT 1 FROM link_spans d WHERE d.message = s.message AND d.key < s.key AND d.url = s.url)
    UNION ALL
    SELECT p.message, (SELECT count(DISTINCT d.url) FROM link_spans d WHERE d.message = p.message), p.url, 1
    FROM link_previews p
    WHERE NOT EXISTS (SELECT 1 FROM link_spans d WHERE d.message = p.message AND d.url = p.url);
    -- A message's links as `links` keeps them: each URL's host is what comes between its scheme and
    -- its path (r, what follows the scheme; a, the authority; h, it without a user), without a
    -- port, in lower case. Each step is the one value of a json_each, which the next reads as a
    -- column: as subqueries, SQLite would copy each step's expression into every use of it, and
    -- the host's into thousands of instructions.
    CREATE VIEW message_links (message, place, chat, at, sender, mine, url, host, title, description) AS
    SELECT u.message, u.place, m.chat, m.at, m.sender, m.mine, u.url,
        lower(CASE WHEN h.value LIKE '[%' THEN substr(h.value, 1, instr(h.value, ']'))
            WHEN instr(h.value, ':') > 0 THEN substr(h.value, 1, instr(h.value, ':') - 1) ELSE h.value END),
        iif(u.previewed, m.detail ->> '$.link.title', NULL), iif(u.previewed, m.detail ->> '$.link.description', NULL)
    FROM message_urls u JOIN messages m ON m.n = u.message,
        json_each(json_array(substr(u.url, instr(u.url, '://') + 3))) r,
        json_each(json_array(substr(r.value, 1,
            min(instr(r.value || '/', '/'), instr(r.value || '?', '?'), instr(r.value || '#', '#')) - 1))) a,
        json_each(json_array(substr(a.value, instr(a.value, '@') + 1))) h;

    -- Class: derived from raw. The words of each message's links, their hosts, titles, descriptions
    -- and URLs, as one document by the message's t, so search reads them newest first and stops at
    -- a page. It's contentless, and a message has one document at most: one waiting in fts_backlog
    -- has none, and is indexed with its text, its links' words and all, by the writer's steps; the
    -- triggers below write and delete the documents of the rest, and the links backfill those of
    -- the messages it links. Its segments are merged as messages_fts's are, in the writer's idle
    -- moments rather than at the commit of whichever transaction fills a level.
    CREATE VIRTUAL TABLE links_fts USING fts5 (
        text,
        content = '',
        contentless_delete = 1,
        tokenize = 'unicode61 remove_diacritics 2'
    );
    INSERT INTO links_fts (links_fts, rank) VALUES ('automerge', 0);
    INSERT INTO links_fts (links_fts, rank) VALUES ('crisismerge', 64);
    CREATE VIEW message_link_text (message, t, text) AS
    SELECT m.n, m.t, (SELECT group_concat(concat_ws(' ', l.host, l.title, l.description, l.url), ' ')
        FROM links l WHERE l.message = m.n)
    FROM messages m;

    -- Writing a message's n here writes its links and, unless it waits in fts_backlog, its
    -- document, which an INSTEAD OF trigger does, so the program reading the views runs only for a
    -- message that may have links: a trigger's whole program is set up each time it fires, before
    -- its WHEN is looked at, and the triggers on messages fire for every message.
    CREATE VIEW links_derived (message) AS SELECT NULL WHERE 0;
    CREATE TRIGGER links_derived_insert INSTEAD OF INSERT ON links_derived BEGIN
        INSERT INTO links (message, place, chat, at, sender, mine, url, host, title, description)
            SELECT message, place, chat, at, sender, mine, url, host, title, description FROM message_links
            WHERE message = new.message;
        INSERT INTO links_fts (rowid, text)
            SELECT t, text FROM message_link_text WHERE message = new.message AND text IS NOT NULL
                AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = new.message);
    END;
    CREATE TRIGGER messages_links_insert AFTER INSERT ON messages
    WHEN instr(new.spans, '\"t\":\"link\"') > 0 OR instr(new.detail, '\"link\":{') > 0 BEGIN
        INSERT INTO links_derived (message) VALUES (new.n);
    END;
    CREATE TRIGGER messages_links_update AFTER UPDATE OF chat, at, sender, mine, spans, detail, deleted ON messages
    WHEN (old.chat IS NOT new.chat OR old.at IS NOT new.at OR old.sender IS NOT new.sender
            OR old.mine IS NOT new.mine OR old.spans IS NOT new.spans OR old.detail IS NOT new.detail
            OR old.deleted IS NOT new.deleted)
        AND (instr(old.spans, '\"t\":\"link\"') > 0 OR instr(old.detail, '\"link\":{') > 0
            OR instr(new.spans, '\"t\":\"link\"') > 0 OR instr(new.detail, '\"link\":{') > 0) BEGIN
        DELETE FROM links WHERE message = old.n;
        DELETE FROM links_fts WHERE rowid = old.t AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = old.n);
        INSERT INTO links_derived (message) VALUES (new.n);
    END;
    -- A message deleted while it waits in fts_backlog has no document, whichever of this and
    -- messages_fts_delete, which takes it out of the backlog, goes first: deleting a rowid a
    -- contentless index doesn't hold changes nothing it holds.
    CREATE TRIGGER messages_links_delete AFTER DELETE ON messages
    WHEN instr(old.spans, '\"t\":\"link\"') > 0 OR instr(old.detail, '\"link\":{') > 0 BEGIN
        DELETE FROM links WHERE message = old.n;
        DELETE FROM links_fts WHERE rowid = old.t AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = old.n);
    END;

    -- Class: jaw's own. Work a migration leaves to the background, a short transaction at a time,
    -- until it's done and its row goes: each backfill's name, the last message it has looked at,
    -- and the last it has to, the newest when the migration ran, since the triggers take the rest.
    CREATE TABLE backfills (
        name TEXT PRIMARY KEY,
        after INTEGER NOT NULL DEFAULT 0 CHECK (after >= 0),
        upto INTEGER NOT NULL CHECK (upto >= after)
    ) STRICT, WITHOUT ROWID;
    INSERT INTO backfills (name, after, upto) SELECT 'links', 0, seq FROM sqlite_sequence
        WHERE name = 'messages' AND seq > 0;

    -- Class: jaw's own. The thumbnails of link previews written to media/links/ as `<sha256>.jpg`,
    -- each named by the SHA-256 of the JPEG that came in the message or was made for one sent,
    -- synced and renamed into place before its row goes in, so every row here has its whole file;
    -- a preview shows its thumbnail once it has a row, and a file goes only after its row has.
    CREATE TABLE thumbnails (
        sha256 TEXT PRIMARY KEY CHECK (length(sha256) = 64),
        bytes INTEGER NOT NULL CHECK (bytes > 0),
        at INTEGER NOT NULL
    ) STRICT, WITHOUT ROWID;
    ",
    // 15, a schema step: votes in polls. Each voter's latest vote is kept beside what it chose, so a
    // vote older than it never replaces it, one that chose nothing among them, and each option's
    // votes and the votes waiting for a poll are counted from indexes. Nothing wrote poll_votes
    // before, so it's empty, and pending holds what waits for a message jaw hasn't seen yet, a few
    // rows, so its index costs the same however many messages there are. The polls jaw.db already
    // holds get their options, and how many a vote may choose, from a backfill that reads their raw
    // protobufs.
    "
    -- Class: jaw's own, as poll_votes is, which keeps no raw once applied: each voter's latest vote
    -- in each poll, by its sender's time and then its message's ID, which a vote older than it never
    -- replaces; one that chose nothing is kept here too. poll_votes holds what it chose.
    CREATE TABLE poll_voters (
        message INTEGER NOT NULL REFERENCES messages (n) ON DELETE CASCADE,
        voter INTEGER NOT NULL REFERENCES person_jids (n),
        at INTEGER NOT NULL,
        id TEXT NOT NULL,
        PRIMARY KEY (message, voter)
    ) STRICT, WITHOUT ROWID;
    CREATE INDEX poll_voters_by_voter ON poll_voters (voter);
    -- Each option's votes, counted from the index.
    CREATE INDEX poll_votes_by_option ON poll_votes (message, position);
    -- The votes waiting for a poll jaw hasn't seen, counted, and the oldest found, from the index.
    CREATE INDEX pending_by_kind ON pending (kind, n);
    -- The polls among the messages jaw.db already holds.
    INSERT INTO backfills (name, after, upto) SELECT 'polls', 0, seq FROM sqlite_sequence
        WHERE name = 'messages' AND seq > 0;
    ",
    // 16, a schema step: the messages jawd couldn't show when they came, of kind unknown or saying
    // so with no text whatever their kind, are converted again from their raw protobufs by a
    // backfill, now that jawd knows community events and the other kinds it didn't; one that
    // changes another message, an encrypted edit or an answer to an event, goes to wait in
    // `pending`. It leaves one row in `backfills`, jaw's own, so it costs the same however many
    // messages there are.
    "
    INSERT INTO backfills (name, after, upto) SELECT 'kinds', 0, seq FROM sqlite_sequence
        WHERE name = 'messages' AND seq > 0;
    ",
    // 17, a schema step: what an interface keeps of how it was laid out takes 64 KiB of JSON rather
    // than 16, so a book of layouts on a grid fits, the Mac app's, which at its limits (the unnamed
    // layout and nine named, each named one with its saved and its edited shape, 12 slots in each)
    // takes about 30 KiB. A CHECK can't be changed in place, so the table is rebuilt and every row
    // copied as it is, as migration 7 rebuilt the outbox. A rebuild of a small table: nothing refers
    // to it, it has no trigger or index of its own, and it holds 16 rows at most.
    "
    -- Class: jaw's own, as migration 12 made it. What each interface keeps of how it was laid out,
    -- by the name it gives in hello (jaw for the terminal) and what it is: under 'layout', a tree
    -- of split panes or a book of layouts on a grid, in the formats jaw-proto's layout module
    -- gives, whose limits jawd checks before it writes one. A row is replaced only by one with a
    -- newer version, which the interface gives, so asking again, or late, changes nothing. Never
    -- dropped; a rebuild, as this one is, copies it as it is. jawd keeps 16 rows at most.
    CREATE TABLE interface_state_rebuilt (
        interface TEXT NOT NULL CHECK (octet_length(interface) BETWEEN 1 AND 64),
        key TEXT NOT NULL CHECK (octet_length(key) BETWEEN 1 AND 64),
        value TEXT NOT NULL CHECK (json_valid(value) AND octet_length(value) <= 65536),
        v INTEGER NOT NULL CHECK (v > 0),
        -- When the row was last replaced, in milliseconds since the epoch.
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (interface, key)
    ) STRICT, WITHOUT ROWID;
    INSERT INTO interface_state_rebuilt (interface, key, value, v, updated_at)
        SELECT interface, key, value, v, updated_at FROM interface_state;
    DROP TABLE interface_state;
    ALTER TABLE interface_state_rebuilt RENAME TO interface_state;
    ",
    // 18, a schema step: a column with a constant default on the outbox, which holds at most two
    // thousand rows (a thousand waiting and a thousand actions kept for a late receipt), its CHECK
    // reading each once. Rows from before take 0, as nothing is left to say whether a message
    // already deleted for you had been deleted for everyone, so their actions go as they would have.
    "
    -- Class: jaw's own column on the outbox.
    -- `target_deleted`: 1 for an action whose message had been deleted for everyone when it was
    -- deleted for you, kept as `target` goes NULL, since a reaction or an edit on a message deleted
    -- for everyone isn't sent, while one on a message only deleted for you goes. While `target`
    -- names the message, the message's own row says.
    ALTER TABLE outbox ADD COLUMN target_deleted INTEGER NOT NULL DEFAULT 0 CHECK (target_deleted IN (0, 1));
    ",
    // 19, a data step: folders and the roster. Migration 2's folders and folder_items, which nothing
    // ever wrote to, are rebuilt as STRICT tables with every row copied, folders with AUTOINCREMENT
    // since interfaces hold their IDs, and folder_items without its position, which nothing wrote
    // either; the built-in folders and the three smart ones jaw starts with are rows of it;
    // favourites hold folders or chats; and each folder's unread messages, mentions and count are
    // kept by triggers through the chats and people each holds, so a message arriving counts in
    // every folder it's in without any folder's chats being walked. People gain their presence,
    // which rosters sort by, how many chats are theirs, and a version of what only their roster
    // row shows, which their presence moves and their v doesn't. Communities, whose rows now show
    // their groups', and the chats filed, whose rows say where, take new versions with their
    // events. It's a data step because it rebuilds folders, which other tables refer to, so its
    // foreign keys are checked before it commits. It reads each chat and person a few times and no
    // message: measured as jawd runs it, on its writer's connection, at 0.37 to 0.49 s beside a
    // million messages, 3,000 chats and 5,000 people with 30 folders holding 600 of them, its
    // foreign_key_check, fingerprint and commit included, on the reference Mac at a load of 78 to
    // 99; the backup before it took 2.1 to 4.0 s (`measure_the_folders_migration`).
    FOLDERS_AND_THE_ROSTER,
    // 20, a schema step: media in. `media` is rebuilt as a STRICT table with what downloading a copy
    // needs, and three tables join it: the cache's files by content hash, the cache's size, and the
    // files no copy names any more. No jawd before this one wrote a row to `media`, so the copy reads
    // an empty table and costs the same however big jaw.db is; nothing refers to `media`, so dropping
    // the old table leaves no row elsewhere pointing at nothing. The messages jaw.db already holds
    // get their files' copies from their raw protobufs by a backfill, which it leaves a row in
    // `backfills` for. At a million messages, at a load of 55 to 70, opening jaw.db and this step took
    // 50 to 430 ms, and its backfill 38 to 60 s with one message in ten a photo and 78 to 107 s for
    // 300,000 photos, each step 0.33 to 0.42 ms at p50 and 1.0 to 1.7 ms at p99 with its commit
    // (`measure_the_copies_backfill`).
    "
    -- Class: fetched from WhatsApp, for each copy of a message's file as WhatsApp describes it, with
    -- the keys to download it, kept here since an HD copy's come in a message of its own that isn't
    -- kept as a row; and jaw's own columns, for what jawd is doing with it: `state`, `progress`,
    -- `error`, `wanted` (when it was given something to do: an arrival's thumbnail and download, an
    -- interface asking), `asked` (an interface asked), `arrived` (wanted as its message arrived
    -- live), `retried_at` (when the phone was asked to upload it again), `cancelled`, and
    -- `direct_path` once the phone has. Its file and previews are the cache's, by `sha256`
    -- (media_files).
    CREATE TABLE media_rebuilt (
        message INTEGER NOT NULL REFERENCES messages (n) ON DELETE CASCADE,
        variant TEXT NOT NULL CHECK (variant IN ('sd', 'hd')),
        -- Which of WhatsApp's keys it's encrypted with; a sticker's are a photo's.
        keys TEXT NOT NULL DEFAULT 'document' CHECK (keys IN ('image', 'video', 'audio', 'document')),
        mime TEXT,
        bytes INTEGER CHECK (bytes >= 0),
        w INTEGER CHECK (w >= 0),
        h INTEGER CHECK (h >= 0),
        seconds REAL CHECK (seconds >= 0),
        name TEXT,
        sha256 BLOB CHECK (length(sha256) = 32),
        enc_sha256 BLOB CHECK (length(enc_sha256) = 32),
        media_key BLOB CHECK (length(media_key) = 32),
        direct_path TEXT,
        -- Whether its message carries a thumbnail to draw until the file comes.
        thumbnail INTEGER NOT NULL DEFAULT 0 CHECK (thumbnail IN (0, 1)),
        state TEXT NOT NULL DEFAULT 'remote'
            CHECK (state IN ('remote', 'downloading', 'expired', 'waiting_for_phone', 'gone')),
        progress REAL CHECK (progress BETWEEN 0 AND 1),
        error TEXT,
        wanted INTEGER,
        asked INTEGER NOT NULL DEFAULT 0 CHECK (asked IN (0, 1)),
        -- Wanted because its message arrived live, which is done ahead of what windows show; it
        -- goes with the want.
        arrived INTEGER NOT NULL DEFAULT 0 CHECK (arrived IN (0, 1)),
        retried_at INTEGER,
        -- An interface cancelled its download, so it's fetched again only when asked for.
        cancelled INTEGER NOT NULL DEFAULT 0 CHECK (cancelled IN (0, 1)),
        PRIMARY KEY (message, variant),
        -- A copy downloading is one jawd was given to do, so a start finds those a stop left by
        -- `wanted`; one waiting for the phone says since when.
        CHECK (state <> 'downloading' OR wanted IS NOT NULL),
        CHECK (state <> 'waiting_for_phone' OR retried_at IS NOT NULL),
        CHECK (arrived = 0 OR wanted IS NOT NULL)
    ) STRICT, WITHOUT ROWID;
    INSERT INTO media_rebuilt (message, variant, mime, bytes, w, h, seconds, name, sha256, state)
    SELECT message, variant, mime, bytes, w, h, seconds, name, iif(length(sha256) = 32, sha256, NULL),
        iif(state IN ('expired', 'gone'), state, 'remote')
    FROM media WHERE variant IN ('sd', 'hd');

    -- Class: jaw's own. Each file the cache holds, by the SHA-256 of its content, which names it and
    -- every file made from it: the file once it's whole and synced in the cache, and its previews and
    -- its message's thumbnail drawn as one, which can be made again. `stored` counts every byte of
    -- them on disk; `used_at` is when one was last placed, asked for, saved or shown, so the cache
    -- drops what was used longest ago first. A row's paths, relative to jaw's folder, name files that
    -- are there; a file goes only after a synced commit has removed its path.
    CREATE TABLE media_files (
        sha256 BLOB PRIMARY KEY CHECK (length(sha256) = 32),
        path TEXT,
        bytes INTEGER CHECK (bytes >= 0),
        small TEXT,
        large TEXT,
        previews_bytes INTEGER CHECK (previews_bytes >= 0),
        thumbnail TEXT,
        thumbnail_bytes INTEGER CHECK (thumbnail_bytes >= 0),
        stored INTEGER NOT NULL
            GENERATED ALWAYS AS (coalesce(bytes, 0) + coalesce(previews_bytes, 0) + coalesce(thumbnail_bytes, 0)) STORED,
        used_at INTEGER NOT NULL,
        -- The number media_made gave the last preview or thumbnail written of it, 0 for none.
        made INTEGER NOT NULL DEFAULT 0 CHECK (made >= 0)
    ) STRICT, WITHOUT ROWID;
    CREATE INDEX media_files_by_use ON media_files (used_at);
    CREATE INDEX media_files_by_made ON media_files (made);

    -- Class: jaw's own. Previews and thumbnails are put in place without waiting for the disk, so
    -- each write is numbered (`made`, the last number given), and `settled` is the number up to
    -- which they've since been sent to the drive and made durable by a synced commit; one past it
    -- at a start may have been broken by a power cut, and is checked before it's shown.
    CREATE TABLE media_made (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        made INTEGER NOT NULL DEFAULT 0 CHECK (made >= 0),
        settled INTEGER NOT NULL DEFAULT 0 CHECK (settled >= 0 AND settled <= made)
    ) STRICT;
    INSERT INTO media_made (id) VALUES (1);
    -- What the old table knew of a downloaded file, by its content, where it had a hash.
    INSERT OR IGNORE INTO media_files (sha256, path, bytes, small, large, used_at)
    SELECT sha256, path, iif(path IS NULL, NULL, bytes), preview_small, preview_large, coalesce(opened_at, 0) FROM media
    WHERE length(sha256) = 32 AND coalesce(path, preview_small, preview_large) IS NOT NULL;

    DROP TABLE media;
    ALTER TABLE media_rebuilt RENAME TO media;
    CREATE INDEX media_by_hash ON media (sha256) WHERE sha256 IS NOT NULL;
    CREATE INDEX media_wanted ON media (wanted) WHERE wanted IS NOT NULL;
    -- What an interface asked for is done ahead of whatever else is wanted, and what arrived live
    -- ahead of what windows show.
    CREATE INDEX media_asked ON media (wanted) WHERE asked = 1;
    CREATE INDEX media_arrived ON media (wanted) WHERE arrived = 1;
    CREATE INDEX media_waiting ON media (retried_at) WHERE state = 'waiting_for_phone';

    -- Class: derived from media_files, kept by its triggers: the cache's bytes on disk and its files.
    CREATE TABLE media_cache (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        bytes INTEGER NOT NULL DEFAULT 0 CHECK (bytes >= 0),
        files INTEGER NOT NULL DEFAULT 0 CHECK (files >= 0)
    ) STRICT;
    INSERT INTO media_cache (id, bytes, files) SELECT 1, coalesce(sum(stored), 0), count(*) FROM media_files;
    CREATE TRIGGER media_files_added AFTER INSERT ON media_files BEGIN
        UPDATE media_cache SET bytes = bytes + new.stored, files = files + 1 WHERE id = 1;
    END;
    CREATE TRIGGER media_files_removed AFTER DELETE ON media_files BEGIN
        UPDATE media_cache SET bytes = bytes - old.stored, files = files - 1 WHERE id = 1;
    END;
    CREATE TRIGGER media_files_resized AFTER UPDATE ON media_files WHEN new.stored IS NOT old.stored BEGIN
        UPDATE media_cache SET bytes = bytes - old.stored + new.stored WHERE id = 1;
    END;

    -- Class: jaw's own. Files in the cache that no copy names any more, as a message deleted for you
    -- or for everyone leaves them, each removed with its files once a synced commit has dropped it.
    CREATE TABLE media_drop (sha256 BLOB PRIMARY KEY CHECK (length(sha256) = 32)) STRICT, WITHOUT ROWID;
    CREATE TRIGGER media_unnamed AFTER DELETE ON media
    WHEN old.sha256 IS NOT NULL AND NOT EXISTS (SELECT 1 FROM media WHERE sha256 = old.sha256) BEGIN
        INSERT OR IGNORE INTO media_drop (sha256)
            SELECT old.sha256 WHERE EXISTS (SELECT 1 FROM media_files WHERE sha256 = old.sha256);
    END;
    CREATE TRIGGER media_named AFTER INSERT ON media WHEN new.sha256 IS NOT NULL BEGIN
        DELETE FROM media_drop WHERE sha256 = new.sha256;
    END;
    CREATE TRIGGER media_renamed AFTER UPDATE OF sha256 ON media WHEN new.sha256 IS NOT NULL BEGIN
        DELETE FROM media_drop WHERE sha256 = new.sha256;
    END;
    -- A file's row written when no copy names it any more, as a download or a thumbnail finishing
    -- after its message was deleted leaves one, waits to go as well.
    CREATE TRIGGER media_files_unnamed AFTER INSERT ON media_files
    WHEN NOT EXISTS (SELECT 1 FROM media WHERE sha256 = new.sha256) BEGIN
        INSERT OR IGNORE INTO media_drop (sha256) VALUES (new.sha256);
    END;

    -- The files of the messages jaw.db already holds, each copy read from its raw protobuf.
    INSERT INTO backfills (name, after, upto) SELECT 'copies', 0, seq FROM sqlite_sequence
        WHERE name = 'messages' AND seq > 0;
    ",
    // 21, a schema step: places read fully. A location is found by its name and its address both;
    // the ones jaw.db already holds are read again from their raw protobufs by a backfill. It adds
    // one row, so it costs the same however many messages there are.
    "
    -- The locations among the messages jaw.db already holds.
    INSERT INTO backfills (name, after, upto) SELECT 'places', 0, seq FROM sqlite_sequence
        WHERE name = 'messages' AND seq > 0;
    ",
    // 22, a schema step: answers to events. Each person's latest answer to an event is kept, so an
    // answer older than it never replaces it, and windows count going, maybe and not going from the
    // event's own rows as they read it. Nothing kept answers before, so the table is empty, and it
    // costs the same however many messages there are. The events jaw.db already holds are read
    // again from their raw protobufs by a backfill, for their place and call link, and what an
    // earlier jawd kept sealed with their secret, their answers and their edits, is opened.
    "
    -- Class: jaw's own, as poll_voters is, which keeps no raw once applied: each person's latest
    -- answer to each event, by its sender's time and then its message's ID, which an answer older
    -- than it never replaces: going, maybe or not going, or none of them, and the guests it brings.
    CREATE TABLE event_answers (
        message INTEGER NOT NULL REFERENCES messages (n) ON DELETE CASCADE,
        who INTEGER NOT NULL REFERENCES person_jids (n),
        answer TEXT NOT NULL CHECK (answer IN ('going', 'maybe', 'not_going', 'none')),
        guests INTEGER NOT NULL CHECK (guests >= 0),
        at INTEGER NOT NULL,
        id TEXT NOT NULL,
        PRIMARY KEY (message, who)
    ) STRICT, WITHOUT ROWID;
    CREATE INDEX event_answers_by_who ON event_answers (who);
    -- The events among the messages jaw.db already holds.
    INSERT INTO backfills (name, after, upto) SELECT 'events', 0, seq FROM sqlite_sequence
        WHERE name = 'messages' AND seq > 0;
    ",
    // 23, a schema step: what waits for an event is taken a few at a time. The transaction that
    // brings an event, moves it with a merge or reads it again takes only a few of the answers and
    // edits waiting for it, which an event's secret opens one by one, and leaves the rest to short
    // steps in the background, from where it got to. The table is new and empty, so it costs the
    // same however many messages there are.
    "
    -- Class: jaw's own, as backfills is: each event jaw has with what waits for it still to be
    -- taken, and where its next step goes on from: past the row of pending last taken, by when it
    -- was done and then its n; past every row of pending, at its largest, once those set aside in
    -- quarantine for it are coming back. It goes once nothing is left for it, or with its event.
    CREATE TABLE openings (
        message INTEGER PRIMARY KEY REFERENCES messages (n) ON DELETE CASCADE,
        at INTEGER NOT NULL,
        n INTEGER NOT NULL
    ) STRICT;
    ",
    // 24, a schema step: what's sealed for an event between people jaw knows by other JIDs only
    // later opens once it does. The table is new and empty, so it costs the same however many
    // messages there are.
    "
    -- Class: jaw's own: each event jaw has with answers or edits kept sealed in pending that didn't
    -- open with the JIDs jaw knew its author and their senders by when they were tried, so once one
    -- of them is known by another JID what's kept for the event is tried again. It goes once a try
    -- leaves nothing sealed for the event, or with its event.
    CREATE TABLE unopened (
        message INTEGER PRIMARY KEY REFERENCES messages (n) ON DELETE CASCADE
    ) STRICT;
    ",
    // 25, a data step: the library. Every message that's a photo, a video, a voice message or a file
    // is an item of that kind (its shelf), found by indexes on messages that hold only the items, in
    // the order of their t, so a library window is a keyset across chats, in a chat or from one
    // sender; each of a message's links is an item too, kept in the same order in library_links by
    // triggers on links. How many items of each kind each chat has is kept by the triggers that count
    // its messages and those on links, and items' and links' words are indexed apart from the rest,
    // with each one's kind, chat and sender as tags, by the triggers that index messages' and those on
    // links, so none adds a trigger to fire for every message and a search reads only what its window
    // asks for. Its indexes read every message once each, so its cost grows with them: as jawd runs
    // it, on its writer's connection, 4.45 to 4.63 s for a million messages, 150,000 of them items
    // and 21,000 links, its foreign_key_check, fingerprint and commit included, at a load of 58 to 66,
    // after the 2.9 to 3.2 s its backup of a 649 MB jaw.db takes; it grows jaw.db by 26 MB, and the
    // backup is all it needs beside it (measure_the_librarys_step).
    "
    -- Class: derived from messages, a column of each one's kind and flags, computed as it's read and
    -- never stored: the library's kind of the item it is. A message deleted for everyone or seen once
    -- is none, and so are stickers and every kind but these.
    ALTER TABLE messages ADD COLUMN shelf TEXT GENERATED ALWAYS AS (CASE
        WHEN deleted OR view_once THEN NULL
        WHEN kind = 'image' THEN 'photo'
        WHEN kind IN ('video', 'gif', 'video_note') THEN 'video'
        WHEN kind = 'voice' THEN 'voice'
        WHEN kind IN ('document', 'audio') THEN 'file' END) VIRTUAL;
    -- The library's windows, newest first by t (the time, then the place in that millisecond):
    -- across chats; in one chat, with who sent each, for one person's items there; and from one
    -- sender across chats, yours having none. Each holds only the messages that are items.
    CREATE INDEX messages_shelved ON messages (shelf, t) WHERE shelf IS NOT NULL;
    CREATE INDEX messages_shelved_in ON messages (chat, shelf, t, sender, mine) WHERE shelf IS NOT NULL;
    CREATE INDEX messages_shelved_by ON messages (sender, shelf, t, mine) WHERE shelf IS NOT NULL;
    -- An item by its message's ID alone, for a window anchored on it across chats.
    CREATE INDEX messages_shelved_named ON messages (id) WHERE shelf IS NOT NULL;

    -- Class: derived from messages, as links is, from which the triggers on links below keep it:
    -- each of a message's links as the library pages them, by its message's t and then `rank`, its
    -- place among the message's links made negative, so a window read newest first reads a
    -- message's links first to last; with the message's ID, its chat and who sent it, so a window of
    -- links is a keyset across chats, in a chat or from one sender, in the order of items. links'
    -- own order can't be the library's: a message's links take new rowids each time they're derived
    -- again, and a later copy of a message can change its time but never its t.
    CREATE TABLE library_links (
        message INTEGER NOT NULL,
        place INTEGER NOT NULL CHECK (place >= 0),
        t INTEGER NOT NULL,
        id TEXT NOT NULL,
        chat INTEGER NOT NULL,
        sender INTEGER,
        mine INTEGER NOT NULL CHECK (mine IN (0, 1)),
        rank INTEGER NOT NULL GENERATED ALWAYS AS (-place) VIRTUAL,
        PRIMARY KEY (message, place)
    ) STRICT, WITHOUT ROWID;
    CREATE INDEX library_links_by_t ON library_links (t, rank);
    CREATE INDEX library_links_in ON library_links (chat, t, rank, sender, mine);
    CREATE INDEX library_links_by ON library_links (sender, mine, t, rank);
    CREATE INDEX library_links_named ON library_links (id);
    INSERT INTO library_links (message, place, t, id, chat, sender, mine)
    SELECT l.message, l.place, m.t, m.id, l.chat, l.sender, l.mine FROM links l JOIN messages m ON m.n = l.message;

    -- Class: derived from messages, kept by the triggers on messages and on links: how many items of
    -- each kind each chat has, links among them, and every chat together as chat 0, so a library
    -- window's counts are read, not counted.
    CREATE TABLE shelves (
        chat INTEGER NOT NULL CHECK (chat >= 0),
        shelf TEXT NOT NULL CHECK (shelf IN ('photo', 'video', 'voice', 'file', 'link')),
        items INTEGER NOT NULL CHECK (items > 0),
        PRIMARY KEY (chat, shelf)
    ) STRICT, WITHOUT ROWID;
    INSERT INTO shelves (chat, shelf, items)
    SELECT chat, shelf, count(*) FROM messages INDEXED BY messages_shelved_in WHERE shelf IS NOT NULL
    GROUP BY chat, shelf;
    INSERT INTO shelves (chat, shelf, items)
    SELECT chat, 'link', count(*) FROM library_links INDEXED BY library_links_in GROUP BY chat;
    INSERT INTO shelves (chat, shelf, items) SELECT 0, shelf, sum(items) FROM shelves GROUP BY shelf;

    -- Class: derived from messages, as messages_fts is: the words of each item, its caption and a
    -- file's name, by its message's t, and in `tags` its kind, its chat and who sent it (`s` and the
    -- kind, `c` and the chat's n, `f` and the sender's n in person_jids, or `fme` for yours), so the
    -- library's search reads only the items a window asks for, newest first, and stops at a page,
    -- and counts them by their rowids alone. A message's links' words, as links_fts has them, are
    -- one entry of kind `link`, by ~t, the complement of its message's t, which is negative where
    -- every t is not, so a message can be an item and have links, and ~t's order is t's backwards. A
    -- message waiting in fts_backlog is indexed here when it's indexed there, and the writer merges
    -- its segments as it merges messages_fts's. Every entry is written OR REPLACE: a contentless
    -- table takes a second insert of a rowid it holds, and a delete then takes only the second,
    -- leaving the first findable.
    CREATE VIRTUAL TABLE library_fts USING fts5 (
        text,
        tags,
        content = '',
        contentless_delete = 1,
        tokenize = 'unicode61 remove_diacritics 2'
    );
    INSERT INTO library_fts (library_fts, rank) VALUES ('automerge', 0);
    INSERT INTO library_fts (library_fts, rank) VALUES ('crisismerge', 64);
    INSERT OR REPLACE INTO library_fts (rowid, text, tags)
    SELECT t, text, 's' || shelf || ' c' || chat || ' f' || coalesce(sender, iif(mine, 'me', 'none'))
    FROM messages INDEXED BY messages_shelved WHERE shelf IS NOT NULL AND text IS NOT NULL
        AND NOT EXISTS (SELECT 1 FROM fts_backlog b WHERE b.n = messages.n);
    -- Newest first, so ~t rises, as FTS5 takes rowids best.
    INSERT OR REPLACE INTO library_fts (rowid, text, tags)
    SELECT ~l.t, w.text, 'slink c' || m.chat || ' f' || coalesce(m.sender, iif(m.mine, 'me', 'none'))
    FROM (SELECT t, message FROM library_links INDEXED BY library_links_by_t GROUP BY t ORDER BY t DESC) l
        JOIN messages m ON m.n = l.message JOIN message_link_text w ON w.message = l.message
    WHERE w.text IS NOT NULL AND NOT EXISTS (SELECT 1 FROM fts_backlog b WHERE b.n = l.message)
    ORDER BY l.t DESC;

    -- A link that comes or goes is an item that comes or goes: in library_links, counted in its chat
    -- and in all of them, and its message's links' words written again with its chat and sender as
    -- tags, unless the message waits in fts_backlog, whose steps index them with its text. The
    -- triggers on messages write a message's links together and take them all away together, as a
    -- deletion of the message does, so its words are written again as each link comes and go with
    -- the last; that one's t is read from library_links, since its message may be gone.
    CREATE TRIGGER links_shelved AFTER INSERT ON links BEGIN
        INSERT INTO library_links (message, place, t, id, chat, sender, mine)
            SELECT new.message, new.place, m.t, m.id, new.chat, new.sender, new.mine FROM messages m
            WHERE m.n = new.message;
        INSERT INTO shelves (chat, shelf, items) VALUES (new.chat, 'link', 1)
            ON CONFLICT (chat, shelf) DO UPDATE SET items = items + 1;
        INSERT INTO shelves (chat, shelf, items) VALUES (0, 'link', 1)
            ON CONFLICT (chat, shelf) DO UPDATE SET items = items + 1;
        INSERT OR REPLACE INTO library_fts (rowid, text, tags)
            SELECT ~w.t, w.text, 'slink c' || new.chat || ' f' || coalesce(new.sender, iif(new.mine, 'me', 'none'))
            FROM message_link_text w WHERE w.message = new.message AND w.text IS NOT NULL
                AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = new.message);
    END;
    CREATE TRIGGER links_unshelved AFTER DELETE ON links BEGIN
        DELETE FROM library_fts WHERE rowid = ~(SELECT t FROM library_links WHERE message = old.message AND place = old.place)
            AND NOT EXISTS (SELECT 1 FROM links WHERE message = old.message)
            AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = old.message);
        DELETE FROM library_links WHERE message = old.message AND place = old.place;
        DELETE FROM shelves WHERE chat IN (old.chat, 0) AND shelf = 'link' AND items = 1;
        UPDATE shelves SET items = items - 1 WHERE chat IN (old.chat, 0) AND shelf = 'link';
    END;

    -- The triggers that count messages count items too: one that comes, goes, moves to another chat
    -- or becomes another kind of item or none moves its count, and a chat's count of a kind goes with
    -- its last item of that kind.
    DROP TRIGGER messages_counted;
    CREATE TRIGGER messages_counted AFTER INSERT ON messages BEGIN
        UPDATE chats SET message_count = message_count + 1 WHERE n = new.chat;
        INSERT INTO shelves (chat, shelf, items) SELECT new.chat, new.shelf, 1 WHERE new.shelf IS NOT NULL
            ON CONFLICT (chat, shelf) DO UPDATE SET items = items + 1;
        INSERT INTO shelves (chat, shelf, items) SELECT 0, new.shelf, 1 WHERE new.shelf IS NOT NULL
            ON CONFLICT (chat, shelf) DO UPDATE SET items = items + 1;
    END;
    DROP TRIGGER messages_uncounted;
    CREATE TRIGGER messages_uncounted AFTER DELETE ON messages BEGIN
        UPDATE chats SET message_count = message_count - 1 WHERE n = old.chat;
        DELETE FROM shelves WHERE old.shelf IS NOT NULL AND chat IN (old.chat, 0) AND shelf = old.shelf AND items = 1;
        UPDATE shelves SET items = items - 1 WHERE old.shelf IS NOT NULL AND chat IN (old.chat, 0) AND shelf = old.shelf;
    END;
    DROP TRIGGER messages_moved;
    CREATE TRIGGER messages_moved AFTER UPDATE OF chat, kind, deleted, view_once ON messages
    WHEN new.chat IS NOT old.chat OR new.shelf IS NOT old.shelf BEGIN
        UPDATE chats SET message_count = message_count - 1 WHERE n = old.chat AND new.chat IS NOT old.chat;
        UPDATE chats SET message_count = message_count + 1 WHERE n = new.chat AND new.chat IS NOT old.chat;
        DELETE FROM shelves WHERE old.shelf IS NOT NULL AND chat IN (old.chat, 0) AND shelf = old.shelf AND items = 1;
        UPDATE shelves SET items = items - 1 WHERE old.shelf IS NOT NULL AND chat IN (old.chat, 0) AND shelf = old.shelf;
        INSERT INTO shelves (chat, shelf, items) SELECT new.chat, new.shelf, 1 WHERE new.shelf IS NOT NULL
            ON CONFLICT (chat, shelf) DO UPDATE SET items = items + 1;
        INSERT INTO shelves (chat, shelf, items) SELECT 0, new.shelf, 1 WHERE new.shelf IS NOT NULL
            ON CONFLICT (chat, shelf) DO UPDATE SET items = items + 1;
    END;

    -- The triggers that index messages' words index items' words too: a message that's an item
    -- with words is in library_fts once it's out of fts_backlog, and one whose words change, that
    -- becomes an item or stops being one, or that moves to another chat or another sender, as a
    -- merge moves it, is indexed again.
    DROP TRIGGER messages_fts_insert;
    CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages
    WHEN new.text IS NOT NULL AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = new.n) BEGIN
        INSERT INTO messages_fts (rowid, text) VALUES (new.t, new.text);
        INSERT OR REPLACE INTO library_fts (rowid, text, tags)
            SELECT new.t, new.text, 's' || new.shelf || ' c' || new.chat || ' f' || coalesce(new.sender, iif(new.mine, 'me', 'none'))
            WHERE new.shelf IS NOT NULL;
    END;
    DROP TRIGGER messages_fts_delete;
    CREATE TRIGGER messages_fts_delete AFTER DELETE ON messages BEGIN
        INSERT INTO messages_fts (messages_fts, rowid, text) SELECT 'delete', old.t, old.text
            WHERE old.text IS NOT NULL AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = old.n);
        DELETE FROM library_fts WHERE old.shelf IS NOT NULL AND old.text IS NOT NULL AND rowid = old.t
            AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = old.n);
        DELETE FROM fts_backlog WHERE n = old.n;
    END;
    DROP TRIGGER messages_fts_update;
    CREATE TRIGGER messages_fts_update AFTER UPDATE OF text, kind, deleted, view_once, chat, sender, mine ON messages
    WHEN (old.text IS NOT new.text OR old.shelf IS NOT new.shelf OR old.shelf IS NOT NULL AND (old.chat IS NOT new.chat
            OR old.sender IS NOT new.sender OR old.mine IS NOT new.mine))
        AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = old.n) BEGIN
        INSERT INTO messages_fts (messages_fts, rowid, text) SELECT 'delete', old.t, old.text
            WHERE old.text IS NOT NULL AND old.text IS NOT new.text;
        INSERT INTO messages_fts (rowid, text) SELECT new.t, new.text WHERE new.text IS NOT NULL AND old.text IS NOT new.text;
        DELETE FROM library_fts WHERE old.shelf IS NOT NULL AND old.text IS NOT NULL AND rowid = old.t;
        INSERT OR REPLACE INTO library_fts (rowid, text, tags)
            SELECT new.t, new.text, 's' || new.shelf || ' c' || new.chat || ' f' || coalesce(new.sender, iif(new.mine, 'me', 'none'))
            WHERE new.shelf IS NOT NULL AND new.text IS NOT NULL;
    END;
    ",
    // 26, a schema step: the contact cards jaw.db holds are read again from their raw protobufs by a
    // backfill, now that a message of several cards shows them as one card's does, `contact`'s name
    // and vCard, and a card its message gives no name takes the one its vCard gives. It leaves one
    // row in `backfills`, jaw's own, so it costs the same however many messages there are.
    "
    INSERT INTO backfills (name, after, upto) SELECT 'contacts', 0, seq FROM sqlite_sequence
        WHERE name = 'messages' AND seq > 0;
    ",
    // 27, a schema step: media out. Two new tables, `outgoing` and `outgoing_drop`, with their
    // triggers, and media_unnamed and media_files_unnamed made again so a file in the cache that a
    // copy on its way names isn't dropped. No jawd before this one sent media, so both tables start
    // empty, and replacing a trigger costs the same however big jaw.db is; nothing is copied, rebuilt
    // or dropped but the triggers.
    "
    -- Class: jaw's own. Each copy of a file of yours on its way to WhatsApp, with the outbox row that
    -- sends it: the SD copy, and the HD copy when one was asked for and the file has more to give
    -- than SD keeps, which goes after its SD message as a message of its own that names it. A row
    -- goes with its outbox row: once its message has gone, or nothing can send it any more.
    CREATE TABLE outgoing (
        outbox INTEGER PRIMARY KEY REFERENCES outbox (n) ON DELETE CASCADE,
        -- The message the copy is of, an HD copy's being its SD message's; NULL once it's deleted for
        -- you, which, as on the phone, doesn't stop it going.
        message INTEGER REFERENCES messages (n) ON DELETE SET NULL,
        variant TEXT NOT NULL CHECK (variant IN ('sd', 'hd')),
        -- What it goes as.
        kind TEXT NOT NULL CHECK (kind IN ('image', 'video', 'gif', 'audio', 'document')),
        -- jaw's own copy of the file you picked, relative to jaw's folder, kept until this copy is made
        -- from it, and that file's size.
        source TEXT,
        bytes INTEGER NOT NULL CHECK (bytes >= 0),
        -- preparing: the copy is being made from `source`; uploading: it's in the cache, by `sha256`,
        -- on its way to WhatsApp's media server; ready: its outbox row's message says where it is,
        -- and goes; failed: it couldn't be made or uploaded, as `error` says, until send.retry.
        state TEXT NOT NULL DEFAULT 'preparing' CHECK (state IN ('preparing', 'uploading', 'ready', 'failed')),
        sha256 BLOB CHECK (length(sha256) = 32),
        error TEXT,
        CHECK (state IN ('preparing', 'failed') OR sha256 IS NOT NULL),
        CHECK (sha256 IS NOT NULL OR source IS NOT NULL)
    ) STRICT;
    CREATE INDEX outgoing_by_message ON outgoing (message) WHERE message IS NOT NULL;
    CREATE INDEX outgoing_by_source ON outgoing (source) WHERE source IS NOT NULL;
    CREATE INDEX outgoing_by_hash ON outgoing (sha256) WHERE sha256 IS NOT NULL;

    -- Class: jaw's own. Copies of files you picked that no copy on its way needs any more, each
    -- removed once a synced commit has dropped it.
    CREATE TABLE outgoing_drop (path TEXT PRIMARY KEY) STRICT, WITHOUT ROWID;
    CREATE TRIGGER outgoing_gone AFTER DELETE ON outgoing
    WHEN old.source IS NOT NULL AND NOT EXISTS (SELECT 1 FROM outgoing WHERE source = old.source) BEGIN
        INSERT OR IGNORE INTO outgoing_drop (path) VALUES (old.source);
    END;
    CREATE TRIGGER outgoing_made AFTER UPDATE OF source ON outgoing
    WHEN old.source IS NOT NULL AND new.source IS NOT old.source
        AND NOT EXISTS (SELECT 1 FROM outgoing WHERE source = old.source) BEGIN
        INSERT OR IGNORE INTO outgoing_drop (path) VALUES (old.source);
    END;

    -- A file in the cache goes once neither a copy of a message nor a copy on its way names it.
    DROP TRIGGER media_unnamed;
    CREATE TRIGGER media_unnamed AFTER DELETE ON media
    WHEN old.sha256 IS NOT NULL AND NOT EXISTS (SELECT 1 FROM media WHERE sha256 = old.sha256)
        AND NOT EXISTS (SELECT 1 FROM outgoing WHERE sha256 = old.sha256) BEGIN
        INSERT OR IGNORE INTO media_drop (sha256)
            SELECT old.sha256 WHERE EXISTS (SELECT 1 FROM media_files WHERE sha256 = old.sha256);
    END;
    CREATE TRIGGER outgoing_unnamed AFTER DELETE ON outgoing
    WHEN old.sha256 IS NOT NULL AND NOT EXISTS (SELECT 1 FROM media WHERE sha256 = old.sha256)
        AND NOT EXISTS (SELECT 1 FROM outgoing WHERE sha256 = old.sha256) BEGIN
        INSERT OR IGNORE INTO media_drop (sha256)
            SELECT old.sha256 WHERE EXISTS (SELECT 1 FROM media_files WHERE sha256 = old.sha256);
    END;
    CREATE TRIGGER outgoing_named AFTER UPDATE OF sha256 ON outgoing WHEN new.sha256 IS NOT NULL BEGIN
        DELETE FROM media_drop WHERE sha256 = new.sha256;
    END;
    DROP TRIGGER media_files_unnamed;
    CREATE TRIGGER media_files_unnamed AFTER INSERT ON media_files
    WHEN NOT EXISTS (SELECT 1 FROM media WHERE sha256 = new.sha256)
        AND NOT EXISTS (SELECT 1 FROM outgoing WHERE sha256 = new.sha256) BEGIN
        INSERT OR IGNORE INTO media_drop (sha256) VALUES (new.sha256);
    END;
    ",
    // 28, a schema step: communities gathered under themselves in every chat list. A community's
    // place in a list is that of the newest of what it brings there, itself or a group of it,
    // worked out as the list is read, so its own last_at follows only its own messages again and
    // the trigger that raised it to its groups' newest goes: it kept a community at the place of a
    // group that had left it, and left one learned after its groups at its own place until one of
    // them had a message. It reads only chats, which hold a row for each conversation, and each
    // community's newest message by the keyset index, so it costs the same however many messages
    // there are: 33 to 36 ms beside a million messages, 3,000 chats and six communities of ten
    // groups, its fingerprint and commit included, at a load of about 60
    // (`measure_the_communities_migration`). It drops no table, and nothing refers to what it
    // changes, so it needs neither a data step's backup nor its check of every foreign key.
    COMMUNITIES_GATHERED,
];

/// Migration 28's text, named so the tests that check and measure it find it whatever number it
/// takes when it merges.
const COMMUNITIES_GATHERED: &str = "
    DROP TRIGGER chats_community_followed;
    -- Class: derived, as chats' other indexes. The communities jaw has, tens at most, for counting
    -- those a chat list holds only for their groups; written only as a chat becomes a community.
    CREATE INDEX chats_known_communities ON chats (n) WHERE kind = 'community';
    -- A community sorts by its own messages; where it sits in a list is the newest of what it brings.
    UPDATE chats SET last_at = coalesce((SELECT max(m.at) FROM messages m WHERE m.chat = chats.n), 0)
        WHERE kind = 'community';
    -- A community's row and its groups' sit and count otherwise in a window now, so each takes a
    -- new version with its event, as the writer gives a row it changes, and an interface watching
    -- again from before the upgrade is sent it. Both statements number the same rows alike, after
    -- the feed's last seq.
    UPDATE chats SET v = bumped.seq FROM (
        SELECT n, (SELECT coalesce(max(seq), 0) FROM events) + row_number() OVER (ORDER BY n) AS seq FROM chats
        WHERE kind = 'community' OR community IS NOT NULL) AS bumped
    WHERE chats.n = bumped.n;
    INSERT INTO events (seq, at, type, key, chat)
        SELECT (SELECT coalesce(max(seq), 0) FROM events) + row_number() OVER (ORDER BY n),
            CAST(unixepoch('subsec') * 1000 AS INTEGER), 'chat', n, n FROM chats
        WHERE kind = 'community' OR community IS NOT NULL;
";

/// Migration 19's text, named so the tests that check and measure it find it whatever number it
/// takes when it merges.
const FOLDERS_AND_THE_ROSTER: &str = "
    -- Class: jaw's own, never dropped or rebuilt from anything else, and copied as it is by any
    -- rebuild of jaw.db. The folder tree: the five built-in folders (builtin), smart ones (rule)
    -- and yours, each a row so their counts are kept alike. Interfaces name a folder by its n, so
    -- it's never used again once the folder is deleted.
    CREATE TABLE folders_rebuilt (
        n INTEGER PRIMARY KEY AUTOINCREMENT,
        builtin TEXT UNIQUE CHECK (builtin IN ('all', 'unread', 'groups', 'communities', 'archived')),
        -- The folder it's in, NULL at the top. A folder goes only once nothing is in it
        -- (folders_emptied): its subfolders moved up and its items unfiled first, so nothing cascades.
        parent INTEGER REFERENCES folders (n),
        name TEXT NOT NULL CHECK (trim(name) <> ''),
        -- Its place among its siblings of the same kind.
        position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
        -- A smart folder's rule: a JSON object, every field of which must hold.
        rule TEXT CHECK (json_valid(rule) AND json_type(rule) = 'object'),
        v INTEGER NOT NULL,
        CHECK (builtin IS NULL OR rule IS NULL),
        -- Only your folders nest, and only in yours, which the writer checks as they move.
        CHECK (parent IS NULL OR builtin IS NULL AND rule IS NULL)
    ) STRICT;
    INSERT INTO folders_rebuilt (n, parent, name, position, rule, v)
        SELECT n, parent, name, max(position, 0), smart, v FROM folders ORDER BY n;
    INSERT INTO folders_rebuilt (builtin, name, position, v) VALUES ('all', 'All', 0, 0), ('unread', 'Unread', 1, 0),
        ('groups', 'Groups', 2, 0), ('communities', 'Communities', 3, 0), ('archived', 'Archived', 4, 0);
    INSERT INTO folders_rebuilt (name, position, rule, v) VALUES ('Waiting on me', 0, '{\"waiting\":86400}', 0),
        ('Mentions me', 1, '{\"mentions\":true}', 0), ('Muted, unread', 2, '{\"muted\":true,\"unread\":true}', 0);

    -- Class: jaw's own, as folders. What Alt-1 to Alt-9 go to: a folder or a chat, in one slot at most.
    CREATE TABLE favourites (
        slot INTEGER PRIMARY KEY CHECK (slot BETWEEN 1 AND 9),
        folder INTEGER UNIQUE REFERENCES folders (n) ON DELETE CASCADE,
        chat INTEGER UNIQUE REFERENCES chats (n) ON DELETE CASCADE,
        CHECK ((folder IS NULL) <> (chat IS NULL))
    ) STRICT;
    INSERT INTO favourites (slot, folder) SELECT favourite, n FROM folders WHERE favourite BETWEEN 1 AND 9;

    -- Class: jaw's own, as folders. What's filed in your folders: someone, who stands for their
    -- one-to-one chat in chat lists and for themselves in rosters, or a chat that isn't anyone's
    -- one-to-one chat, such as a group; filing someone's chat files them. An item can sit in any
    -- number of folders. Migration 2's position goes, as nothing ever wrote it: a folder's chats
    -- sort by their newest message, and its people as rosters do.
    CREATE TABLE folder_items_rebuilt (
        n INTEGER PRIMARY KEY,
        folder INTEGER NOT NULL REFERENCES folders (n) ON DELETE CASCADE,
        chat INTEGER REFERENCES chats (n) ON DELETE CASCADE,
        person INTEGER REFERENCES people (n) ON DELETE CASCADE,
        CHECK ((chat IS NULL) <> (person IS NULL))
    ) STRICT;
    INSERT INTO folder_items_rebuilt (folder, chat, person)
        SELECT DISTINCT i.folder, iif(c.person IS NULL, i.chat, NULL), coalesce(i.person, c.person)
        FROM folder_items i LEFT JOIN chats c ON c.n = i.chat;
    DROP TABLE folder_items;
    DROP TABLE folders;
    ALTER TABLE folders_rebuilt RENAME TO folders;
    ALTER TABLE folder_items_rebuilt RENAME TO folder_items;
    CREATE INDEX folders_by_parent ON folders (parent, position);
    -- Two of your folders side by side never share a name, whatever its case.
    CREATE UNIQUE INDEX folders_named ON folders (coalesce(parent, 0), lower(name)) WHERE builtin IS NULL AND rule IS NULL;
    CREATE UNIQUE INDEX folder_items_chats ON folder_items (folder, chat) WHERE chat IS NOT NULL;
    CREATE UNIQUE INDEX folder_items_people ON folder_items (folder, person) WHERE person IS NOT NULL;
    -- Each folder's items, for its foreign key too.
    CREATE INDEX folder_items_by_folder ON folder_items (folder);
    CREATE INDEX folder_items_by_chat ON folder_items (chat) WHERE chat IS NOT NULL;
    CREATE INDEX folder_items_by_person ON folder_items (person) WHERE person IS NOT NULL;

    -- Class: derived from chats, kept by the triggers below: how many chats are this person's
    -- one-to-one chat, two while a merge joins them.
    ALTER TABLE people ADD COLUMN direct_chats INTEGER NOT NULL DEFAULT 0;
    UPDATE people SET direct_chats = (SELECT count(*) FROM chats WHERE person = people.n)
        WHERE n IN (SELECT person FROM chats WHERE person IS NOT NULL);
    -- Class: fetched from WhatsApp while jawd follows them: online, or NULL when they're not around
    -- or jawd can't know, as whenever WhatsApp can't be reached. Who's typing or recording is never
    -- kept in jaw.db, only in jawd's memory.
    ALTER TABLE people ADD COLUMN presence TEXT CHECK (presence = 'online');
    -- Class: jaw's own, as it runs, cleared at every start: whether jawd follows their presence now.
    ALTER TABLE people ADD COLUMN followed INTEGER NOT NULL DEFAULT 0 CHECK (followed IN (0, 1));
    -- Class: derived, as v is: the seq of the last change to what only their roster row shows,
    -- their presence, last seen and whether jawd follows them, and their about text, avatar, chat
    -- and folders. v is that of what message rows show of them too, their names and JIDs, so
    -- someone coming online or going away moves no message row's version. A roster row's version
    -- is the newer of the two.
    ALTER TABLE people ADD COLUMN roster_v INTEGER NOT NULL DEFAULT 0;
    -- Where they sort in a roster: around first, by name; then by when they were last seen, the
    -- most recent first; then those who hide it or whom WhatsApp hasn't told jawd about, by name.
    ALTER TABLE people ADD COLUMN roster_sort TEXT GENERATED ALWAYS AS (
        iif(presence IS NOT NULL, '0~',
            iif(last_seen > 0, '1~' || printf('%019d', 9223372036854775807 - last_seen) || '~', '2~'))
        || lower(coalesce(name, push_name, verified_name, ''))) VIRTUAL;
    -- The roster: your contacts on WhatsApp, and anyone with a one-to-one chat with you.
    CREATE INDEX people_roster ON people (roster_sort, n) WHERE name IS NOT NULL OR direct_chats > 0;
    -- Whose about text is due, the longest unasked first.
    CREATE INDEX people_about_due ON people (about_at) WHERE name IS NOT NULL OR direct_chats > 0;
    -- Who's around or followed, all cleared when presence can't be known.
    CREATE INDEX people_present ON people (n) WHERE presence IS NOT NULL OR followed <> 0;

    -- Chat lists other than All and Archived, each read by keyset on an index of its own, as All is.
    CREATE INDEX chats_unread ON chats (sort, n) WHERE archived = 0 AND (unread > 0 OR marked_unread <> 0);
    CREATE INDEX chats_groups ON chats (sort, n) WHERE archived = 0 AND kind IN ('group', 'announcement');
    CREATE INDEX chats_communities ON chats (sort, n)
        WHERE archived = 0 AND (kind = 'community' OR community IS NOT NULL);
    CREATE INDEX chats_mentioned ON chats (sort, n) WHERE archived = 0 AND mentions > 0;
    -- A community's groups, the latest written in first. A community sorts by the newest of its
    -- groups' messages, which chats_community_followed keeps in its last_at.
    CREATE INDEX chats_by_community ON chats (community, last_at) WHERE community IS NOT NULL;
    UPDATE chats SET last_at = (SELECT max(g.last_at) FROM chats g WHERE g.community = chats.id)
        WHERE kind = 'community' AND last_at < (SELECT max(g.last_at) FROM chats g WHERE g.community = chats.id);
    -- A community's row shows its groups' unread messages and newest message now, and a filed
    -- chat's row says where it's filed, so each takes a new version with its event, as the writer
    -- gives a row it changes, and an interface watching again from before the upgrade is sent it.
    -- Both statements number the same rows alike, after the feed's last seq.
    UPDATE chats SET v = bumped.seq FROM (
        SELECT n, (SELECT coalesce(max(seq), 0) FROM events) + row_number() OVER (ORDER BY n) AS seq FROM chats
        WHERE kind = 'community' OR n IN (SELECT chat FROM folder_items WHERE chat IS NOT NULL)
            OR person IN (SELECT person FROM folder_items WHERE person IS NOT NULL)) AS bumped
    WHERE chats.n = bumped.n;
    INSERT INTO events (seq, at, type, key, chat)
        SELECT (SELECT coalesce(max(seq), 0) FROM events) + row_number() OVER (ORDER BY n),
            CAST(unixepoch('subsec') * 1000 AS INTEGER), 'chat', n, n FROM chats
        WHERE kind = 'community' OR n IN (SELECT chat FROM folder_items WHERE chat IS NOT NULL)
            OR person IN (SELECT person FROM folder_items WHERE person IS NOT NULL);

    -- Class: derived from folders, and rebuilt from them: each of your folders with every folder
    -- it's in, itself too, so what's in a folder and under it is found without walking the tree.
    CREATE TABLE folder_paths (
        folder INTEGER NOT NULL REFERENCES folders (n) ON DELETE CASCADE,
        ancestor INTEGER NOT NULL REFERENCES folders (n) ON DELETE CASCADE,
        PRIMARY KEY (folder, ancestor)
    ) STRICT, WITHOUT ROWID;
    CREATE INDEX folder_paths_by_ancestor ON folder_paths (ancestor, folder);

    -- Class: derived from folder_items, folder_paths and chats, and rebuilt from them. The chats in
    -- each of your folders and under it, each once: filed there, or the one-to-one chat of someone
    -- filed there. paths counts the ways, so a chat goes when the last does. Its sort, unread
    -- messages and mentions are the chat's, kept as they change, so a folder's chat list pages by
    -- its own index and a chat that goes takes out what it brought, even once it's gone.
    CREATE TABLE folder_chats (
        folder INTEGER NOT NULL REFERENCES folders (n) ON DELETE CASCADE,
        chat INTEGER NOT NULL REFERENCES chats (n) ON DELETE CASCADE,
        paths INTEGER NOT NULL CHECK (paths > 0),
        sort TEXT NOT NULL,
        unread INTEGER NOT NULL,
        mentions INTEGER NOT NULL,
        PRIMARY KEY (folder, chat)
    ) STRICT, WITHOUT ROWID;
    CREATE INDEX folder_chats_by_chat ON folder_chats (chat);
    CREATE INDEX folder_chats_by_sort ON folder_chats (folder, sort, chat);

    -- Class: derived from folder_items, folder_paths and people, and rebuilt from them. The people
    -- filed in each of your folders and under it, each once, for its roster: lone while they have
    -- no chat of their own, so the folder counts them beside its chats; and where they sort, the
    -- person's, kept as it changes.
    CREATE TABLE folder_people (
        folder INTEGER NOT NULL REFERENCES folders (n) ON DELETE CASCADE,
        person INTEGER NOT NULL REFERENCES people (n) ON DELETE CASCADE,
        paths INTEGER NOT NULL CHECK (paths > 0),
        lone INTEGER NOT NULL CHECK (lone IN (0, 1)),
        roster_sort TEXT NOT NULL,
        PRIMARY KEY (folder, person)
    ) STRICT, WITHOUT ROWID;
    CREATE INDEX folder_people_by_person ON folder_people (person);
    CREATE INDEX folder_people_by_sort ON folder_people (folder, roster_sort, person);

    -- Class: derived, kept by the triggers below and checked against what it's derived from in
    -- the background. For each built-in folder and each of yours: its chats' unread messages and
    -- mentions, each chat counted once, how many chats it holds, and how many people with no chat
    -- of their own. Smart folders are counted as they're read.
    CREATE TABLE folder_counts (
        folder INTEGER PRIMARY KEY REFERENCES folders (n) ON DELETE CASCADE,
        unread INTEGER NOT NULL DEFAULT 0,
        mentions INTEGER NOT NULL DEFAULT 0,
        chats INTEGER NOT NULL DEFAULT 0,
        lone INTEGER NOT NULL DEFAULT 0
    ) STRICT;

    WITH RECURSIVE up (folder, ancestor) AS (
        SELECT n, n FROM folders WHERE builtin IS NULL AND rule IS NULL
        UNION SELECT up.folder, f.parent FROM up JOIN folders f ON f.n = up.ancestor WHERE f.parent IS NOT NULL)
    INSERT INTO folder_paths (folder, ancestor) SELECT folder, ancestor FROM up;
    INSERT INTO folder_chats (folder, chat, paths, sort, unread, mentions)
        SELECT r.folder, c.n, r.paths, c.sort, c.unread, c.mentions FROM (
            SELECT folder, chat, count(*) AS paths FROM (
                SELECT p.ancestor AS folder, i.chat AS chat FROM folder_items i JOIN folder_paths p ON p.folder = i.folder
                    WHERE i.chat IS NOT NULL
                UNION ALL
                SELECT p.ancestor, c.n FROM folder_items i JOIN folder_paths p ON p.folder = i.folder
                    JOIN chats c ON c.person = i.person WHERE i.person IS NOT NULL)
            GROUP BY folder, chat) r JOIN chats c ON c.n = r.chat;
    INSERT INTO folder_people (folder, person, paths, lone, roster_sort)
        SELECT p.ancestor, i.person, count(*), x.direct_chats = 0, x.roster_sort
        FROM folder_items i JOIN folder_paths p ON p.folder = i.folder JOIN people x ON x.n = i.person
        WHERE i.person IS NOT NULL GROUP BY p.ancestor, i.person;
    INSERT INTO folder_counts (folder, unread, mentions, chats, lone)
        SELECT f.n, (SELECT coalesce(sum(unread), 0) FROM folder_chats WHERE folder = f.n),
            (SELECT coalesce(sum(mentions), 0) FROM folder_chats WHERE folder = f.n),
            (SELECT count(*) FROM folder_chats WHERE folder = f.n),
            (SELECT count(*) FROM folder_people WHERE folder = f.n AND lone)
        FROM folders f WHERE f.builtin IS NULL AND f.rule IS NULL;
    INSERT INTO folder_counts (folder, unread, mentions, chats)
        SELECT f.n, coalesce(sum(c.unread), 0), coalesce(sum(c.mentions), 0), count(c.n)
        FROM folders f LEFT JOIN chats c ON CASE f.builtin
            WHEN 'all' THEN c.archived = 0
            WHEN 'unread' THEN c.archived = 0 AND (c.unread > 0 OR c.marked_unread <> 0)
            WHEN 'groups' THEN c.archived = 0 AND c.kind IN ('group', 'announcement')
            WHEN 'communities' THEN c.archived = 0 AND (c.kind = 'community' OR c.community IS NOT NULL)
            ELSE c.archived <> 0 END
        WHERE f.builtin IS NOT NULL GROUP BY f.n;

    -- The triggers that keep folder_paths, folder_chats, folder_people, folder_counts and
    -- people.direct_chats derived. Each change to a source row adds or takes out only what that
    -- row brings: a folder's ancestors, its items, a chat's person and counts.

    -- A folder of yours placed in the tree takes every folder above its parent as its own.
    CREATE TRIGGER folders_placed AFTER INSERT ON folders WHEN new.builtin IS NULL AND new.rule IS NULL BEGIN
        INSERT INTO folder_counts (folder) VALUES (new.n);
        INSERT INTO folder_paths (folder, ancestor) VALUES (new.n, new.n);
        INSERT INTO folder_paths (folder, ancestor) SELECT new.n, ancestor FROM folder_paths WHERE folder = new.parent;
    END;
    -- Moved, everything under it leaves the folders it was under and joins those it's under now.
    CREATE TRIGGER folders_moved AFTER UPDATE OF parent ON folders WHEN new.parent IS NOT old.parent BEGIN
        DELETE FROM folder_paths WHERE folder IN (SELECT folder FROM folder_paths WHERE ancestor = new.n)
            AND ancestor IN (SELECT ancestor FROM folder_paths WHERE folder = new.n AND ancestor <> new.n);
        INSERT INTO folder_paths (folder, ancestor)
            SELECT below.folder, above.ancestor FROM folder_paths below JOIN folder_paths above
            WHERE below.ancestor = new.n AND above.folder = new.parent;
    END;
    CREATE TRIGGER folders_emptied BEFORE DELETE ON folders
    WHEN EXISTS (SELECT 1 FROM folders WHERE parent = old.n) OR EXISTS (SELECT 1 FROM folder_items WHERE folder = old.n)
    BEGIN
        SELECT RAISE(ABORT, 'a folder goes only once its subfolders have moved and its items are unfiled');
    END;

    -- A folder gains a folder above it, which takes what's filed in it; or loses one.
    CREATE TRIGGER folder_paths_added AFTER INSERT ON folder_paths BEGIN
        INSERT INTO folder_chats (folder, chat, paths, sort, unread, mentions)
            SELECT new.ancestor, c.n, 1, c.sort, c.unread, c.mentions FROM folder_items i JOIN chats c ON c.n = i.chat
            WHERE i.folder = new.folder AND i.chat IS NOT NULL
            ON CONFLICT (folder, chat) DO UPDATE SET paths = paths + 1;
        INSERT INTO folder_chats (folder, chat, paths, sort, unread, mentions)
            SELECT new.ancestor, c.n, 1, c.sort, c.unread, c.mentions FROM folder_items i JOIN chats c ON c.person = i.person
            WHERE i.folder = new.folder AND i.person IS NOT NULL
            ON CONFLICT (folder, chat) DO UPDATE SET paths = paths + 1;
        INSERT INTO folder_people (folder, person, paths, lone, roster_sort)
            SELECT new.ancestor, x.n, 1, x.direct_chats = 0, x.roster_sort FROM folder_items i JOIN people x ON x.n = i.person
            WHERE i.folder = new.folder AND i.person IS NOT NULL
            ON CONFLICT (folder, person) DO UPDATE SET paths = paths + 1;
    END;
    CREATE TRIGGER folder_paths_removed AFTER DELETE ON folder_paths BEGIN
        DELETE FROM folder_chats WHERE folder = old.ancestor AND paths = 1
            AND chat IN (SELECT chat FROM folder_items WHERE folder = old.folder AND chat IS NOT NULL);
        UPDATE folder_chats SET paths = paths - 1 WHERE folder = old.ancestor
            AND chat IN (SELECT chat FROM folder_items WHERE folder = old.folder AND chat IS NOT NULL);
        DELETE FROM folder_chats WHERE folder = old.ancestor AND paths = 1 AND chat IN
            (SELECT c.n FROM folder_items i JOIN chats c ON c.person = i.person WHERE i.folder = old.folder AND i.person IS NOT NULL);
        UPDATE folder_chats SET paths = paths - 1 WHERE folder = old.ancestor AND chat IN
            (SELECT c.n FROM folder_items i JOIN chats c ON c.person = i.person WHERE i.folder = old.folder AND i.person IS NOT NULL);
        DELETE FROM folder_people WHERE folder = old.ancestor AND paths = 1
            AND person IN (SELECT person FROM folder_items WHERE folder = old.folder AND person IS NOT NULL);
        UPDATE folder_people SET paths = paths - 1 WHERE folder = old.ancestor
            AND person IN (SELECT person FROM folder_items WHERE folder = old.folder AND person IS NOT NULL);
    END;

    -- An item filed or unfiled counts in its folder and every folder above it.
    CREATE TRIGGER folder_items_filed AFTER INSERT ON folder_items BEGIN
        INSERT INTO folder_chats (folder, chat, paths, sort, unread, mentions)
            SELECT p.ancestor, c.n, 1, c.sort, c.unread, c.mentions FROM folder_paths p JOIN chats c ON c.n = new.chat
            WHERE p.folder = new.folder
            ON CONFLICT (folder, chat) DO UPDATE SET paths = paths + 1;
        INSERT INTO folder_chats (folder, chat, paths, sort, unread, mentions)
            SELECT p.ancestor, c.n, 1, c.sort, c.unread, c.mentions FROM folder_paths p JOIN chats c ON c.person = new.person
            WHERE p.folder = new.folder
            ON CONFLICT (folder, chat) DO UPDATE SET paths = paths + 1;
        INSERT INTO folder_people (folder, person, paths, lone, roster_sort)
            SELECT p.ancestor, x.n, 1, x.direct_chats = 0, x.roster_sort FROM folder_paths p JOIN people x ON x.n = new.person
            WHERE p.folder = new.folder
            ON CONFLICT (folder, person) DO UPDATE SET paths = paths + 1;
    END;
    CREATE TRIGGER folder_items_unfiled AFTER DELETE ON folder_items BEGIN
        DELETE FROM folder_chats WHERE paths = 1 AND chat = old.chat
            AND folder IN (SELECT ancestor FROM folder_paths WHERE folder = old.folder);
        UPDATE folder_chats SET paths = paths - 1 WHERE chat = old.chat
            AND folder IN (SELECT ancestor FROM folder_paths WHERE folder = old.folder);
        DELETE FROM folder_chats WHERE paths = 1 AND chat IN (SELECT n FROM chats WHERE person = old.person)
            AND folder IN (SELECT ancestor FROM folder_paths WHERE folder = old.folder);
        UPDATE folder_chats SET paths = paths - 1 WHERE chat IN (SELECT n FROM chats WHERE person = old.person)
            AND folder IN (SELECT ancestor FROM folder_paths WHERE folder = old.folder);
        DELETE FROM folder_people WHERE paths = 1 AND person = old.person
            AND folder IN (SELECT ancestor FROM folder_paths WHERE folder = old.folder);
        UPDATE folder_people SET paths = paths - 1 WHERE person = old.person
            AND folder IN (SELECT ancestor FROM folder_paths WHERE folder = old.folder);
    END;
    CREATE TRIGGER folder_items_refiled AFTER UPDATE OF folder, chat, person ON folder_items BEGIN
        DELETE FROM folder_chats WHERE paths = 1 AND chat = old.chat
            AND folder IN (SELECT ancestor FROM folder_paths WHERE folder = old.folder);
        UPDATE folder_chats SET paths = paths - 1 WHERE chat = old.chat
            AND folder IN (SELECT ancestor FROM folder_paths WHERE folder = old.folder);
        DELETE FROM folder_chats WHERE paths = 1 AND chat IN (SELECT n FROM chats WHERE person = old.person)
            AND folder IN (SELECT ancestor FROM folder_paths WHERE folder = old.folder);
        UPDATE folder_chats SET paths = paths - 1 WHERE chat IN (SELECT n FROM chats WHERE person = old.person)
            AND folder IN (SELECT ancestor FROM folder_paths WHERE folder = old.folder);
        DELETE FROM folder_people WHERE paths = 1 AND person = old.person
            AND folder IN (SELECT ancestor FROM folder_paths WHERE folder = old.folder);
        UPDATE folder_people SET paths = paths - 1 WHERE person = old.person
            AND folder IN (SELECT ancestor FROM folder_paths WHERE folder = old.folder);
        INSERT INTO folder_chats (folder, chat, paths, sort, unread, mentions)
            SELECT p.ancestor, c.n, 1, c.sort, c.unread, c.mentions FROM folder_paths p JOIN chats c ON c.n = new.chat
            WHERE p.folder = new.folder
            ON CONFLICT (folder, chat) DO UPDATE SET paths = paths + 1;
        INSERT INTO folder_chats (folder, chat, paths, sort, unread, mentions)
            SELECT p.ancestor, c.n, 1, c.sort, c.unread, c.mentions FROM folder_paths p JOIN chats c ON c.person = new.person
            WHERE p.folder = new.folder
            ON CONFLICT (folder, chat) DO UPDATE SET paths = paths + 1;
        INSERT INTO folder_people (folder, person, paths, lone, roster_sort)
            SELECT p.ancestor, x.n, 1, x.direct_chats = 0, x.roster_sort FROM folder_paths p JOIN people x ON x.n = new.person
            WHERE p.folder = new.folder
            ON CONFLICT (folder, person) DO UPDATE SET paths = paths + 1;
    END;

    -- What a folder's chats and people bring to its counts.
    CREATE TRIGGER folder_chats_added AFTER INSERT ON folder_chats BEGIN
        UPDATE folder_counts SET chats = chats + 1, unread = unread + new.unread, mentions = mentions + new.mentions
        WHERE folder = new.folder;
    END;
    CREATE TRIGGER folder_chats_removed AFTER DELETE ON folder_chats BEGIN
        UPDATE folder_counts SET chats = chats - 1, unread = unread - old.unread, mentions = mentions - old.mentions
        WHERE folder = old.folder;
    END;
    CREATE TRIGGER folder_chats_recounted AFTER UPDATE OF unread, mentions ON folder_chats BEGIN
        UPDATE folder_counts SET unread = unread + new.unread - old.unread, mentions = mentions + new.mentions - old.mentions
        WHERE folder = new.folder;
    END;
    CREATE TRIGGER folder_people_added AFTER INSERT ON folder_people WHEN new.lone BEGIN
        UPDATE folder_counts SET lone = lone + 1 WHERE folder = new.folder;
    END;
    CREATE TRIGGER folder_people_removed AFTER DELETE ON folder_people WHEN old.lone BEGIN
        UPDATE folder_counts SET lone = lone - 1 WHERE folder = old.folder;
    END;
    CREATE TRIGGER folder_people_recounted AFTER UPDATE OF lone ON folder_people WHEN new.lone IS NOT old.lone BEGIN
        UPDATE folder_counts SET lone = lone + new.lone - old.lone WHERE folder = new.folder;
    END;

    -- Someone's first chat of their own, or their last going, and where they sort.
    CREATE TRIGGER people_chatted AFTER UPDATE OF direct_chats ON people
    WHEN (new.direct_chats = 0) IS NOT (old.direct_chats = 0) BEGIN
        UPDATE folder_people SET lone = new.direct_chats = 0 WHERE person = new.n;
    END;
    CREATE TRIGGER people_resorted AFTER UPDATE OF presence, last_seen, name, push_name, verified_name ON people
    WHEN new.roster_sort IS NOT old.roster_sort BEGIN
        UPDATE folder_people SET roster_sort = new.roster_sort WHERE person = new.n;
    END;

    -- A chat in the built-in folders it belongs to, in the folders its person is filed in, and in
    -- its person's count.
    CREATE TRIGGER chats_counted AFTER INSERT ON chats BEGIN
        UPDATE people SET direct_chats = direct_chats + 1 WHERE n = new.person;
        INSERT INTO folder_chats (folder, chat, paths, sort, unread, mentions)
            SELECT folder, new.n, paths, new.sort, new.unread, new.mentions FROM folder_people WHERE person = new.person
            ON CONFLICT (folder, chat) DO UPDATE SET paths = paths + excluded.paths;
        UPDATE folder_counts SET chats = chats + 1, unread = unread + new.unread, mentions = mentions + new.mentions
        WHERE folder IN (SELECT n FROM folders WHERE builtin IN (iif(new.archived = 0, 'all', NULL),
            iif(new.archived = 0 AND (new.unread > 0 OR new.marked_unread <> 0), 'unread', NULL),
            iif(new.archived = 0 AND new.kind IN ('group', 'announcement'), 'groups', NULL),
            iif(new.archived = 0 AND (new.kind = 'community' OR new.community IS NOT NULL), 'communities', NULL),
            iif(new.archived <> 0, 'archived', NULL)));
    END;
    -- Gone, as a chat merged into another goes: its folder_chats rows go with it.
    CREATE TRIGGER chats_uncounted AFTER DELETE ON chats BEGIN
        UPDATE people SET direct_chats = direct_chats - 1 WHERE n = old.person;
        UPDATE folder_counts SET chats = chats - 1, unread = unread - old.unread, mentions = mentions - old.mentions
        WHERE folder IN (SELECT n FROM folders WHERE builtin IN (iif(old.archived = 0, 'all', NULL),
            iif(old.archived = 0 AND (old.unread > 0 OR old.marked_unread <> 0), 'unread', NULL),
            iif(old.archived = 0 AND old.kind IN ('group', 'announcement'), 'groups', NULL),
            iif(old.archived = 0 AND (old.kind = 'community' OR old.community IS NOT NULL), 'communities', NULL),
            iif(old.archived <> 0, 'archived', NULL)));
    END;
    -- Someone else's now, as joining a person's number and LID makes it: out of the folders its
    -- person was filed in, into those its new person is.
    CREATE TRIGGER chats_repersoned AFTER UPDATE OF person ON chats WHEN new.person IS NOT old.person BEGIN
        UPDATE people SET direct_chats = direct_chats - 1 WHERE n = old.person;
        UPDATE people SET direct_chats = direct_chats + 1 WHERE n = new.person;
        DELETE FROM folder_chats WHERE chat = new.n
            AND paths = (SELECT f.paths FROM folder_people f WHERE f.person = old.person AND f.folder = folder_chats.folder);
        UPDATE folder_chats
            SET paths = paths - (SELECT f.paths FROM folder_people f WHERE f.person = old.person AND f.folder = folder_chats.folder)
            WHERE chat = new.n AND folder IN (SELECT folder FROM folder_people WHERE person = old.person);
        INSERT INTO folder_chats (folder, chat, paths, sort, unread, mentions)
            SELECT folder, new.n, paths, new.sort, new.unread, new.mentions FROM folder_people WHERE person = new.person
            ON CONFLICT (folder, chat) DO UPDATE SET paths = paths + excluded.paths;
    END;
    CREATE TRIGGER chats_recounted AFTER UPDATE OF archived, kind, community, unread, mentions, marked_unread ON chats
    WHEN old.archived IS NOT new.archived OR old.kind IS NOT new.kind OR old.community IS NOT new.community
        OR old.unread IS NOT new.unread OR old.mentions IS NOT new.mentions OR old.marked_unread IS NOT new.marked_unread
    BEGIN
        UPDATE folder_chats SET unread = new.unread, mentions = new.mentions
        WHERE chat = new.n AND (unread IS NOT new.unread OR mentions IS NOT new.mentions);
        UPDATE folder_counts SET chats = chats - 1, unread = unread - old.unread, mentions = mentions - old.mentions
        WHERE folder IN (SELECT n FROM folders WHERE builtin IN (iif(old.archived = 0, 'all', NULL),
            iif(old.archived = 0 AND (old.unread > 0 OR old.marked_unread <> 0), 'unread', NULL),
            iif(old.archived = 0 AND old.kind IN ('group', 'announcement'), 'groups', NULL),
            iif(old.archived = 0 AND (old.kind = 'community' OR old.community IS NOT NULL), 'communities', NULL),
            iif(old.archived <> 0, 'archived', NULL)));
        UPDATE folder_counts SET chats = chats + 1, unread = unread + new.unread, mentions = mentions + new.mentions
        WHERE folder IN (SELECT n FROM folders WHERE builtin IN (iif(new.archived = 0, 'all', NULL),
            iif(new.archived = 0 AND (new.unread > 0 OR new.marked_unread <> 0), 'unread', NULL),
            iif(new.archived = 0 AND new.kind IN ('group', 'announcement'), 'groups', NULL),
            iif(new.archived = 0 AND (new.kind = 'community' OR new.community IS NOT NULL), 'communities', NULL),
            iif(new.archived <> 0, 'archived', NULL)));
    END;
    CREATE TRIGGER chats_resorted AFTER UPDATE OF last_at, pinned ON chats WHEN new.sort IS NOT old.sort BEGIN
        UPDATE folder_chats SET sort = new.sort WHERE chat = new.n;
    END;
    -- A community sorts by the newest of its groups' messages.
    CREATE TRIGGER chats_community_followed AFTER UPDATE OF last_at, community ON chats WHEN new.community IS NOT NULL
    BEGIN
        UPDATE chats SET last_at = new.last_at WHERE id = new.community AND last_at < new.last_at;
    END;
";

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
    Runner::default().run(db)
}

/// Runs the migrations. Tests can make one statement fail, as a fault would, and say how much
/// space is free.
#[derive(Default)]
pub(super) struct Runner {
    /// The statement, counted from 0 across the run, that fails.
    fail_at: Option<usize>,
    /// The free space to take, instead of asking the file system.
    free: Option<u64>,
    /// How many statements have run.
    ran: usize,
}

impl Runner {
    fn run(&mut self, db: &mut Connection) -> Result<()> {
        let locked = locked(LOCK)?;
        if locked.len() != MIGRATIONS.len() {
            bail!(
                "migrations.lock has {} migrations and schema.rs {}; each migration needs its line (see the top of migrations.lock)",
                locked.len(),
                MIGRATIONS.len()
            );
        }
        if let Some(n) =
            MIGRATIONS.iter().zip(&locked).position(|(migration, locked)| sha256(migration) != locked.sha256)
        {
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
        let steps = steps(&kinds, version);
        // A new database has nothing to keep.
        let file = db.path().filter(|path| !path.is_empty()).map(PathBuf::from);
        if let Some(path) = file
            && version > 0
            && steps.iter().any(|step| kinds[step.start] == Kind::Data)
        {
            back_up(db, &path, version, self.free)?;
        }
        // A table rebuilt drops the old one, which with foreign keys on would delete every row that
        // refers to it, so they're off while the migrations run, and a data step checks them before
        // it commits.
        let keys: bool = db.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
        db.pragma_update(None, "foreign_keys", false)?;
        let ran = steps.into_iter().try_for_each(|Range { start: at, end }| {
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for (n, migration) in MIGRATIONS.iter().enumerate().take(end).skip(at) {
                self.execute(&tx, migration).with_context(|| format!("jaw.db's migration {}", n + 1))?;
            }
            if kinds[at] == Kind::Data
                && let Some(table) =
                    tx.prepare("PRAGMA foreign_key_check")?.query_row([], |r| r.get::<_, String>(0)).optional()?
            {
                bail!("jaw.db's migration {end} left a row in {table} that refers to nothing, so it was undone");
            }
            tx.pragma_update(None, "application_id", APPLICATION_ID)?;
            tx.pragma_update(None, "user_version", end as i64)?;
            if fingerprint(&tx)? != expected(end)? {
                bail!("jaw.db's migrations to version {end} didn't make the schema they make on an empty database");
            }
            tx.commit()?;
            anyhow::Ok(())
        });
        db.pragma_update(None, "foreign_keys", keys)?;
        ran?;
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

    /// Runs a migration's statements one at a time.
    fn execute(&mut self, db: &Connection, migration: &str) -> Result<()> {
        let mut statements = Batch::new(db, migration);
        while let Some(mut statement) = statements.next()? {
            if self.fail_at == Some(self.ran) {
                bail!("a fault at statement {}", self.ran);
            }
            statement.raw_execute()?;
            self.ran += 1;
        }
        Ok(())
    }
}

/// The name a backup of jaw.db before migrating from `version` takes beside it.
fn backup_name(version: usize) -> String {
    format!("{BACKUP}{version}")
}

const BACKUP: &str = "jaw.db.before-v";

/// The free space, as a multiple of jaw.db, that an upgrade with data steps needs beside it: the
/// backup, the step's WAL (one transaction, as big as what it writes) and jaw.db's own growth,
/// since a rebuilt table's new pages come before the old ones are freed. Migration 6 took 2.48 to
/// 2.64 times jaw.db from 50,000 to 1,000,000 messages (`measure_the_room_the_data_steps_take`),
/// and this leaves a margin over that.
const DATA_STEP_ROOM: f64 = 3.0;

/// Copies jaw.db, as it is at `version`, to a backup beside it before a data step changes its rows,
/// once there's room for the backup and the step: free space at least DATA_STEP_ROOM times the
/// database. The copy is synced, then renamed into place and its folder synced, so a backup found
/// is whole.
fn back_up(db: &Connection, path: &Path, version: usize, free: Option<u64>) -> Result<()> {
    let folder = path.parent().context("jaw.db has no folder")?;
    let pages: i64 = db.pragma_query_value(None, "page_count", |row| row.get(0))?;
    let page: i64 = db.pragma_query_value(None, "page_size", |row| row.get(0))?;
    let needed = ((pages * page).max(0) as f64 * DATA_STEP_ROOM) as u64;
    let free = match free {
        Some(free) => free,
        None => free_space(folder)?,
    };
    if free < needed {
        bail!(
            "upgrading jaw.db needs {:.0} MB free beside it, for a backup and the upgrade, and there's {:.0} MB: \
             make room and start jawd again",
            needed as f64 / 1e6,
            free as f64 / 1e6
        );
    }
    let backup = folder.join(backup_name(version));
    let part = folder.join(format!("{}.part", backup_name(version)));
    match std::fs::remove_file(&part) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    let into = part.to_str().context("jaw.db's folder isn't UTF-8")?;
    db.execute("VACUUM INTO ?1", [into]).context("can't back jaw.db up before upgrading it")?;
    File::open(&part)?.sync_all()?;
    std::fs::rename(&part, &backup)?;
    File::open(folder)?.sync_all()?;
    // The newest backup holds everything an older one does, so only it is kept.
    for older in backups(folder)?.into_iter().filter(|older| older.path != backup) {
        std::fs::remove_file(older.path)?;
    }
    File::open(folder)?.sync_all()?;
    Ok(())
}

/// How long the backup taken before an upgrade is kept: long enough to notice what the upgrade
/// got wrong, and to recover what it lost, before a clean stop removes it.
pub(super) const KEEP_BACKUP: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 3600);

/// A backup of jaw.db beside it: one taken before an upgrade, or one half written.
pub(super) struct Backup {
    pub path: PathBuf,
    pub bytes: u64,
    pub taken: std::time::SystemTime,
    pub whole: bool,
}

/// The backups beside jaw.db in `folder`, oldest first.
pub(super) fn backups(folder: &Path) -> Result<Vec<Backup>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        if name.starts_with(BACKUP) {
            let meta = entry.metadata()?;
            found.push(Backup {
                path: entry.path(),
                bytes: meta.len(),
                taken: meta.modified()?,
                whole: !name.ends_with(".part"),
            });
        }
    }
    found.sort_by_key(|backup| backup.taken);
    Ok(found)
}

/// Removes, at a clean stop, the backups older than KEEP_BACKUP and any half written; says how
/// many went.
pub(super) fn remove_old_backups(folder: &Path, now: std::time::SystemTime) -> Result<usize> {
    let mut removed = 0;
    for backup in backups(folder)? {
        let old = now.duration_since(backup.taken).is_ok_and(|age| age >= KEEP_BACKUP);
        if old || !backup.whole {
            std::fs::remove_file(&backup.path)?;
            removed += 1;
        }
    }
    if removed > 0 {
        File::open(folder)?.sync_all()?;
    }
    Ok(removed)
}

fn free_space(folder: &Path) -> Result<u64> {
    let name = CString::new(folder.as_os_str().as_bytes())?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `name` is a NUL-terminated path and `stats` is a statvfs to fill in.
    if unsafe { libc::statvfs(name.as_ptr(), &mut stats) } != 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("can't read {}'s free space", folder.display()));
    }
    Ok(stats.f_bavail as u64 * stats.f_frsize as u64)
}

/// How many migrations come before the library's, wherever it's numbered.
#[cfg(test)]
pub(super) fn before_the_library() -> usize {
    MIGRATIONS.iter().position(|migration| migration.contains("ADD COLUMN shelf")).expect("the library's migration")
}

#[cfg(test)]
mod kept;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
    fn rebuilding_chats_messages_and_people_keeps_every_row_and_never_reuses_a_key_an_event_names() {
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
            INSERT INTO tombstones (chat, id, at) VALUES (1, 'gone', 5);
            INSERT INTO events (seq, at, type, key, chat) VALUES (1, 0, 'message', 9, 1), (2, 0, 'person', 7, NULL),
                (3, 0, 'chat', 12, 12);",
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
        assert_eq!(
            count("SELECT count(*) FROM tombstones"),
            1,
            "nothing that refers to a chat went with the old table"
        );
        db.execute("INSERT INTO chats (id, kind, v) VALUES ('rui@s.whatsapp.net', 'direct', 3)", []).unwrap();
        assert_eq!(count("SELECT max(n) FROM chats"), 13, "past the chat a removed chat's event names");
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
        for (n, migration) in MIGRATIONS.iter().enumerate() {
            let hash = sha256(migration);
            let Some(locked) = locked.get(n) else {
                panic!(
                    "migration {} has no line in migrations.lock: add `{} schema {hash}`, or `data` for a step whose cost grows with the rows",
                    n + 1,
                    n + 1
                );
            };
            assert_eq!(
                hash,
                locked.sha256,
                "migration {} isn't the text it was merged with; a merged migration is never edited, so add a new one",
                n + 1
            );
        }
        assert_eq!(locked.len(), MIGRATIONS.len(), "migrations.lock has lines for migrations schema.rs doesn't have");
    }

    /// A database at `version`, in memory or in a file, with a row in the inbox and in history.
    fn at(path: Option<&Path>, version: usize) -> Connection {
        let db = match path {
            Some(path) => Connection::open(path).unwrap(),
            None => Connection::open_in_memory().unwrap(),
        };
        for migration in &MIGRATIONS[..version] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", version as i64).unwrap();
        if version > 0 {
            db.execute("INSERT INTO inbox (kind, at, payload) VALUES ('message', 1, x'01')", []).unwrap();
            db.execute("INSERT INTO history_raw (chat, id, at, raw) VALUES ('c', 'i', 2, x'02')", []).unwrap();
        }
        db
    }

    fn user_version(db: &Connection) -> usize {
        db.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0)).unwrap() as usize
    }

    /// The number of the migration whose text is `migration`.
    pub(super) fn number(migration: &str) -> usize {
        MIGRATIONS.iter().position(|text| *text == migration).expect("one of MIGRATIONS") + 1
    }

    fn rows(db: &Connection) -> (i64, i64) {
        db.query_row("SELECT (SELECT count(*) FROM inbox), (SELECT count(*) FROM history_raw)", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap()
    }

    /// How many statements migration `n` runs.
    fn statements(n: usize) -> usize {
        let db = at(None, n - 1);
        let mut statements = Batch::new(&db, MIGRATIONS[n - 1]);
        let mut count = 0;
        while let Some(mut statement) = statements.next().unwrap() {
            statement.raw_execute().unwrap();
            count += 1;
        }
        count
    }

    #[test]
    fn a_fault_at_any_statement_of_any_migration_leaves_the_database_as_it_was_before_that_step() {
        let kinds: Vec<Kind> = locked(LOCK).unwrap().iter().map(|l| l.kind).collect();
        let counts: Vec<usize> = (1..=MIGRATIONS.len()).map(statements).collect();
        assert!(counts.iter().all(|&n| n > 0), "every migration runs statements: {counts:?}");
        for from in 0..MIGRATIONS.len() {
            // The version each pending statement's transaction began at.
            let mut began = Vec::new();
            for step in steps(&kinds, from) {
                for n in step.clone() {
                    began.extend(std::iter::repeat_n(step.start, counts[n]));
                }
            }
            for (k, &start) in began.iter().enumerate() {
                let mut db = at(None, from);
                let fault = Runner { fail_at: Some(k), ..Runner::default() }.run(&mut db).unwrap_err();
                assert!(format!("{fault:#}").contains("a fault at statement"), "from {from}, statement {k}: {fault:#}");
                assert_eq!(user_version(&db), start, "from {from}, statement {k}");
                assert_eq!(fingerprint(&db).unwrap(), expected(start).unwrap(), "from {from}, statement {k}");
                if from > 0 {
                    assert_eq!(rows(&db), (1, 1), "from {from}, statement {k}");
                }
                migrate(&mut db).unwrap();
                assert_eq!(fingerprint(&db).unwrap(), expected(MIGRATIONS.len()).unwrap());
            }
        }
    }

    #[test]
    fn a_data_step_waits_for_a_whole_synced_backup_at_the_old_version_and_a_database_up_to_date_needs_none() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("jaw.db");
        let mut db = at(Some(&path), 2);
        migrate(&mut db).unwrap();
        let backup = Connection::open_with_flags(
            home.path().join("jaw.db.before-v2"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(user_version(&backup), 2);
        assert_eq!(fingerprint(&backup).unwrap(), expected(2).unwrap());
        assert_eq!(rows(&backup), (1, 1));
        let check: String = backup.query_row("PRAGMA quick_check", [], |r| r.get(0)).unwrap();
        assert_eq!(check, "ok");
        assert!(!home.path().join("jaw.db.before-v2.part").exists());

        let other = tempfile::tempdir().unwrap();
        let mut db = at(Some(&other.path().join("jaw.db")), MIGRATIONS.len());
        migrate(&mut db).unwrap();
        assert_eq!(std::fs::read_dir(other.path()).unwrap().count(), 1, "only jaw.db");
    }

    #[test]
    fn an_upgrade_with_a_data_step_and_too_little_room_is_refused_and_changes_nothing() {
        let home = tempfile::tempdir().unwrap();
        let mut db = at(Some(&home.path().join("jaw.db")), 2);
        let refused = Runner { free: Some(10_000), ..Runner::default() }.run(&mut db).unwrap_err().to_string();
        assert!(refused.contains("MB free beside it") && refused.contains("there's 0 MB"), "{refused}");
        assert_eq!(user_version(&db), 2);
        assert_eq!(fingerprint(&db).unwrap(), expected(2).unwrap());
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 1, "no backup");
        assert!(free_space(home.path()).unwrap() > 0);
    }

    /// Measures the library's migration on a jaw.db at the version before it with this many
    /// messages, one in seven of them an item and about one in fifty with a link, as jawd runs it, on
    /// its writer's connection: how long the upgrade takes, its backup and its step, and what it takes
    /// on disk beside jaw.db at its peak:
    /// `JAW_MEASURE_MESSAGES=1000000 cargo test --release -p jawd measure_the_librarys -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, a minute long at a million messages"]
    fn measure_the_librarys_step() {
        let messages: i64 = std::env::var("JAW_MEASURE_MESSAGES").ok().and_then(|n| n.parse().ok()).unwrap_or(200_000);
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("jaw.db");
        let db = Connection::open(&path).unwrap();
        db.pragma_update(None, "journal_mode", "WAL").unwrap();
        let version = super::before_the_library();
        for migration in &MIGRATIONS[..version] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", version as i64).unwrap();
        db.execute_batch(&format!(
            "BEGIN;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 5000)
            INSERT INTO people (n, name, v) SELECT x, 'Person ' || x, 1 FROM c;
            INSERT INTO person_jids (n, jid, person) SELECT n, printf('3519%08d@s.whatsapp.net', n), n FROM people;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 2000)
            INSERT INTO chats (n, id, kind, person, last_at, v)
            SELECT x, printf('3519%08d@s.whatsapp.net', x), 'direct', x, x, 1 FROM c;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < {messages}),
                k(x, kind) AS (SELECT x, CASE WHEN x % 1000 < 90 THEN 'image' WHEN x % 1000 < 110 THEN 'video'
                    WHEN x % 1000 < 120 THEN 'gif' WHEN x % 1000 < 140 THEN 'voice' WHEN x % 1000 < 150 THEN 'document'
                    ELSE 'text' END FROM c)
            INSERT INTO messages (n, chat, id, at, t, sender, mine, kind, text, spans, summary, detail, raw, v)
            SELECT x, 1 + x % 2000, printf('3EB0%016d', x), 1790000000000 + x * 1000, (1790000000000 + x * 1000) << 20,
                iif(x % 7 = 0, NULL, 1 + x % 5000), x % 7 = 0, kind,
                CASE WHEN kind = 'text' AND x % 40 = 1 THEN 'vê https://www.ipma.pt/previsao/' || x
                    WHEN kind = 'text' THEN 'mensagem número ' || x || ' com algumas palavras de conversa'
                    WHEN kind = 'image' THEN iif(x % 3 = 0, 'na praia com a família ' || x, NULL)
                    WHEN kind = 'document' THEN 'relatorio-' || x || '.pdf' END,
                iif(kind = 'text' AND x % 40 = 1, json_array(json_object('t', 'text', 's', 'vê '),
                    json_object('t', 'link', 's', 'https://www.ipma.pt/previsao/' || x,
                        'url', 'https://www.ipma.pt/previsao/' || x)), NULL),
                'mensagem ' || x,
                iif(kind = 'text', NULL, json_object('media', json_object('mime', 'image/jpeg', 'w', 1600, 'h', 1200))),
                randomblob(300), x
            FROM k;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 50000)
            INSERT INTO events (seq, at, type, key, chat) SELECT x, 0, 'message', {messages} - x, 1 + x % 2000 FROM c;
            COMMIT;
            PRAGMA wal_checkpoint(TRUNCATE);"
        ))
        .unwrap();
        let bytes = |db: &Connection| {
            let pages: i64 = db.pragma_query_value(None, "page_count", |r| r.get(0)).unwrap();
            let page: i64 = db.pragma_query_value(None, "page_size", |r| r.get(0)).unwrap();
            (pages * page) as u64
        };
        let before = bytes(&db);
        drop(db);
        // As jawd opens jaw.db and runs what's pending.
        let mut db = crate::store::connect(&path, crate::store::Role::Writer).unwrap();
        let (stop, peak) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicU64::new(0)));
        let backed_up = Arc::new(std::sync::Mutex::new(None));
        let began = std::time::Instant::now();
        // What's beside jaw.db at its peak, and when the backup is whole, which splits the upgrade
        // into the backup and the step.
        let sampler = {
            let (stop, peak, backed_up, folder) =
                (stop.clone(), peak.clone(), backed_up.clone(), home.path().to_path_buf());
            let backup = backup_name(version);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let files: Vec<(String, u64)> = std::fs::read_dir(&folder)
                        .unwrap()
                        .filter_map(|e| {
                            let e = e.ok()?;
                            Some((e.file_name().to_string_lossy().into_owned(), e.metadata().ok()?.len()))
                        })
                        .collect();
                    peak.fetch_max(files.iter().map(|(_, len)| len).sum(), Ordering::Relaxed);
                    let mut whole = backed_up.lock().unwrap();
                    if whole.is_none() && files.iter().any(|(name, _)| *name == backup) {
                        *whole = Some(std::time::Instant::now());
                    }
                    drop(whole);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            })
        };
        Runner::default().run(&mut db).unwrap();
        let took = began.elapsed();
        stop.store(true, Ordering::Relaxed);
        sampler.join().unwrap();
        let backup = backed_up.lock().unwrap().expect("a backup before the step").duration_since(began);
        let items: i64 = db
            .query_row("SELECT sum(items) FROM shelves WHERE chat = 0 AND shelf <> 'link'", [], |r| r.get(0))
            .unwrap();
        let links: i64 = db.query_row("SELECT count(*) FROM library_links", [], |r| r.get(0)).unwrap();
        let beside = peak.load(Ordering::Relaxed).saturating_sub(before);
        eprintln!(
            "{messages} messages, {items} items and {links} links: jaw.db {:.0} MB at version {version}; upgraded in {:.1} s, \
             its backup {:.1} s and the step {:.2} s; {:.0} MB beside jaw.db at its peak, {:.2} times it \
             (DATA_STEP_ROOM is {DATA_STEP_ROOM}); jaw.db after: {:.0} MB; load {:.0}",
            before as f64 / 1e6,
            took.as_secs_f64(),
            backup.as_secs_f64(),
            took.saturating_sub(backup).as_secs_f64(),
            beside as f64 / 1e6,
            beside as f64 / before as f64,
            bytes(&db) as f64 / 1e6,
            load(),
        );
        assert!((beside as f64) < before as f64 * DATA_STEP_ROOM, "the step took more room than is asked for");
    }

    /// The machine's load average over the last minute.
    fn load() -> f64 {
        let mut loads = [0f64; 3];
        // SAFETY: getloadavg fills at most as many values as it's given room for.
        unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
        loads[0]
    }

    /// Measures what the data steps take on disk beside jaw.db at their peak, for DATA_STEP_ROOM:
    /// `JAW_MEASURE_MESSAGES=1000000 cargo test --release -p jawd measure_the_room -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, minutes long at a million messages"]
    fn measure_the_room_the_data_steps_take() {
        let messages: i64 = std::env::var("JAW_MEASURE_MESSAGES").ok().and_then(|n| n.parse().ok()).unwrap_or(200_000);
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("jaw.db");
        let mut db = Connection::open(&path).unwrap();
        db.pragma_update(None, "journal_mode", "WAL").unwrap();
        for migration in &MIGRATIONS[..5] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", 5).unwrap();
        db.execute_batch(&format!(
            "BEGIN;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 5000)
            INSERT INTO people (n, name, v) SELECT x, 'Person ' || x, 1 FROM c;
            INSERT INTO person_jids (n, jid, person) SELECT n, printf('3519%08d@s.whatsapp.net', n), n FROM people;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 2000)
            INSERT INTO chats (n, id, kind, person, last_at, v) SELECT x, printf('3519%08d@s.whatsapp.net', x), 'direct', x, x, 1 FROM c;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < {messages})
            INSERT INTO messages (n, chat, id, at, t, sender, kind, text, spans, summary, raw, v)
            SELECT x, 1 + x % 2000, printf('3EB0%016d', x), 1790000000000 + x * 1000, (1790000000000 + x * 1000) << 20,
                1 + x % 5000, 'text', 'mensagem número ' || x || ' com algumas palavras de conversa',
                json_array(json_object('t', 'text', 's', 'mensagem número ' || x)), 'mensagem ' || x, randomblob(300), x
            FROM c;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 50000)
            INSERT INTO events (seq, at, type, key, chat) SELECT x, 0, 'message', {messages} - x, 1 + x % 2000 FROM c;
            COMMIT;
            PRAGMA wal_checkpoint(TRUNCATE);"
        ))
        .unwrap();
        let bytes = |db: &Connection| {
            let pages: i64 = db.pragma_query_value(None, "page_count", |r| r.get(0)).unwrap();
            let page: i64 = db.pragma_query_value(None, "page_size", |r| r.get(0)).unwrap();
            (pages * page) as u64
        };
        let before = bytes(&db);
        let (stop, peak) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicU64::new(0)));
        let sampler = {
            let (stop, peak, folder) = (stop.clone(), peak.clone(), home.path().to_path_buf());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let used: u64 = std::fs::read_dir(&folder)
                        .unwrap()
                        .filter_map(|e| e.ok()?.metadata().ok())
                        .map(|m| m.len())
                        .sum();
                    peak.fetch_max(used, Ordering::Relaxed);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            })
        };
        let began = std::time::Instant::now();
        Runner::default().run(&mut db).unwrap();
        let took = began.elapsed();
        stop.store(true, Ordering::Relaxed);
        sampler.join().unwrap();
        let peak = peak.load(Ordering::Relaxed);
        let beside = peak.saturating_sub(before);
        eprintln!(
            "{messages} messages: jaw.db {:.0} MB at version 5; upgraded in {:.1} s; the folder peaked at {:.0} MB, \
             {:.0} MB beside jaw.db, {:.2} times it (DATA_STEP_ROOM is {DATA_STEP_ROOM}); jaw.db after: {:.0} MB",
            before as f64 / 1e6,
            took.as_secs_f64(),
            peak as f64 / 1e6,
            beside as f64 / 1e6,
            beside as f64 / before as f64,
            bytes(&db) as f64 / 1e6,
        );
        assert!((beside as f64) < before as f64 * DATA_STEP_ROOM, "the data steps took more room than is asked for");
    }

    /// Measures the folders' migration on an account the size of `jawd demo`'s: 5,000 people, 3,000
    /// chats (2,400 of them people's, 600 groups, 60 of those in communities), as many messages as
    /// asked, and 30 folders from before it holding 600 chats and people; its backup, its
    /// transaction and its check of every foreign key included, as jawd runs it on its writer's
    /// connection:
    /// `JAW_MEASURE_MESSAGES=1000000 cargo test --release -p jawd measure_the_folders_migration -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, a minute long at a million messages"]
    fn measure_the_folders_migration() {
        let messages: i64 = std::env::var("JAW_MEASURE_MESSAGES").ok().and_then(|n| n.parse().ok()).unwrap_or(200_000);
        let migration = number(FOLDERS_AND_THE_ROSTER);
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("jaw.db");
        let mut db = at(Some(&path), migration - 1);
        db.pragma_update(None, "journal_mode", "WAL").unwrap();
        // jawd's writer's connection: no checkpoint in a commit, and its cache.
        db.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        db.pragma_update(None, "cache_size", -32 * 1024).unwrap();
        db.execute_batch(&format!(
            "BEGIN;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 5000)
            INSERT INTO people (n, name, last_seen, v) SELECT x, 'Person ' || x, 1790000000000 + x, 1 FROM c;
            INSERT INTO person_jids (n, jid, person) SELECT n, printf('3519%08d@s.whatsapp.net', n), n FROM people;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 3000)
            INSERT INTO chats (n, id, kind, person, community, unread, mentions, last_at, v)
            SELECT x, iif(x <= 2400, printf('3519%08d@s.whatsapp.net', x), printf('120363%012d@g.us', x)),
                iif(x <= 2400, 'direct', 'group'), iif(x <= 2400, x, NULL), iif(x > 2940, 2401 + x % 6, NULL),
                x % 7, iif(x % 13 = 0, 1, 0), 1790000000000 + x * 1000, 1 FROM c;
            UPDATE chats SET kind = 'community', community = NULL WHERE n BETWEEN 2401 AND 2406;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < {messages})
            INSERT INTO messages (n, chat, id, at, t, sender, kind, summary, raw, v)
            SELECT x, 1 + x % 3000, printf('3EB0%016d', x), 1790000000000 + x * 1000, (1790000000000 + x * 1000) << 20,
                1 + x % 5000, 'text', 'mensagem ' || x, randomblob(300), x
            FROM c;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 30)
            INSERT INTO folders (n, parent, name, position, favourite, v)
            SELECT x, iif(x <= 10, NULL, 1 + x % 10), 'Folder ' || x, x, iif(x <= 9, x, NULL), 1 FROM c;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 600)
            INSERT OR IGNORE INTO folder_items (folder, chat, person)
            SELECT 1 + x % 30, iif(x % 2 = 0, 1 + (x * 7) % 3000, NULL), iif(x % 2 = 1, 1 + (x * 11) % 5000, NULL) FROM c;
            COMMIT;
            PRAGMA wal_checkpoint(TRUNCATE);"
        ))
        .unwrap();
        let began = std::time::Instant::now();
        back_up(&db, &path, migration - 1, None).unwrap();
        let backup = began.elapsed();
        // The data step as Runner runs it: its statements, its check of every foreign key, the
        // schema's fingerprint and the commit.
        let began = std::time::Instant::now();
        db.pragma_update(None, "foreign_keys", false).unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate).unwrap();
        Runner::default().execute(&tx, FOLDERS_AND_THE_ROSTER).unwrap();
        let dangling: Option<String> =
            tx.prepare("PRAGMA foreign_key_check").unwrap().query_row([], |r| r.get(0)).optional().unwrap();
        assert_eq!(dangling, None);
        tx.pragma_update(None, "user_version", migration as i64).unwrap();
        assert_eq!(fingerprint(&tx).unwrap(), expected(migration).unwrap());
        tx.commit().unwrap();
        let took = began.elapsed();
        let count = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        eprintln!(
            "{messages} messages in 3,000 chats with 5,000 people and 30 folders holding {} chats and people: \
             migration {migration} took {:.2} s, its check of every foreign key, fingerprint and commit included, \
             and the backup before it {:.2} s",
            count("SELECT count(*) FROM folder_items"),
            took.as_secs_f64(),
            backup.as_secs_f64(),
        );
        assert!(super::super::lists::drift(&db).unwrap().is_empty(), "it counts what it holds");
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
    fn the_folders_migration_keeps_the_folders_from_before_it_files_a_persons_chat_as_them_and_counts_what_they_hold() {
        let mut db = at(None, number(FOLDERS_AND_THE_ROSTER) - 1);
        db.execute_batch(
            "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 1);
            INSERT INTO person_jids (n, jid, person) VALUES (1, 'ana@s.whatsapp.net', 1);
            INSERT INTO chats (n, id, kind, person, unread, mentions, last_at, v)
                VALUES (1, 'ana@s.whatsapp.net', 'direct', 1, 2, 1, 50, 1), (2, 'g@g.us', 'group', NULL, 3, 0, 40, 1);
            INSERT INTO folders (n, parent, name, position, favourite, v) VALUES (7, NULL, 'Family', 0, 4, 1),
                (8, 7, 'Cousins', 0, NULL, 1);
            INSERT INTO folder_items (folder, chat, person) VALUES (7, 1, NULL), (8, 2, NULL), (8, NULL, 1);",
        )
        .unwrap();
        migrate(&mut db).unwrap();
        let count = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(count("SELECT count(*) FROM folders WHERE n IN (7, 8) AND builtin IS NULL AND rule IS NULL"), 2);
        assert_eq!(count("SELECT folder FROM favourites WHERE slot = 4"), 7, "the slot Family had");
        assert_eq!(
            count("SELECT count(*) FROM folder_items WHERE chat IS NOT NULL AND chat = 1"),
            0,
            "Ana's chat files Ana"
        );
        assert_eq!(count("SELECT count(*) FROM folder_items WHERE folder = 7 AND person = 1"), 1);
        for folder in [7, 8] {
            let counts: (i64, i64, i64) = db
                .query_row(
                    "SELECT unread, mentions, chats + lone FROM folder_counts WHERE folder = ?1",
                    [folder],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
            assert_eq!(counts, (5, 1, 2), "folder {folder}: Ana's chat once, and the group");
        }
        assert_eq!(
            count("SELECT chats FROM folder_counts k JOIN folders f ON f.n = k.folder WHERE f.builtin = 'all'"),
            2
        );
        assert_eq!(
            count("SELECT count(*) FROM folders WHERE rule IS NOT NULL"),
            3,
            "the three smart folders jaw starts with"
        );
        assert!(
            super::super::lists::drift(&db).unwrap().is_empty(),
            "counted as what they're derived from counts them"
        );
        db.execute("INSERT INTO folders (name, v) VALUES ('Later', 1)", []).unwrap();
        assert!(count("SELECT max(n) FROM folders") > 8 + 8, "a folder made later takes an n past every one before");
    }

    #[test]
    fn the_folders_migration_gives_the_chat_rows_it_changes_new_versions_with_their_events() {
        let db = at(None, number(FOLDERS_AND_THE_ROSTER) - 1);
        db.execute_batch(
            "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 1);
            INSERT INTO chats (n, id, kind, person, community, last_at, v) VALUES
                (1, 'ana@s.whatsapp.net', 'direct', 1, NULL, 10, 4), (2, 'bairro@g.us', 'community', NULL, NULL, 20, 5),
                (3, 'festa@g.us', 'group', NULL, 'bairro@g.us', 50, 6), (4, 'escalada@g.us', 'group', NULL, NULL, 30, 7),
                (5, 'rui@s.whatsapp.net', 'direct', NULL, NULL, 40, 8);
            INSERT INTO folders (n, name, v) VALUES (7, 'Family', 1);
            INSERT INTO folder_items (folder, chat, person) VALUES (7, 4, NULL), (7, NULL, 1);
            INSERT INTO events (seq, at, type, key, chat) VALUES (8, 0, 'chat', 5, 5);",
        )
        .unwrap();
        // Migration 19 alone, as the Runner runs it: the migrations after it give versions of their own.
        db.pragma_update(None, "foreign_keys", false).unwrap();
        Runner::default().execute(&db, FOLDERS_AND_THE_ROSTER).unwrap();
        // An interface holding every row as of seq 8 is sent again those with a version past it.
        let versions: Vec<(i64, i64)> = db
            .prepare("SELECT n, v FROM chats ORDER BY n")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            versions,
            [(1, 9), (2, 10), (3, 6), (4, 11), (5, 8)],
            "Ana's chat, as she's filed; Bairro, which shows its groups now; Escalada, filed"
        );
        let events: Vec<(i64, String, i64, i64)> = db
            .prepare("SELECT seq, type, key, chat FROM events WHERE seq > 8 ORDER BY seq")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let chat = |seq: i64, n: i64| (seq, "chat".to_string(), n, n);
        assert_eq!(events, [chat(9, 1), chat(10, 2), chat(11, 4)], "each new version is its event's seq");
    }

    /// A community whose newest group left it, one jaw learned after its groups, and one with a
    /// message of its own, as migration 19's trigger left them.
    fn communities_before_they_were_gathered() -> Connection {
        let db = at(None, number(COMMUNITIES_GATHERED) - 1);
        db.execute_batch(
            "INSERT INTO chats (n, id, kind, community, last_at, last_message, v) VALUES
                (1, 'bairro@g.us', 'community', NULL, 50, NULL, 5), (2, 'festa@g.us', 'group', 'bairro@g.us', 20, NULL, 6),
                (3, 'escalada@g.us', 'group', NULL, 50, NULL, 7),
                (4, 'vela@g.us', 'community', NULL, 0, NULL, 8), (5, 'regata@g.us', 'group', 'vela@g.us', 40, NULL, 9),
                (6, 'xadrez@g.us', 'community', NULL, 60, 1, 10), (7, 'ana@s.whatsapp.net', 'direct', NULL, 70, NULL, 11);
            INSERT INTO messages (n, chat, id, at, t, kind, summary, v) VALUES
                (1, 6, 'x1', 30, 30, 'system', 'Community created: Xadrez', 10);
            INSERT INTO events (seq, at, type, key, chat) VALUES (11, 0, 'chat', 7, 7);",
        )
        .unwrap();
        db
    }

    #[test]
    fn the_communities_migration_sorts_each_community_by_its_own_messages_and_lets_the_trigger_go() {
        let db = communities_before_they_were_gathered();
        db.execute_batch(COMMUNITIES_GATHERED).unwrap();
        let last_at =
            |n: i64| db.query_row("SELECT last_at FROM chats WHERE n = ?1", [n], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(
            (last_at(1), last_at(4), last_at(6)),
            (0, 0, 30),
            "Bairro no longer at the time of Escalada, which left it, and Xadrez at its own message's"
        );
        assert_eq!((last_at(2), last_at(5), last_at(7)), (20, 40, 70), "the other chats as they were");
        db.execute("UPDATE chats SET last_at = 90 WHERE n = 2", []).unwrap();
        assert_eq!(last_at(1), 0, "a group's message no longer moves its community's own last_at");
        let index: i64 = db
            .query_row("SELECT count(*) FROM sqlite_schema WHERE name = 'chats_known_communities'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(index, 1);
    }

    #[test]
    fn the_communities_migration_gives_communities_and_their_groups_new_versions_with_their_events() {
        let db = communities_before_they_were_gathered();
        db.execute_batch(COMMUNITIES_GATHERED).unwrap();
        let versions: Vec<(i64, i64)> = db
            .prepare("SELECT n, v FROM chats ORDER BY n")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            versions,
            [(1, 12), (2, 13), (3, 7), (4, 14), (5, 15), (6, 16), (7, 11)],
            "each community and each group naming one, after the feed's last seq; Escalada and Ana's chat as they were"
        );
        let events: Vec<(i64, i64)> = db
            .prepare("SELECT seq, key FROM events WHERE seq > 11 AND type = 'chat' AND chat = key ORDER BY seq")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(events, [(12, 1), (13, 2), (14, 4), (15, 5), (16, 6)], "each new version is its event's seq");
    }

    /// Migration 28 beside a million messages, 3,000 chats and six communities of ten groups each,
    /// in its transaction, as jawd runs it with the schema steps before it:
    /// `JAW_MEASURE_MESSAGES=1000000 cargo test --release -p jawd measure_the_communities_migration -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, a minute long at a million messages"]
    fn measure_the_communities_migration() {
        let messages: i64 = std::env::var("JAW_MEASURE_MESSAGES").ok().and_then(|n| n.parse().ok()).unwrap_or(200_000);
        let migration = number(COMMUNITIES_GATHERED);
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("jaw.db");
        let mut db = at(Some(&path), migration - 1);
        db.pragma_update(None, "journal_mode", "WAL").unwrap();
        db.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        db.pragma_update(None, "cache_size", -32 * 1024).unwrap();
        db.execute_batch(&format!(
            "BEGIN;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 5000)
            INSERT INTO people (n, name, last_seen, v) SELECT x, 'Person ' || x, 1790000000000 + x, 1 FROM c;
            INSERT INTO person_jids (n, jid, person) SELECT n, printf('3519%08d@s.whatsapp.net', n), n FROM people;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 3000)
            INSERT INTO chats (n, id, kind, person, community, unread, mentions, last_at, v)
            SELECT x, iif(x <= 2400, printf('3519%08d@s.whatsapp.net', x), printf('120363%012d@g.us', x)),
                iif(x <= 2400, 'direct', 'group'), iif(x <= 2400, x, NULL),
                iif(x > 2940, printf('120363%012d@g.us', 2401 + x % 6), NULL),
                x % 7, iif(x % 13 = 0, 1, 0), 1790000000000 + x * 1000, 1 FROM c;
            UPDATE chats SET kind = 'community', community = NULL WHERE n BETWEEN 2401 AND 2406;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < {messages})
            INSERT INTO messages (n, chat, id, at, t, sender, kind, summary, raw, v)
            SELECT x, 1 + x % 3000, printf('3EB0%016d', x), 1790000000000 + x * 1000, (1790000000000 + x * 1000) << 20,
                1 + x % 5000, 'text', 'mensagem ' || x, randomblob(300), x
            FROM c;
            COMMIT;
            PRAGMA wal_checkpoint(TRUNCATE);"
        ))
        .unwrap();
        let began = std::time::Instant::now();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate).unwrap();
        Runner::default().execute(&tx, COMMUNITIES_GATHERED).unwrap();
        tx.pragma_update(None, "user_version", migration as i64).unwrap();
        assert_eq!(fingerprint(&tx).unwrap(), expected(migration).unwrap());
        tx.commit().unwrap();
        let took = began.elapsed();
        let count = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        eprintln!(
            "{messages} messages in 3,000 chats, {} of them communities and {} their groups: migration {migration} \
             took {:.1} ms, its fingerprint and commit included",
            count("SELECT count(*) FROM chats WHERE kind = 'community'"),
            count("SELECT count(*) FROM chats WHERE community IS NOT NULL"),
            took.as_secs_f64() * 1000.0,
        );
        assert!(super::super::lists::drift(&db).unwrap().is_empty(), "it leaves what folders count as it was");
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
