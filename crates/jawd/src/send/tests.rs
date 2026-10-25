use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use jaw_proto::{Push, code};
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};

use super::*;
use crate::engine::demo::{self, DemoEngine, Replies, World};
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::store::outbox::Step;
use crate::store::{Amend, Amendment, Lane, Members, Message, Person};

const ANA: &str = "351912345678@s.whatsapp.net";
const CLIMBING: &str = "120363012345678901@g.us";
const RUI: &str = "351913333333@s.whatsapp.net";
const ME: &str = "351910000001@s.whatsapp.net";

fn quick() -> Timing {
    Timing {
        retry_first: Duration::from_millis(5),
        retry_most: Duration::from_millis(20),
        unconfirmed_for: Duration::from_secs(3_600),
        read_every: Duration::from_millis(150),
        typing_again: Duration::from_millis(150),
        typing_quiet: Duration::from_millis(250),
        online_after: Duration::from_millis(30),
        away_after: Duration::from_millis(150),
    }
}

/// jawd's sending on a jaw.db in `home`, with the demo engine on `world`, connected.
struct Running {
    store: Store,
    sending: Sending,
    world: Arc<World>,
    pushes: broadcast::Sender<Push>,
    supervisor: Option<Supervisor>,
    attention: Arc<crate::attention::Attention>,
}

impl Running {
    async fn on(home: &Path, world: Arc<World>, engine: bool) -> Running {
        Running::timed(home, world, engine, true, quick()).await
    }

    /// With the demo engine running but never connected to WhatsApp, until the test says so, for
    /// an account linked in an earlier run, whose JIDs jaw.db keeps.
    async fn unconnected(home: &Path) -> Running {
        Running::timed(home, Arc::default(), true, false, quick()).await
    }

    async fn timed(home: &Path, world: Arc<World>, engine: bool, connect: bool, timing: Timing) -> Running {
        let store = Store::open(home).unwrap();
        if engine && !connect {
            store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
        }
        let live: crate::sync::Shared = Arc::new(std::sync::Mutex::new(crate::sync::starting(&store, engine)));
        let (pushes, _) = broadcast::channel(256);
        let (events, incoming) = mpsc::channel(1024);
        let supervisor = engine.then(|| {
            let script = if connect { vec![demo::connected()] } else { Vec::new() };
            let engine = DemoEngine::sharing(script, world.clone());
            Supervisor::spawn(Arc::new(engine), events, Backoff::default())
        });
        let mut connected = pushes.subscribe();
        let sync =
            crate::sync::run(incoming, store.clone(), live.clone(), pushes.clone(), supervisor.clone(), home.into());
        tokio::spawn(sync);
        let attention = Arc::new(crate::attention::Attention::default());
        let sending =
            Sending::start(store.clone(), supervisor.clone(), live, &pushes, attention.focus(), timing).await.unwrap();
        if engine && connect {
            let online = async {
                while let Ok(push) = connected.recv().await {
                    if push.method == "connection" && push.params["state"] == "online" {
                        return;
                    }
                }
            };
            tokio::time::timeout(Duration::from_secs(10), online).await.expect("the demo engine never connected");
        }
        Running { store, sending, world, pushes, supervisor, attention }
    }

    async fn new(home: &Path) -> Running {
        let world: Arc<World> = Arc::default();
        world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
        Running::on(home, world, true).await
    }

    async fn chats(&self, chats: &[&str]) {
        let changes = chats.iter().map(|id| Change::Chat(Chat { id: id.to_string(), ..Chat::default() })).collect();
        self.store.apply(Lane::Live, changes).await.unwrap();
    }

    /// `chat`'s members, you among them, as sync keeps them, and who you are.
    async fn members(&self, chat: &str, members: &[&str]) {
        let me = Change::Remember { key: "me".into(), value: ME.into() };
        let set = members.iter().chain([&ME]).map(|jid| (jid.to_string(), "member")).collect();
        let members = Change::Members(Members { chat: chat.into(), all: true, set, remove: Vec::new() });
        self.store.apply(Lane::Live, vec![me, members]).await.unwrap();
    }

    /// `from`'s phone saying it has, or has read, message `id`.
    async fn receipt(&self, chat: &str, id: &str, from: &str, status: &str) {
        let amend = Amend {
            chat: chat.into(),
            target: id.into(),
            sender: Some(from.into()),
            mine: false,
            at: now_ms(),
            what: Amendment::Status { status: status.into() },
            raw: None,
        };
        self.store.apply(Lane::Live, vec![Change::Amend(amend)]).await.unwrap();
    }

    async fn status(&self, chat: &str, id: &str) -> Option<String> {
        let (chat, id) = (chat.to_string(), id.to_string());
        self.query(move |db| Ok(read::message(db, &chat, &id)?.and_then(|(row, _)| row.status))).await
    }

    fn connection(&self, state: jaw_proto::ConnectionState) {
        let params = serde_json::to_value(jaw_proto::Connection::new(state)).unwrap();
        let _ = self.pushes.send(Push { method: "connection".into(), params });
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        self.sending.call(method, params).await
    }

    async fn send(&self, chat: &str, text: &str) -> String {
        let sent = self.call("send.text", json!({ "chat": chat, "text": text })).await.unwrap();
        sent["message"].as_str().unwrap().to_string()
    }

