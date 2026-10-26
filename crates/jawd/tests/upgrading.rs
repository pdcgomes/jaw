//! The owner's first run of this jawd: a jaw.db as the spike left it on 16 October, at version 1,
//! generated here, which this jawd with the demo engine upgrades through every migration since,
//! converting the history it kept; and from then on the interfaces' layouts kept, across a restart.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use jaw_proto::Status;
use prost::Message as _;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

/// The demo engine's Ana and its climbing group, Mãe's chat, which only the spike's history has,
/// and a chat this account doesn't have.
const ANA: &str = "351912345678@s.whatsapp.net";
const CLIMBING: &str = "120363012345678901@g.us";
const MAE: &str = "351915555555@s.whatsapp.net";
const GONE: &str = "351919999999@s.whatsapp.net";
const AT: i64 = 1_792_000_000_000;

/// Migration 1, as the spike ran it on 16 October: the engine's events kept raw, and history as it
/// came. A jaw.db at version 1 holds nothing else.
const VERSION_1: &str = "
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
    ";

/// The schema version this jawd upgrades to: one for each migration in its lock, so the test
/// holds whatever number the newest migration takes when it merges.
fn latest() -> i64 {
    include_str!("../src/store/migrations.lock")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .count() as i64
}

/// The history the spike kept raw: each message's chat, its sender (none for yours), ID, time and
/// words.
const KEPT: [(&str, Option<&str>, &str, i64, &str); 4] = [
    (ANA, Some(ANA), "H1ANA", AT - 300_000, "from before the photos"),
    (ANA, None, "H2ANA", AT - 290_000, "send them when you can"),
    (MAE, Some(MAE), "H1MAE", AT - 200_000, "jantar às 8?"),
    (CLIMBING, Some("351913333333@s.whatsapp.net"), "H1CLIMB", AT - 100_000, "who's in?"),
];

/// `home`'s jaw.db at version 1, with KEPT as the phone sent it.
fn version_1(home: &Path) {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.execute_batch(VERSION_1).unwrap();
    db.pragma_update(None, "user_version", 1).unwrap();
    for (chat, sender, id, at, words) in KEPT {
        let info = WebMessageInfo {
            key: MessageKey {
                remote_jid: Some(chat.into()),
                from_me: Some(sender.is_none()),
                id: Some(id.into()),
                participant: sender.filter(|_| chat.ends_with("@g.us")).map(str::to_string),
            },
            message: Some(e2e::Message { conversation: Some(words.into()), ..Default::default() }),
            message_timestamp: Some((at / 1000) as u64),
            ..Default::default()
        };
        db.execute(
            "INSERT INTO history_raw (chat, id, at, raw) VALUES (?1, ?2, 0, ?3)",
            rusqlite::params![chat, id, info.encode_to_vec()],
        )
        .unwrap();
    }
}

/// jawd on a folder with the demo engine, stopped with SIGTERM, or killed should the test fail
/// first, so it never outlives the test.
struct Jawd {
    child: Child,
    home: PathBuf,
}

impl Jawd {
    fn start(home: &Path) -> Jawd {
        let log = OpenOptions::new().create(true).append(true).open(home.join("jawd.log")).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_jawd"))
            .args(["--engine", "demo"])
            .env("JAW_HOME", home)
            .env("JAW_NOTIFY", "off")
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("jawd starts");
        Jawd { child, home: home.to_path_buf() }
    }

