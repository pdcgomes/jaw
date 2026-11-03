//! jawd's start as the binary runs it, with an engine that never says hello: what jawd says at hello
//! before its engine has said anything, that it knows its own account before it converts what the
//! last run left, and that it leaves its folder out of Time Machine, with Spotlight's marker in it.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use jaw_proto::{AccountState, ConnectionState, Me, Welcome};
use prost::Message as _;
use wa_proto::jaw::engine as pb;
use wa_proto::wa_web_protobufs_e2e as e2e;

/// jawd on `home` with `engine` as its jaw-engine.
fn jawd(home: &Path, engine: &Path) -> Child {
    let log = std::fs::OpenOptions::new().create(true).append(true).open(home.join("jawd.log")).unwrap();
    Command::new(env!("CARGO_BIN_EXE_jawd"))
        .env("JAW_HOME", home)
        .env("JAW_ENGINE", engine)
        .env("JAW_NOTIFY", "off")
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .expect("jawd starts")
}

/// What jawd says at hello, once it answers.
fn hello(home: &Path) -> Welcome {
    let socket = home.join("jaw.sock");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok((_, welcome)) = jaw_client::Connection::open(&socket, "test", "0") {
            return welcome;
        }
        assert!(Instant::now() < deadline, "jawd never answered on {}", socket.display());
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Stops the jawd this test started, with SIGTERM, as launchd and systemd do.
fn stop(mut child: Child) {
    // SAFETY: kill only sends a signal to the child this test started.
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    child.wait().unwrap();
}

/// An engine that starts and never says hello, so jawd hears nothing from it.
fn silent_engine(home: &Path) -> std::path::PathBuf {
    let engine = home.join("silent-engine");
    std::fs::write(&engine, "#!/bin/sh\nexec sleep 600\n").unwrap();
    std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755)).unwrap();
    engine
}

#[test]
fn jawd_starts_linked_as_its_account_and_converts_what_the_last_run_left_as_that_account() {
    let home = tempfile::tempdir().unwrap();
    let engine = silent_engine(home.path());
    let first = jawd(home.path(), &engine);
    assert_eq!(hello(home.path()).account.state, AccountState::Unlinked, "a fresh folder");
    stop(first);
    // What a linked run leaves: the account, its JIDs, and a message mentioning it, kept in the inbox
    // and not converted yet.
    let pedro = Me { id: "351910000001:12@s.whatsapp.net".into(), name: "Pedro".into(), phone: "+351910000001".into() };
    let mention = e2e::Message {
        extended_text_message: Some(Box::new(e2e::ExtendedTextMessage {
            text: Some("@351910000001 are you in?".into()),
            context_info: Some(Box::new(e2e::ContextInfo {
                mentioned_jid: vec!["351910000001@s.whatsapp.net".into()],
                ..Default::default()
            })),
            ..Default::default()
        })),
        ..Default::default()
    };
    let left = pb::Message {
        info: Some(pb::MessageInfo {
            chat: "120363000000000001@g.us".into(),
            sender: "351910000002@s.whatsapp.net".into(),
            group: true,
            id: "left".into(),
            timestamp_ms: 1_792_000_000_000,
            push_name: "Ana".into(),
            ..Default::default()
        }),
        raw: mention.encode_to_vec(),
        ..Default::default()
    };
    {
        let db = rusqlite::Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        let remember = "INSERT OR REPLACE INTO sync_state (key, value) VALUES (?1, ?2)";
        db.execute(remember, ["account", &serde_json::to_string(&pedro).unwrap()]).unwrap();
        db.execute(remember, ["me", "100000000001@lid 351910000001@s.whatsapp.net"]).unwrap();
        db.execute(
            "INSERT INTO inbox (kind, at, payload) VALUES ('message', ?1, ?2)",
            rusqlite::params![1_792_000_000_000i64, left.encode_to_vec()],
        )
        .unwrap();
    }
    let second = jawd(home.path(), &engine);
    let welcome = hello(home.path());
    assert_eq!(welcome.account.state, AccountState::Linked, "linked before the engine has said anything");
    assert_eq!(welcome.account.me, Some(pedro));
    assert_eq!(welcome.connection.state, ConnectionState::Connecting);
    let db = rusqlite::Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mentions_me = loop {
        let row = db.query_row("SELECT mentions_me FROM messages WHERE id = 'left'", [], |r| r.get::<_, bool>(0));
        if let Ok(mentions_me) = row {
            break mentions_me;
        }
        assert!(Instant::now() < deadline, "the message left in the inbox was never converted");
        std::thread::sleep(Duration::from_millis(20));
    };
    drop(db);
    stop(second);
    assert!(mentions_me, "converted knowing whose account it is, so its mention of it counts");
}