    async fn query<T: Send + 'static>(
        &self,
        query: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
    ) -> T {
        self.store.reads().query(Priority::Interactive, query).await.unwrap()
    }

    /// Waits, a commit at a time, until `done` holds of jaw.db.
    async fn until(&self, what: &str, done: impl Fn(&Connection) -> anyhow::Result<bool> + Clone + Send + 'static) {
        let mut committed = self.store.committed();
        let waited = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let done = done.clone();
                if self.query(move |db| done(db)).await {
                    return;
                }
                committed.changed().await.unwrap();
            }
        });
        waited.await.unwrap_or_else(|_| panic!("{what} never happened"));
    }

    /// Waits until message `id` has got at least as far as `status`, since receipts can take it
    /// further between two looks.
    async fn until_status(&self, chat: &str, id: &str, status: &'static str) {
        const ORDER: [&str; 6] = ["pending", "failed", "sent", "delivered", "read", "played"];
        let rank = |status: Option<&str>| status.and_then(|s| ORDER.iter().position(|o| *o == s));
        let (chat, id) = (chat.to_string(), id.to_string());
        self.until(&format!("{id} {status}"), move |db| {
            let now = read::message(db, &chat, &id)?.and_then(|(row, _)| row.status);
            Ok(rank(now.as_deref()) >= rank(Some(status)))
        })
        .await;
    }

    async fn outbox(&self) -> Vec<(String, String)> {
        self.query(|db| {
            Ok(db
                .prepare("SELECT m.id, o.state FROM outbox o JOIN messages m ON m.n = o.message ORDER BY o.n")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// Someone else's message arriving live in `chat`.
    async fn arrives(&self, chat: &str, sender: &str, id: &str, at: i64) {
        let message = Message {
            chat: chat.into(),
            id: id.into(),
            at,
            sender: Some(sender.into()),
            kind: "text",
            text: Some(format!("message {id}")),
            summary: format!("message {id}"),
            live: true,
            ..Message::default()
        };
        self.store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
    }

    fn marked_read(&self) -> Vec<pb::MarkRead> {
        self.world
            .asked()
            .into_iter()
            .filter_map(|kind| match kind {
                pb::command::Kind::MarkRead(mark) => Some(mark),
                _ => None,
            })
            .collect()
    }

    fn presences(&self) -> Vec<(String, i32)> {
        self.world
            .asked()
            .into_iter()
            .filter_map(|kind| match kind {
                pb::command::Kind::ChatPresence(p) => Some((p.chat, p.state)),
                _ => None,
            })
            .collect()
    }

    fn online(&self) -> Vec<bool> {
        self.world
            .asked()
            .into_iter()
            .filter_map(|kind| match kind {
                pb::command::Kind::Presence(p) => Some(p.available),
                _ => None,
            })
            .collect()
    }
}

const SCENE_HOME: &str = "JAW_SEND_TEST_HOME";
const SCENE: &str = "JAW_SEND_TEST_SCENE";

/// Runs `scene` of jawd's on `home` in a process of its own, this test binary again, and kills it
/// the moment it says it has got to where the scene stops, as a crash stops jawd: whatever it had
/// in flight goes with it, and its writer with it. Returns what it said there.
async fn crash(home: &Path, scene: &str) -> String {
    use std::io::{BufRead, BufReader};
    let (home, scene) = (home.to_path_buf(), scene.to_string());
    tokio::task::spawn_blocking(move || {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["send::tests::a_scene_of_jawd_to_be_killed", "--exact", "--ignored", "--nocapture"])
            .env(SCENE_HOME, &home)
            .env(SCENE, &scene)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as libc::pid_t;
        let watchdog = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(60));
            // SAFETY: kill only sends a signal; the scene is either done or stuck by now.
            unsafe { libc::kill(pid, libc::SIGKILL) };
        });
        let lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let said = lines.map_while(Result::ok).find_map(|line| line.strip_prefix("AT ").map(str::to_string));
        child.kill().unwrap();
        child.wait().unwrap();
        drop(watchdog);
        said.unwrap_or_else(|| panic!("the scene {scene} never got to where it stops"))
    })
    .await
    .unwrap()
}

/// Where a scene says it has got to, for `crash`.
fn at(what: &str) {
    use std::io::Write;
    println!("AT {what}");
    std::io::stdout().flush().unwrap();
}

/// The scenes `crash` runs, each in a process of its own; run on its own, it does nothing.
#[test]
#[ignore = "run by the crash tests, in a process of their own"]
fn a_scene_of_jawd_to_be_killed() {
    let (Ok(home), Ok(scene)) = (std::env::var(SCENE_HOME), std::env::var(SCENE)) else { return };
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    runtime.block_on(async move {
        let home = Path::new(&home);
        let world: Arc<World> = Arc::default();
        world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
        match scene.as_str() {
            "answered" | "held" => {
                world.hold(true);
                let running = Running::on(home, world, true).await;
                running.chats(&[ANA]).await;
                let id = running.send(ANA, "did it go?").await;
                if scene == "answered" {
                    let db = Connection::open(home.join("jaw.db")).unwrap();
                    let found: Option<(String, String)> = db
                        .query_row(
                            "SELECT m.status, o.state FROM messages m JOIN outbox o ON o.message = m.n WHERE m.id = ?1",
                            [&id],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()
                        .unwrap();
                    if found == Some(("pending".into(), "queued".into())) {
                        at(&id);
                    }
                } else {
                    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::Send(_)))).await;
                    at(&id);
                }
            }
            "sent" => {
                let running = Running::on(home, world, true).await;
                running.chats(&[ANA]).await;
                let id = running.send(ANA, "once").await;
                running.until_status(ANA, &id, "sent").await;
                at(&id);
            }
            "refused" => {
                world.refuse_next(1);
                let running = Running::on(home, world, true).await;
                running.chats(&[ANA]).await;
                let id = running.send(ANA, "nobody home").await;
                running.until_status(ANA, &id, "failed").await;
                at(&id);
            }
            "read offline" => {
                let running = Running::on(home, world, false).await;
                running.arrives(ANA, ANA, "m1", 1_000).await;
                running.call("read", json!({ "chat": ANA, "up_to": "m1" })).await.unwrap();
                at("m1");
            }
            other => panic!("there's no scene {other}"),
        }
    });
    std::thread::sleep(Duration::from_secs(3_600));
}