    /// A connection to it as `client`, once it answers.
    fn connect(&self, client: &str) -> jaw_client::Connection {
        let socket = self.home.join("jaw.sock");
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Ok((connection, _)) = jaw_client::Connection::open(&socket, client, "0") {
                return connection;
            }
            assert!(Instant::now() < deadline, "jawd never answered: {}", log(&self.home));
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Stops it with SIGTERM, as launchd and systemd do, and says whether it stopped cleanly.
    fn stop(mut self) -> bool {
        // SAFETY: kill only sends a signal to the child this test started.
        unsafe { libc::kill(self.child.id() as i32, libc::SIGTERM) };
        self.child.wait().unwrap().success()
    }
}

impl Drop for Jawd {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn log(home: &Path) -> String {
    std::fs::read_to_string(home.join("jawd.log")).unwrap_or_default()
}

fn read(home: &Path) -> Connection {
    let db = Connection::open_with_flags(home.join("jaw.db"), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    db
}

fn count(db: &Connection, sql: &str) -> i64 {
    db.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn layouts(connection: &mut jaw_client::Connection) -> jaw_proto::Layouts {
    let status: Status = serde_json::from_value(connection.call("status", Value::Null).unwrap()).unwrap();
    status.layouts.expect("status says what layouts are kept")
}

#[test]
fn the_owners_first_run_upgrades_a_version_1_jaw_db_converts_its_history_and_keeps_layouts_from_then_on() {
    let home = tempfile::tempdir().unwrap();
    version_1(home.path());
    let first = Jawd::start(home.path());
    let mut jaw = first.connect("jaw");
    {
        let db = read(home.path());
        let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, latest(), "upgraded through every migration before the socket answered");
        let id: i64 = db.pragma_query_value(None, "application_id", |r| r.get(0)).unwrap();
        assert_eq!(id, 0x6A61_7701, "given jaw's application_id");
        let deadline = Instant::now() + Duration::from_secs(60);
        while count(&db, "SELECT count(*) FROM history_raw") > 0 {
            assert!(Instant::now() < deadline, "the spike's history was never converted: {}", log(home.path()));
            std::thread::sleep(Duration::from_millis(50));
        }
        for (chat, _, id, _, _) in KEPT {
            let found = db
                .query_row(
                    "SELECT count(*) FROM messages m JOIN chats c ON c.n = m.chat WHERE c.id = ?1 AND m.id = ?2",
                    [chat, id],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap();
            assert_eq!(found, 1, "{id} in {chat}, as the spike kept it");
        }
        assert_eq!(count(&db, "SELECT count(*) FROM quarantine"), 0);
        assert_eq!(db.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0)).unwrap(), "ok");
        assert_eq!(count(&db, "SELECT count(*) FROM pragma_foreign_key_check"), 0);
    }
    assert!(home.path().join("jaw.db.before-v1").exists(), "backed up at version 1 before the data steps");
    assert_eq!(
        layouts(&mut jaw),
        jaw_proto::Layouts { kept: 0, most: 16, largest_bytes: 0, bytes_most: 16_384, refused: 0 }
    );

    // The terminal's panes: Mãe, whose chat the spike's history made, then the group, with the keys
    // on a chat this account doesn't have.
    let three = json!({ "format": 1, "focused": 2, "tree": { "split": { "how": "across", "first": 500,
        "a": { "chat": MAE },
        "b": { "split": { "how": "down", "first": 500, "a": { "chat": CLIMBING }, "b": { "chat": GONE } } } } } });
    let two = json!({ "format": 1, "focused": 1, "tree": { "split": { "how": "across", "first": 500,
        "a": { "chat": MAE }, "b": { "chat": CLIMBING } } } });
    let v = AT as u64 + 1;
    let saved = jaw.call("layout.set", json!({ "v": v, "layout": three })).unwrap();
    assert_eq!(saved, json!({ "interface": "jaw", "v": v, "layout": two, "kept": true, "dropped": [GONE] }));
    let mut mac = first.connect("jaw-mac");
    let opened = mac.call("layout.get", json!({ "interface": "jaw" })).unwrap();
    assert_eq!(opened["layout"], two, "the Mac app opens the terminal's");
    drop((jaw, mac));
    assert!(first.stop(), "a clean stop: {}", log(home.path()));

    let second = Jawd::start(home.path());
    let mut jaw = second.connect("jaw");
    let restored = jaw.call("layout.get", Value::Null).unwrap();
    assert_eq!(restored, json!({ "interface": "jaw", "v": v, "layout": two, "dropped": [GONE] }), "as it was left");
    let bytes =
        serde_json::to_string(&serde_json::from_value::<jaw_proto::layout::Layout>(three).unwrap()).unwrap().len();
    assert_eq!(
        layouts(&mut jaw),
        jaw_proto::Layouts { kept: 1, most: 16, largest_bytes: bytes as u64, bytes_most: 16_384, refused: 0 }
    );
    let backups = std::fs::read_dir(home.path())
        .unwrap()
        .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with("jaw.db.before"))
        .count();
    assert_eq!(backups, 1, "a second start has nothing to upgrade");
    drop(jaw);
    assert!(second.stop(), "a clean stop: {}", log(home.path()));
}