/// jaw's folder, which a start leaves out of Time Machine and gives Spotlight's marker, unless it has them.
#[cfg(target_os = "macos")]
mod kept_out {
    use super::{hello, jawd, silent_engine, stop};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::Path;
    use std::process::Command;

    /// What `tmutil addexclusion` writes on a folder it leaves out of Time Machine: the string
    /// "com.apple.backupd" as a binary property list.
    const BY_TMUTIL: &[u8] =
        b"bplist00\x5f\x10\x11com.apple.backupd\x08\0\0\0\0\0\0\x01\x01\0\0\0\0\0\0\0\x01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x1c";

    /// The attribute Time Machine reads on `path`, if it has one.
    fn excluded(path: &Path) -> Option<Vec<u8>> {
        let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        let name = c"com.apple.metadata:com_apple_backup_excludeItem";
        let mut value = vec![0u8; 256];
        // SAFETY: both names end in NUL and outlive the call, and `value` is as long as it's said to be.
        let len = unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), value.as_mut_ptr().cast(), value.len(), 0, 0) };
        value.truncate(usize::try_from(len).ok()?);
        Some(value)
    }

    /// Takes away everyone's right to set `folder`'s attributes, its files left as they were.
    fn deny_attributes(folder: &Path) {
        let denied = Command::new("/bin/chmod").arg("+a").arg("everyone deny writeextattr").arg(folder).status();
        assert!(denied.unwrap().success());
    }

    fn log(home: &Path) -> String {
        std::fs::read_to_string(home.join("jawd.log")).unwrap()
    }

    #[test]
    fn a_start_leaves_its_folder_out_of_time_machine_with_spotlights_marker_and_the_next_changes_neither() {
        let home = tempfile::tempdir().unwrap();
        let engine = silent_engine(home.path());
        let marker = home.path().join(".metadata_never_index");
        assert_eq!(excluded(home.path()), None);
        let first = jawd(home.path(), &engine);
        hello(home.path());
        stop(first);
        assert_eq!(excluded(home.path()).as_deref(), Some(BY_TMUTIL), "as tmutil addexclusion leaves it");
        let made = std::fs::metadata(&marker).expect("Spotlight's marker is there");
        assert!(made.is_file() && made.len() == 0);
        // Neither could be set again, so a start that tried would say it can't.
        deny_attributes(home.path());
        std::fs::set_permissions(&marker, std::fs::Permissions::from_mode(0o444)).unwrap();
        let second = jawd(home.path(), &engine);
        hello(home.path());
        stop(second);
        assert_eq!(excluded(home.path()).as_deref(), Some(BY_TMUTIL));
        let kept = std::fs::metadata(&marker).unwrap();
        assert_eq!((kept.ino(), kept.mtime(), kept.mtime_nsec()), (made.ino(), made.mtime(), made.mtime_nsec()));
        let log = log(home.path());
        assert!(!log.contains("Time Machine") && !log.contains("Spotlight"), "{log}");
    }

    #[test]
    fn jawd_starts_in_a_folder_it_can_neither_leave_out_of_time_machine_nor_mark_for_spotlight_and_says_each() {
        let home = tempfile::tempdir().unwrap();
        let engine = silent_engine(home.path());
        deny_attributes(home.path());
        std::fs::create_dir(home.path().join(".metadata_never_index")).unwrap();
        let started = jawd(home.path(), &engine);
        hello(home.path());
        stop(started);
        assert_eq!(excluded(home.path()), None);
        let log = log(home.path());
        assert_eq!(log.matches("can't leave").count(), 1, "{log}");
        assert_eq!(log.matches("out of Time Machine: Permission denied").count(), 1, "{log}");
        assert_eq!(log.matches("can't put Spotlight's .metadata_never_index in").count(), 1, "{log}");
        assert_eq!(log.matches("Is a directory").count(), 1, "{log}");
    }
}