/// Waits until `done` holds of jaw.db, for what changes rows no window shows, which no event wakes.
async fn eventually(
    running: &Running,
    what: &str,
    done: impl Fn(&Connection) -> anyhow::Result<bool> + Clone + Send + 'static,
) {
    for _ in 0..1_500 {
        let done = done.clone();
        if running.query(move |db| done(db)).await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{what} never happened");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_shows_pending_at_once_then_goes_sent_delivered_and_read() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    running.world.gate(true);
    running.call("draft.set", json!({ "chat": ANA, "text": "on my w" })).await.unwrap();
    let sent = running.call("send.text", json!({ "chat": ANA, "text": "on my way" })).await.unwrap();
    let id = sent["message"].as_str().unwrap().to_string();
    assert!(
        id.starts_with("3EB0")
            && id.len() == 22
            && id[4..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()),
        "{id}"
    );
    let row = &sent["row"];
    assert_eq!(
        (row["id"].as_str(), row["status"].as_str(), row["mine"].as_bool()),
        (Some(id.as_str()), Some("pending"), Some(true))
    );
    assert_eq!(row["spans"], json!([{ "t": "text", "s": "on my way" }]));
    let chat = running.query(|db| Ok(read::chats(db, false, None, 10)?.remove(0))).await;
    assert_eq!(
        (chat.draft, chat.last.unwrap().status.as_deref()),
        (None, Some("pending")),
        "the draft went with what was sent"
    );
    running.world.gate(false);
    running.until_status(ANA, &id, "read").await;
    let sends = running.world.sends();
    assert_eq!(sends.len(), 1);
    assert_eq!((sends[0].chat.as_str(), sends[0].id.as_str()), (ANA, id.as_str()));
    let raw = wa_proto::wa_web_protobufs_e2e::Message::decode(sends[0].raw.as_slice()).unwrap();
    assert_eq!(raw.conversation.as_deref(), Some("on my way"));
    assert!(running.outbox().await.is_empty(), "a message the server has leaves the outbox");
    let receipts: (i64, bool, bool) = running
        .query(|db| {
            Ok(db.query_row(
                "SELECT count(*), max(delivered_at) IS NOT NULL, max(read_at) IS NOT NULL FROM receipts",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        })
        .await;
    assert_eq!(receipts, (1, true, true), "Ana's receipt, delivered and read");
}

#[tokio::test(flavor = "multi_thread")]
async fn in_a_group_through_the_demo_a_message_is_read_only_once_both_members_have_read_it() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(300) });
    let other = "351914444444@s.whatsapp.net";
    running.members(CLIMBING, &[RUI, other]).await;
    let id = running.send(CLIMBING, "Saturday at 9?").await;
    running.until_status(CLIMBING, &id, "delivered").await;
    let rui_read = move |db: &Connection| {
        Ok(db.query_row(
            "SELECT count(*) FROM receipts r JOIN person_jids j ON j.n = r.recipient WHERE j.jid = ?1 AND r.read_at IS NOT NULL",
            [RUI],
            |r| r.get::<_, i64>(0),
        )? == 1)
    };
    running.until("Rui reading it", rui_read).await;
    let status = running.status(CLIMBING, &id).await;
    assert!(matches!(status.as_deref(), Some("delivered" | "read")), "{status:?}");
    running.until_status(CLIMBING, &id, "read").await;
    let receipts: Vec<(String, bool, bool)> = running
        .query(|db| {
            Ok(db
                .prepare(
                    "SELECT j.jid, r.delivered_at IS NOT NULL, r.read_at IS NOT NULL FROM receipts r
                    JOIN person_jids j ON j.n = r.recipient ORDER BY j.jid",
                )?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await;
    assert_eq!(
        receipts,
        [(RUI.to_string(), true, true), (other.to_string(), true, true)],
        "each recipient's receipts are kept"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn in_a_group_a_message_is_delivered_once_every_member_has_it_and_read_once_every_member_has_read_it() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.world.replying(Replies { delivered: Duration::from_secs(3_600), read: Duration::from_secs(3_600) });
    let (ana, sofia, cat, cat_lid, dora) = (
        ANA,
        "351915555555@s.whatsapp.net",
        "351916666666@s.whatsapp.net",
        "100000000006@lid",
        "351917777777@s.whatsapp.net",
    );
    let cat_both = Person { jids: vec![cat.into(), cat_lid.into()], ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(cat_both)]).await.unwrap();
    running.members(CLIMBING, &[ana, sofia, cat]).await;
    let id = running.send(CLIMBING, "who's in?").await;
    running.until_status(CLIMBING, &id, "sent").await;
    let joined = Members { chat: CLIMBING.into(), all: false, set: vec![(dora.into(), "member")], remove: Vec::new() };
    running.store.apply(Lane::Live, vec![Change::Members(joined)]).await.unwrap();
    let steps = [
        (ana, "delivered", "sent"),
        (sofia, "delivered", "sent"),
        (cat_lid, "delivered", "delivered"),
        (ana, "read", "delivered"),
        (sofia, "read", "delivered"),
    ];
    for (from, receipt, then) in steps {
        running.receipt(CLIMBING, &id, from, receipt).await;
        assert_eq!(running.status(CLIMBING, &id).await.as_deref(), Some(then), "after {from} said {receipt}");
    }
    let left = Members { chat: CLIMBING.into(), all: false, set: Vec::new(), remove: vec![cat.into()] };
    running.store.apply(Lane::Live, vec![Change::Members(left)]).await.unwrap();
    // Cat left without reading it, and Dora, who joined after, never had it.
    running.until_status(CLIMBING, &id, "read").await;
    assert_eq!(
        running.status(CLIMBING, &id).await.as_deref(),
        Some("read"),
        "Cat left without reading it, and Dora, who joined after, never had it"
    );
    let kept = running.query(|db| Ok(db.query_row("SELECT count(*) FROM receipts", [], |r| r.get::<_, i64>(0))?)).await;
    assert_eq!(kept, 3, "each recipient's own receipt is kept for the message's info");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_message_and_its_outbox_row_are_committed_before_send_text_answers_and_survive_a_crash() {
    let home = tempfile::tempdir().unwrap();
    let id = crash(home.path(), "answered").await;
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    let state: String = db
        .query_row("SELECT o.state FROM outbox o JOIN messages m ON m.n = o.message WHERE m.id = ?1", [&id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(state, "queued", "the outbox row outlived jawd");
}

fn world() -> Arc<World> {
    let world: Arc<World> = Arc::default();
    world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
    world
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_mid_send_sends_the_same_id_again_and_the_message_arrives_once() {
    let home = tempfile::tempdir().unwrap();
    let id = crash(home.path(), "held").await;
    let running = Running::on(home.path(), world(), true).await;
    running.until_status(ANA, &id, "read").await;
    let sends = running.world.sends();
    assert_eq!(sends.len(), 1, "sent again once after the crash");
    assert_eq!(sends[0].id, id, "with the ID jawd made");
    assert_eq!(running.world.given(), 1, "Ana got it once");
    let count = running.query(|db| read::message_count(db, ANA)).await;
    assert_eq!(count, 1);
    assert!(running.outbox().await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_the_server_had_before_a_restart_is_not_sent_again() {
    let home = tempfile::tempdir().unwrap();
    let id = crash(home.path(), "sent").await;
    let running = Running::on(home.path(), world(), true).await;
    running.chats(&[RUI]).await;
    let later = running.send(RUI, "and another").await;
    running.until_status(RUI, &later, "sent").await;
    let ids: Vec<String> = running.world.sends().into_iter().map(|send| send.id).collect();
    assert_eq!(ids, [later], "{id} wasn't sent again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_resend_of_what_may_have_gone_waits_for_a_receipt_instead_of_failing() {
    let home = tempfile::tempdir().unwrap();
    let id = crash(home.path(), "held").await;
    let world = world();
    world.refuse_next(1);
    let running = Running::on(home.path(), world, true).await;
    let unconfirmed = |db: &Connection| {
        Ok(db.query_row("SELECT count(*) FROM outbox WHERE state = 'unconfirmed'", [], |r| r.get::<_, i64>(0))? == 1)
    };
    eventually(&running, "the resend's refusal", unconfirmed).await;
    assert_eq!(running.status(ANA, &id).await.as_deref(), Some("pending"), "not shown failed: it may have gone");
    running.call("send.retry", json!({ "message": id })).await.unwrap();
    assert_eq!(running.world.sends().len(), 1, "a retry by hand has nothing to send while it waits");
    running.receipt(ANA, &id, ANA, "delivered").await;
    assert_eq!(running.status(ANA, &id).await.as_deref(), Some("delivered"));
    assert!(running.outbox().await.is_empty(), "the receipt settles it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_resend_with_no_receipt_fails_once_its_time_has_passed_and_can_be_retried() {
    let home = tempfile::tempdir().unwrap();
    let id = crash(home.path(), "held").await;
    let world = world();
    world.refuse_next(1);
    let timing = Timing { unconfirmed_for: Duration::from_millis(400), ..quick() };
    let running = Running::timed(home.path(), world, true, true, timing).await;
    let unconfirmed = |db: &Connection| {
        Ok(db.query_row("SELECT count(*) FROM outbox WHERE state = 'unconfirmed'", [], |r| r.get::<_, i64>(0))? == 1)
    };
    eventually(&running, "the resend's refusal", unconfirmed).await;
    let began = std::time::Instant::now();
    running.until_status(ANA, &id, "failed").await;
    assert!(began.elapsed() < Duration::from_secs(10));
    assert_eq!(running.outbox().await, [(id.clone(), "failed".to_string())]);
    let error: String = running.query(|db| Ok(db.query_row("SELECT error FROM outbox", [], |r| r.get(0))?)).await;
    assert!(error.contains("no receipt came"), "{error}");
    running.call("send.retry", json!({ "message": id })).await.unwrap();
    running.until_status(ANA, &id, "read").await;
    assert!(running.outbox().await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_gives_a_message_waiting_for_a_receipt_only_the_time_it_had_left() {
    let home = tempfile::tempdir().unwrap();
    let id = crash(home.path(), "held").await;
    // As a refused resend leaves it, with 1.5 s of its wait left when jawd starts again.
    let until = now_ms() + 1_500;
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.execute("UPDATE outbox SET state = 'unconfirmed', attempts = 1, error = 'refused', next_at = ?1", [until])
        .unwrap();
    drop(db);
    let running = Running::on(home.path(), world(), true).await;
    assert_eq!(running.status(ANA, &id).await.as_deref(), Some("pending"), "still waiting");
    running.until_status(ANA, &id, "failed").await;
    let late = now_ms() - until;
    assert!(late >= 0, "it waited what it had left, failing {}ms early", -late);
    assert!(late < 10_000, "and not the hour a fresh wait has here: {late}ms late");
    assert!(running.world.sends().is_empty(), "what waits for a receipt isn't sent again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_send_that_fails_is_tried_again_with_the_same_id_until_it_goes() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    running.world.fail_next(3);
    let id = running.send(ANA, "third time lucky").await;
    running.until_status(ANA, &id, "sent").await;
    let sends = running.world.sends();
    assert_eq!(sends.len(), 4);
    assert!(sends.iter().all(|send| send.id == id));
}

#[tokio::test(flavor = "multi_thread")]
async fn chats_waiting_to_try_again_leave_their_places_to_chats_that_can_send() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let stuck: Vec<String> = (0..CHATS_AT_ONCE).map(|n| format!("3519300000{n:02}@s.whatsapp.net")).collect();
    running.chats(&stuck.iter().map(String::as_str).chain([ANA]).collect::<Vec<_>>()).await;
    running.world.fail_chats(&stuck);
    for chat in &stuck {
        running.send(chat, "this one won't go yet").await;
    }
    let tried_each = |asked: &[pb::command::Kind]| {
        asked.iter().filter(|k| matches!(k, pb::command::Kind::Send(_))).count() >= 2 * CHATS_AT_ONCE
    };
    running.world.until(tried_each).await;
    let id = running.send(ANA, "and this one goes").await;
    running.until_status(ANA, &id, "read").await;
    let stuck_tries = running.world.sends().iter().filter(|send| stuck.contains(&send.chat)).count();
    assert!(stuck_tries > CHATS_AT_ONCE, "the stuck chats keep trying meanwhile: {stuck_tries}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_whatsapp_refuses_is_failed_until_it_is_retried_by_hand_even_across_a_restart() {
    let home = tempfile::tempdir().unwrap();
    let id = crash(home.path(), "refused").await;
    let running = Running::on(home.path(), world(), true).await;
    assert_eq!(running.outbox().await, [(id.clone(), "failed".to_string())], "a restart doesn't send a failed message");
    let chat = running.query(|db| Ok(read::chats(db, false, None, 10)?.remove(0))).await;
    assert_eq!(chat.last.unwrap().status.as_deref(), Some("failed"), "the chat list shows it failed");
    let unknown = running.call("send.retry", json!({ "message": "3EB0000000000000000000" })).await.unwrap_err();
    assert_eq!(unknown.code, code::NOT_FOUND);
    running.call("send.retry", json!({ "message": id })).await.unwrap();
    running.until_status(ANA, &id, "read").await;
    assert_eq!(running.world.sends().len(), 1);
    assert!(running.outbox().await.is_empty());
    running.call("send.retry", json!({ "message": id })).await.unwrap_err();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_same_key_answers_with_the_same_message_and_sends_nothing_new() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    running.world.gate(true);
    let send = |key: &str| running.call("send.text", json!({ "chat": ANA, "text": "on my way", "key": key }));
    let first = send("compose-1").await.unwrap();
    let seq = *running.store.committed().borrow();
    let again = send("compose-1").await.unwrap();
    assert_eq!(*running.store.committed().borrow(), seq, "asking again commits nothing");
    assert_eq!(again, first, "the same message and row");
    let (one, two) = tokio::join!(send("compose-2"), send("compose-2"));
    assert_eq!(one.unwrap()["message"], two.unwrap()["message"], "asked twice at once, one message");
    running.world.gate(false);
    let id = first["message"].as_str().unwrap().to_string();
    running.until_status(ANA, &id, "read").await;
    let ids: Vec<String> = running.world.sends().into_iter().map(|send| send.id).collect();
    assert_eq!(ids.len(), 2, "one each for two keys: {ids:?}");
    assert_eq!(running.query(|db| read::message_count(db, ANA)).await, 2);
    let late = send("compose-1").await.unwrap();
    assert_eq!(late["message"], first["message"], "even once it's gone");
    assert_eq!(late["row"]["status"], "read");
    let bad = running.call("send.text", json!({ "chat": ANA, "text": "x", "key": "" })).await.unwrap_err();
    assert_eq!(bad.code, code::INVALID_PARAMS);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_waits_pending_for_as_long_as_whatsapp_cant_be_reached_and_goes_once_it_can() {
    use jaw_proto::ConnectionState::{Offline, Online};
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
    running.chats(&[ANA]).await;
    let id = running.send(ANA, "when you're back").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(running.world.sends().is_empty(), "nothing is tried while WhatsApp can't be reached");
    assert_eq!(running.status(ANA, &id).await.as_deref(), Some("pending"));
    assert_eq!(running.outbox().await, [(id.clone(), "queued".to_string())]);
    running.world.fail_next(2);
    running.connection(Online);
    running.until_status(ANA, &id, "read").await;
    assert_eq!(running.world.sends().len(), 3, "tried again while connected until it went");
    running.connection(Offline);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let later = running.send(ANA, "and this one").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(running.world.sends().len(), 3);
    running.connection(Online);
    running.until_status(ANA, &later, "sent").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn past_a_thousand_messages_waiting_a_new_one_is_refused_and_none_is_dropped() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.chats(&[ANA]).await;
    {
        let mut db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        let tx = db.transaction().unwrap();
        for n in 0..1_000i64 {
            tx.execute(
                "INSERT INTO messages (chat, id, at, t, kind, mine, status, v)
                VALUES ((SELECT n FROM chats WHERE id = ?3), ?1, ?2, ?2, 'text', 1, 'pending', 1)",
                rusqlite::params![format!("w{n}"), n, ANA],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO outbox (message, jid, id, kind, raw, state, next_at, created_at)
                VALUES (last_insert_rowid(), ?2, ?3, 'text', x'', ?1, 0, 0)",
                rusqlite::params![if n < 10 { "failed" } else { "queued" }, ANA, format!("w{n}")],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let refused = running.call("send.text", json!({ "chat": ANA, "text": "one too many" })).await.unwrap_err();
    assert_eq!(refused.code, code::LIMIT);
    assert_eq!(refused.data, Some(json!({ "limit": "outbox", "waiting": 1_000, "failed": 10 })));
    assert!(
        refused.message.contains("1000 messages of yours are waiting to send, 10 of them failed"),
        "{}",
        refused.message
    );
    assert_eq!(running.outbox().await.len(), 1_000);
}

#[tokio::test(flavor = "multi_thread")]
async fn each_chats_messages_go_in_order_with_sixteen_chats_sending_at_once() {
    let home = tempfile::tempdir().unwrap();
    let running = Arc::new(Running::new(home.path()).await);
    running.world.gate(true);
    let chats: Vec<String> = (0..24).map(|n| format!("3519200000{n:02}@s.whatsapp.net")).collect();
    running.chats(&chats.iter().map(String::as_str).collect::<Vec<_>>()).await;
    let mut writing = Vec::new();
    for chat in chats.clone() {
        let running = running.clone();
        writing.push(tokio::spawn(async move {
            let mut ids = Vec::new();
            for n in 0..6 {
                ids.push(running.send(&chat, &format!("{n}")).await);
            }
            (chat, ids)
        }));
    }
    let mut written = Vec::new();
    for writing in writing {
        written.push(writing.await.unwrap());
    }
    let sends = |asked: &[pb::command::Kind]| asked.iter().filter(|k| matches!(k, pb::command::Kind::Send(_))).count();
    running.world.until(|asked| sends(asked) == 16).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        (sends(&running.world.asked()), running.world.most_at_once()),
        (16, 16),
        "sixteen chats send at once while the server holds them, and no more"
    );
    running.world.gate(false);
    running.world.until(|asked| sends(asked) == 24 * 6).await;
    let sent = running.world.sends();
    for (chat, ids) in &written {
        let order: Vec<&str> = sent.iter().filter(|send| &send.chat == chat).map(|send| send.id.as_str()).collect();
        assert_eq!(order, ids.iter().map(String::as_str).collect::<Vec<_>>(), "{chat} sent out of order");
    }
    assert_eq!(running.world.most_at_once(), 16);
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_start_by_number_asks_whatsapp_and_makes_one_chat_for_the_number_and_its_lid() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let started = running.call("chat.start", json!({ "phone": "+351 912-345-678" })).await.unwrap();
    assert_eq!((started["chat"]["id"].as_str(), started["chat"]["kind"].as_str()), (Some(ANA), Some("direct")));
    let asked = running.world.asked();
    assert!(asked.iter().any(|k| matches!(k, pb::command::Kind::OnWhatsapp(o) if o.phones == ["351912345678"])));
    running.arrives("1000351912345678@lid", "1000351912345678@lid", "m1", 1_000).await;
    let again = running.call("chat.start", json!({ "phone": "351912345678" })).await.unwrap();
    assert_eq!(again["chat"]["id"].as_str(), Some(ANA));
    let chats = running.query(|db| Ok(db.query_row("SELECT count(*) FROM chats", [], |r| r.get::<_, i64>(0))?)).await;
    assert_eq!(chats, 1, "the LID's message went to the number's chat");
    let nobody = running.call("chat.start", json!({ "phone": "+999 123 4567" })).await.unwrap_err();
    assert_eq!((nobody.code, nobody.message.as_str()), (code::NOT_FOUND, "+9991234567 isn't on WhatsApp"));
    let garbled = running.call("chat.start", json!({ "phone": "call me" })).await.unwrap_err();
    assert_eq!(garbled.code, code::INVALID_PARAMS);
    let neither = running.call("chat.start", json!({})).await.unwrap_err();
    assert_eq!(neither.code, code::INVALID_PARAMS);
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_start_by_person_finds_their_chat_under_any_of_their_jids_or_makes_one() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let lid = "100000000001@lid";
    let ana = Person { jids: vec![ANA.into(), lid.into()], name: Some("Ana".into()), ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(ana)]).await.unwrap();
    running.arrives(lid, lid, "m1", 1_000).await;
    let person: i64 = running
        .query(move |db| Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [ANA], |r| r.get(0))?))
        .await;
    let started = running.call("chat.start", json!({ "person": format!("person:{person}") })).await.unwrap();
    assert_eq!((started["chat"]["id"].as_str(), started["chat"]["name"].as_str()), (Some(lid), Some("Ana")));
    let rui = Person { jids: vec![RUI.into()], name: Some("Rui".into()), ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(rui)]).await.unwrap();
    let person: i64 = running
        .query(move |db| Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [RUI], |r| r.get(0))?))
        .await;
    let made = running.call("chat.start", json!({ "person": format!("person:{person}") })).await.unwrap();
    assert_eq!((made["chat"]["id"].as_str(), made["chat"]["name"].as_str()), (Some(RUI), Some("Rui")));
    let again = running.call("chat.start", json!({ "person": format!("person:{person}") })).await.unwrap();
    assert_eq!(again["chat"]["id"].as_str(), Some(RUI));
    let chats = running.query(|db| Ok(db.query_row("SELECT count(*) FROM chats", [], |r| r.get::<_, i64>(0))?)).await;
    assert_eq!(chats, 2);
    let unknown = running.call("chat.start", json!({ "person": "person:9999" })).await.unwrap_err();
    assert_eq!(unknown.code, code::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_draft_is_kept_for_every_interface_shown_in_the_chat_list_and_cleared_when_empty() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    running.call("draft.set", json!({ "chat": ANA, "text": "see you" })).await.unwrap();
    let draft = || running.query(|db| Ok(read::chats(db, false, None, 10)?.remove(0).draft));
    assert_eq!(draft().await.as_deref(), Some("see you"));
    let before = running.query(|db| Ok(read::chats(db, false, None, 10)?.remove(0).v)).await;
    running.call("draft.set", json!({ "chat": ANA, "text": "see you" })).await.unwrap();
    let after = running.query(|db| Ok(read::chats(db, false, None, 10)?.remove(0).v)).await;
    assert_eq!(after, before, "the same draft again changes nothing");
    running.call("draft.set", json!({ "chat": ANA, "text": "" })).await.unwrap();
    assert_eq!(draft().await, None);
    let unknown = running.call("draft.set", json!({ "chat": RUI, "text": "hi" })).await.unwrap_err();
    assert_eq!(unknown.code, code::NOT_FOUND);
    let long = running.call("draft.set", json!({ "chat": ANA, "text": "a".repeat(TEXT_MOST + 1) })).await.unwrap_err();
    assert_eq!(long.code, code::LIMIT);
}

#[tokio::test(flavor = "multi_thread")]
async fn send_text_refuses_what_it_cant_send_yet_and_chats_it_doesnt_have() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    let refused = |params: Value| running.call("send.text", params);
    assert_eq!(refused(json!({ "chat": ANA, "text": "  \n" })).await.unwrap_err().code, code::INVALID_PARAMS);
    assert_eq!(
        refused(json!({ "chat": ANA, "text": "hi", "reply": "3EB0AA" })).await.unwrap_err().code,
        code::INVALID_PARAMS
    );
    assert_eq!(refused(json!({ "chat": RUI, "text": "hi" })).await.unwrap_err().code, code::NOT_FOUND);
    assert_eq!(refused(json!({ "chat": ANA, "text": "a".repeat(TEXT_MOST + 1) })).await.unwrap_err().code, code::LIMIT);
    assert_eq!(refused(json!({ "chat": ANA })).await.unwrap_err().code, code::INVALID_PARAMS);
    assert!(running.world.sends().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disappearing_chats_messages_carry_its_timer() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let chat = Chat { id: ANA.into(), ephemeral: Some(86_400), ..Chat::default() };
    running.store.apply(Lane::Live, vec![Change::Chat(chat)]).await.unwrap();
    running.world.gate(true);
    let id = running.send(ANA, "gone tomorrow").await;
    let (chat, message) = (ANA, id.clone());
    let pending = running.query(move |db| Ok(read::message(db, chat, &message)?.unwrap().0)).await;
    assert_eq!(pending.expires_at, None, "its timer waits for it to go");
    running.world.gate(false);
    running.until_status(ANA, &id, "sent").await;
    let raw = wa_proto::wa_web_protobufs_e2e::Message::decode(running.world.sends()[0].raw.as_slice()).unwrap();
    let text = raw.extended_text_message.unwrap();
    assert_eq!((text.text.as_deref(), text.context_info.unwrap().expiration), (Some("gone tomorrow"), Some(86_400)));
    let row = running.query(move |db| Ok(read::message(db, ANA, &id)?.unwrap().0)).await;
    assert!(row.expires_at.is_some_and(|at| at > row.at));
}

#[tokio::test(flavor = "multi_thread")]
async fn reading_moves_the_read_position_and_marks_read_once_a_spell_however_fast_it_is_read() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    for n in 0..10 {
        running.arrives(ANA, ANA, &format!("m{n}"), 1_000 + n).await;
    }
    let unread = || running.query(|db| Ok(read::chats(db, false, None, 10)?.remove(0).unread));
    assert_eq!(unread().await, 10);
    running.call("read", json!({ "chat": ANA, "up_to": "m3" })).await.unwrap();
    assert_eq!(unread().await, 6, "counted again from where it's read");
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::MarkRead(_)))).await;
    for upto in ["m5", "m7", "m9", "m2"] {
        running.call("read", json!({ "chat": ANA, "up_to": upto })).await.unwrap();
    }
    assert_eq!(unread().await, 0, "and it never goes back");
    running
        .world
        .until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::MarkRead(_))).count() >= 2)
        .await;
    tokio::time::sleep(quick().read_every * 2).await;
    let marks = running.marked_read();
    assert_eq!(marks.len(), 2, "one at once, then one for the rest of the spell: {marks:?}");
    assert_eq!(marks[0].ids, ["m0", "m1", "m2", "m3"]);
    assert_eq!(marks[1].ids, ["m4", "m5", "m6", "m7", "m8", "m9"]);
    assert!(marks.iter().all(|mark| mark.chat == ANA && mark.sender.is_empty()));
    let missing = running.call("read", json!({ "chat": ANA, "up_to": "nope" })).await.unwrap_err();
    assert_eq!(missing.code, code::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_sent_here_reads_the_chat_and_owes_its_receipts_but_one_from_your_phone_doesnt() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.world.gate(true);
    running.arrives(ANA, ANA, "m1", now_ms() - 10_000).await;
    running.send(ANA, "on it").await;
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::MarkRead(_)))).await;
    assert_eq!(running.marked_read()[0].ids, ["m1"], "replying here reads what came before, as on the phone");
    let chat = running.query(|db| Ok(read::chats(db, false, None, 10)?.remove(0))).await;
    assert_eq!(chat.unread, 0);
    running.arrives(ANA, ANA, "m2", now_ms() - 5_000).await;
    let from_phone = Message {
        chat: ANA.into(),
        id: "3EB0FROMPHONE".into(),
        at: now_ms() - 1_000,
        mine: true,
        kind: "text",
        status: Some("sent"),
        live: true,
        ..Message::default()
    };
    running.store.apply(Lane::Live, vec![Change::Message(from_phone)]).await.unwrap();
    tokio::time::sleep(quick().read_every * 3).await;
    assert_eq!(running.marked_read().len(), 1, "the phone that sent it told WhatsApp itself");
    let owing = running
        .query(|db| {
            Ok(db.query_row("SELECT count(*) FROM chats WHERE told_at IS NOT NULL", [], |r| r.get::<_, i64>(0))?)
        })
        .await;
    assert_eq!(owing, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn each_sending_call_that_keeps_something_is_one_synced_commit() {
    let home = tempfile::tempdir().unwrap();
    // No engine, so nothing else commits synced meanwhile: the inbox would.
    let running = Running::on(home.path(), Arc::default(), false).await;
    running.chats(&[ANA]).await;
    running.arrives(ANA, ANA, "m1", 1_000).await;
    running
        .store
        .apply(Lane::Live, vec![Change::Person(Person { jids: vec![RUI.into()], ..Person::default() })])
        .await
        .unwrap();
    let rui: i64 = running
        .query(|db| Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [RUI], |r| r.get(0))?))
        .await;
    let synced = || running.store.storage().synced_commits;
    let calls = [
        ("send.text", json!({ "chat": ANA, "text": "one" })),
        ("draft.set", json!({ "chat": ANA, "text": "two" })),
        ("read", json!({ "chat": ANA, "up_to": "m1" })),
        ("chat.start", json!({ "person": format!("person:{rui}") })),
    ];
    for (method, params) in calls {
        let before = synced();
        running.call(method, params).await.unwrap();
        assert_eq!(synced() - before, 1, "{method}");
    }
    let id = running.query(|db| Ok(db.query_row("SELECT o.id FROM outbox o", [], |r| r.get::<_, String>(0))?)).await;
    running
        .store
        .apply(Lane::Live, vec![Change::Outbox(Step::Failed { outbox: 1, error: "no".into(), next_at: None })])
        .await
        .unwrap();
    let before = synced();
    running.call("send.retry", json!({ "message": id })).await.unwrap();
    assert_eq!(synced() - before, 1, "send.retry");
    let before = synced();
    running.call("typing", json!({ "chat": ANA, "state": "composing" })).await.unwrap();
    assert_eq!(synced() - before, 0, "typing keeps nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_read_here_first_owes_only_what_it_had_unread() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    for n in 0..5 {
        running.arrives(ANA, ANA, &format!("m{n}"), 1_000 + n).await;
    }
    let read_on_phone =
        crate::store::Read { chat: ANA.into(), at: 1_002, ids: vec!["m2".into()], ..Default::default() };
    running.store.apply(Lane::Live, vec![Change::Read(read_on_phone)]).await.unwrap();
    running.call("read", json!({ "chat": ANA, "up_to": "m4" })).await.unwrap();
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::MarkRead(_)))).await;
    assert_eq!(running.marked_read()[0].ids, ["m3", "m4"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn read_receipts_in_a_group_name_each_sender() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let sofia = "351915555555@s.whatsapp.net";
    for (n, sender) in [RUI, sofia, RUI].into_iter().enumerate() {
        running.arrives(CLIMBING, sender, &format!("g{n}"), 1_000 + n as i64).await;
    }
    running.call("read", json!({ "chat": CLIMBING, "up_to": "g2" })).await.unwrap();
    running
        .world
        .until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::MarkRead(_))).count() >= 2)
        .await;
    let mut marks: Vec<(String, Vec<String>)> = running.marked_read().into_iter().map(|m| (m.sender, m.ids)).collect();
    marks.sort();
    assert_eq!(
        marks,
        [(RUI.to_string(), vec!["g0".to_string(), "g2".to_string()]), (sofia.to_string(), vec!["g1".to_string()])]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn read_receipts_owed_when_jawd_stops_are_sent_when_it_starts_again() {
    let home = tempfile::tempdir().unwrap();
    crash(home.path(), "read offline").await;
    let running = Running::on(home.path(), world(), true).await;
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::MarkRead(_)))).await;
    assert_eq!(running.marked_read()[0].ids, ["m1"]);
    tokio::time::sleep(quick().read_every * 2).await;
    assert_eq!(running.marked_read().len(), 1, "once told, it's not owed any more");
}

