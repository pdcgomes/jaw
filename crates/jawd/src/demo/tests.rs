use super::*;

fn plan(folder: &Path, report: &Path) -> Plan {
    Plan {
        folder: folder.to_path_buf(),
        shape: Asked { chats: Some(30), people: Some(40), messages: Some(3_000), chunk: Some(500) },
        steady: false,
        flood: None,
        seconds: None,
        catch_up: None,
        in_flight: 256,
        after: 0.0,
        idle: None,
        report: Some(report.to_path_buf()),
        exit: true,
        notify: false,
    }
}

fn count(folder: &Path, table: &str) -> i64 {
    let db = rusqlite::Connection::open_with_flags(folder.join("jaw.db"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

fn report(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn a_demo_accounts_history_catch_up_and_flood_each_arrive_whole_through_the_engine_and_the_socket() {
    let folder = tempfile::tempdir().unwrap();
    let report_to = folder.path().join("report.json");
    run(plan(folder.path(), &report_to)).unwrap();
    let history = &report(&report_to)["history"];
    assert_eq!(count(folder.path(), "messages"), 3_000 + history["ack"]["n"].as_i64().unwrap());
    assert_eq!(history["blobs"], 6);
    assert_eq!(history["missing"], 0, "every live message reached the interface");
    assert!(history["open"]["n"].as_u64().unwrap() > 0, "chats were opened while history went in");
    let marker: Marker = serde_json::from_slice(&std::fs::read(folder.path().join(MARKER)).unwrap()).unwrap();
    assert!(marker.history, "the folder notes its history is in");
    assert_eq!(std::fs::read_dir(folder.path().join("spool")).unwrap().count(), 0, "every blob left the spool");

    let before = count(folder.path(), "messages");
    let again = Plan {
        shape: Asked::default(),
        catch_up: Some(150),
        in_flight: 1,
        steady: true,
        ..plan(folder.path(), &report_to)
    };
    run(again).unwrap();
    let numbers = report(&report_to);
    let catch_up = &numbers["catch_up"];
    assert_eq!(catch_up["missing"], 0);
    let steady = &numbers["steady"];
    for page in ["page_newest", "page_middle", "page_oldest"] {
        assert!(steady[page]["n"].as_u64().unwrap() > 0, "{page} was read");
    }
    assert!(numbers.get("history").is_none(), "the history came once");
    let arrived = 150 + steady["lone_ack"]["n"].as_i64().unwrap();
    assert_eq!(count(folder.path(), "messages"), before + arrived);

    let before = count(folder.path(), "messages");
    let flood = Plan { flood: Some(400), seconds: Some(1.0), ..plan(folder.path(), &report_to) };
    run(flood).unwrap();
    let flood = &report(&report_to)["flood"];
    assert_eq!(flood["messages"], 400);
    assert_eq!(flood["ack"]["n"], 400, "each message was acked");
    assert_eq!(flood["missing"], 0);
    assert_eq!(count(folder.path(), "messages"), before + 400);
    for table in ["inbox", "quarantine"] {
        assert_eq!(count(folder.path(), table), 0, "{table} is empty");
    }

    let other = Plan { shape: Asked { messages: Some(5_000), ..Asked::default() }, ..plan(folder.path(), &report_to) };
    let refused = run(other).unwrap_err().to_string();
    assert!(refused.contains("holds a made-up account of 3000 messages"), "{refused}");
}

#[test]
fn a_demo_refuses_a_folder_that_holds_a_real_account() {
    let folder = tempfile::tempdir().unwrap();
    let report_to = folder.path().join("report.json");
    std::fs::write(folder.path().join("session.db"), b"an engine's session").unwrap();
    let refused = run(plan(folder.path(), &report_to)).unwrap_err().to_string();
    assert!(refused.contains("session"), "{refused}");

    let other = tempfile::tempdir().unwrap();
    std::fs::write(other.path().join("jaw.db"), b"someone's jaw.db").unwrap();
    let refused = run(plan(other.path(), &report_to)).unwrap_err().to_string();
    assert!(refused.contains("didn't make"), "{refused}");
    assert_eq!(std::fs::read(other.path().join("jaw.db")).unwrap(), b"someone's jaw.db", "it's left as it was");
}

#[test]
fn a_catch_up_on_a_folder_without_its_history_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let report_to = folder.path().join("report.json");
    let refused = run(Plan { catch_up: Some(10), ..plan(folder.path(), &report_to) }).unwrap_err().to_string();
    assert!(refused.contains("history"), "{refused}");
}