#[tokio::test(flavor = "multi_thread")]
async fn more_than_a_receipts_worth_owed_goes_in_receipts_of_a_thousand_oldest_first_and_none_is_skipped() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.arrives(ANA, ANA, "m0", 1_000).await;
    running.call("read", json!({ "chat": ANA, "up_to": "m0" })).await.unwrap();
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::MarkRead(_)))).await;
    let messages: Vec<Change> = (1..1_500)
        .map(|n| {
            Change::Message(Message {
                chat: ANA.into(),
                id: format!("m{n}"),
                at: 1_000 + n,
                sender: Some(ANA.into()),
                kind: "text",
                text: Some("hi".into()),
                summary: "hi".into(),
                live: true,
                ..Message::default()
            })
        })
        .collect();
    running.store.apply(Lane::Live, messages).await.unwrap();
    running.call("read", json!({ "chat": ANA, "up_to": "m1499" })).await.unwrap();
    running
        .world
        .until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::MarkRead(_))).count() >= 3)
        .await;
    let marks = running.marked_read();
    let sizes: Vec<usize> = marks.iter().map(|mark| mark.ids.len()).collect();
    assert_eq!(sizes, [1, 1_000, 499]);
    let ids: Vec<String> = marks.iter().flat_map(|mark| mark.ids.clone()).collect();
    assert_eq!(ids, (0..1_500).map(|n| format!("m{n}")).collect::<Vec<_>>(), "each once, oldest first");
}

#[tokio::test(flavor = "multi_thread")]
async fn typing_is_said_when_it_starts_again_only_now_and_then_and_stopped_after_a_quiet_spell() {
    use pb::chat_presence::State;
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let typing = |state: &str| running.call("typing", json!({ "chat": ANA, "state": state }));
    for _ in 0..5 {
        typing("composing").await.unwrap();
    }
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::ChatPresence(_)))).await;
    assert_eq!(running.presences(), [(ANA.to_string(), State::Composing as i32)]);
    tokio::time::sleep(quick().typing_again).await;
    typing("composing").await.unwrap();
    typing("recording").await.unwrap();
    typing("paused").await.unwrap();
    typing("paused").await.unwrap();
    running
        .world
        .until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::ChatPresence(_))).count() >= 4)
        .await;
    let said: Vec<i32> = running.presences().into_iter().map(|(_, state)| state).collect();
    assert_eq!(said, [State::Composing, State::Composing, State::Recording, State::Paused].map(|s| s as i32));
    typing("composing").await.unwrap();
    running
        .world
        .until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::ChatPresence(_))).count() >= 6)
        .await;
    let said: Vec<i32> = running.presences().into_iter().map(|(_, state)| state).skip(4).collect();
    assert_eq!(said, [State::Composing, State::Paused].map(|s| s as i32), "stopped once nobody said it again");
    assert_eq!(typing("nonsense").await.unwrap_err().code, code::INVALID_PARAMS);
}

#[tokio::test(flavor = "multi_thread")]
async fn you_are_online_only_while_an_interface_has_focus_and_away_once_none_has_for_a_while() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::Presence(_)))).await;
    assert_eq!(running.online(), [false], "away from the start, with nothing in front");
    let terminal = running.attention.viewer();
    terminal.focus(true);
    let other = running.attention.viewer();
    running
        .world
        .until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::Presence(_))).count() >= 2)
        .await;
    assert_eq!(running.online(), [false, true]);
    terminal.focus(false);
    tokio::time::sleep(quick().away_after / 3).await;
    other.focus(true);
    tokio::time::sleep(quick().away_after * 2).await;
    assert_eq!(running.online(), [false, true], "switching windows doesn't flicker");
    drop(other);
    running
        .world
        .until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::Presence(_))).count() >= 3)
        .await;
    assert_eq!(running.online(), [false, true, false]);
    terminal.focus(true);
    running
        .world
        .until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::Presence(_))).count() >= 4)
        .await;
    let offline = jaw_proto::Connection::new(jaw_proto::ConnectionState::Offline);
    let online = jaw_proto::Connection::new(jaw_proto::ConnectionState::Online);
    for connection in [offline, online] {
        let _ = running
            .pushes
            .send(Push { method: "connection".into(), params: serde_json::to_value(connection).unwrap() });
    }
    running
        .world
        .until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::Presence(_))).count() >= 5)
        .await;
    assert_eq!(running.online(), [false, true, false, true, true], "told again when the connection comes back");
}

#[tokio::test(flavor = "multi_thread")]
async fn sending_goes_through_the_socket_with_the_pending_row_in_its_answer() {
    use std::io::{BufRead, BufReader, Write};

    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    let socket = jaw_proto::local::socket(home.path());
    let listener = crate::sock::listen(&socket).unwrap();
    let state = crate::state::State::new(
        home.path(),
        Ok(running.supervisor.clone().unwrap()),
        Vec::new(),
        Arc::default(),
        Some(running.store.clone()),
        running.pushes.clone(),
    )
    .unwrap()
    .with_attention(running.attention.clone())
    .with_sending(running.sending.clone());
    tokio::spawn(crate::sock::serve(Arc::new(state), listener));
    let replies = tokio::task::spawn_blocking(move || {
        let mut stream = std::os::unix::net::UnixStream::connect(&socket).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        [
            json!({ "id": 1, "method": "hello", "params": { "protocol": 1, "client": "test", "version": "0", "focus": true } }),
            json!({ "id": 2, "method": "send.text", "params": { "chat": ANA, "text": "olá" } }),
            json!({ "id": 3, "method": "typing", "params": { "chat": ANA, "state": "composing" } }),
            json!({ "id": 4, "method": "chat.start", "params": { "phone": "+351 913 333 333" } }),
        ]
        .into_iter()
        .map(|line| {
            stream.write_all(format!("{line}\n").as_bytes()).unwrap();
            loop {
                let mut reply = String::new();
                reader.read_line(&mut reply).unwrap();
                let reply: Value = serde_json::from_str(&reply).unwrap();
                if reply["id"] == line["id"] {
                    return reply;
                }
            }
        })
        .collect::<Vec<_>>()
    })
    .await
    .unwrap();
    assert_eq!(replies[1]["result"]["row"]["status"], "pending", "{}", replies[1]);
    assert_eq!(replies[2]["result"], json!({}));
    assert_eq!(replies[3]["result"]["chat"]["id"], RUI);
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::Presence(p) if p.available))).await;
}

#[test]
fn the_statements_sending_runs_itself_read_an_index_and_sort_nothing() {
    let home = tempfile::tempdir().unwrap();
    drop(Store::open(home.path()).unwrap());
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    let statements = [
        (
            start::DIRECT_CHAT_BY_JID,
            "SEARCH j USING INDEX sqlite_autoindex_person_jids_1 (jid=?)\nSEARCH c USING INDEX chats_by_person (person=?)",
        ),
        (start::DIRECT_CHAT_BY_PERSON, "SEARCH chats USING INDEX chats_by_person (person=?)"),
        (start::PERSON_JIDS, "SEARCH person_jids USING INDEX person_jids_by_person (person=?)"),
        (outbox::CHAT_EPHEMERAL, "SEARCH chats USING INDEX sqlite_autoindex_chats_1 (id=?)"),
    ];
    for (sql, expected) in statements {
        let mut explain = db.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        let plan: Vec<String> = explain
            .query_map(rusqlite::params_from_iter(std::iter::repeat_n(0, explain.parameter_count())), |r| r.get(3))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(plan.join("\n"), expected, "{sql}");
    }
}
