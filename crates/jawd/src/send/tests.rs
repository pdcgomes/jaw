use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use jaw_proto::{Push, code};
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc, watch};
use wa_proto::wa_web_protobuf_sync_action::{ContactAction, PinAction, SyncActionValue};

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
    live: crate::sync::Shared,
    /// Who's typing, as sync takes it from the engine and WhatsApp is heard by `sending`.
    typing: crate::feed::Typing,
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
        let script = if connect { vec![demo::connected()] } else { Vec::new() };
        let demo: Option<Arc<dyn crate::engine::Engine>> =
            engine.then(|| Arc::new(DemoEngine::sharing(script, world.clone())) as Arc<dyn crate::engine::Engine>);
        Running::running(home, world, demo, connect, timing).await
    }

    /// With `engine` running on `world`, connected once its script says so if `connect`.
    async fn running(
        home: &Path,
        world: Arc<World>,
        engine: Option<Arc<dyn crate::engine::Engine>>,
        connect: bool,
        timing: Timing,
    ) -> Running {
        let store = Store::open(home).unwrap();
        if engine.is_some() && !connect {
            store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
        }
        let live: crate::sync::Shared =
            Arc::new(std::sync::Mutex::new(crate::sync::starting(&store, engine.is_some())));
        let (pushes, _) = broadcast::channel(256);
        let (events, incoming) = mpsc::channel(1024);
        let connect = connect && engine.is_some();
        let supervisor = engine.map(|engine| Supervisor::spawn(engine, events, Backoff::default()));
        let mut connected = pushes.subscribe();
        let typing = crate::feed::Typing::default();
        let app_state = crate::sync::AppStateTurn::default();
        let sync = crate::sync::run(
            incoming,
            store.clone(),
            live.clone(),
            pushes.clone(),
            supervisor.clone(),
            home.into(),
            Default::default(),
            typing.clone(),
            app_state.clone(),
        );
        tokio::spawn(sync);
        let attention = Arc::new(crate::attention::Attention::default());
        let sending = Sending::start(
            store.clone(),
            supervisor.clone(),
            live.clone(),
            &pushes,
            attention.focus(),
            timing,
            Makers { previews: link::Previews::off(), media: None },
            app_state,
        )
        .await
        .unwrap();
        let me = live.lock().expect("live state").me();
        typing.start(store.clone(), me, sending.hearing());
        if connect {
            let online = async {
                while let Ok(push) = connected.recv().await {
                    if push.method == "connection" && push.params["state"] == "online" {
                        return;
                    }
                }
            };
            tokio::time::timeout(Duration::from_secs(10), online).await.expect("the demo engine never connected");
        }
        Running { store, sending, world, pushes, supervisor, attention, live, typing }
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
            "edit held" => {
                let running = Running::on(home, world, true).await;
                running.chats(&[ANA]).await;
                let id = running.send(ANA, "chego às 20h").await;
                running.until_status(ANA, &id, "sent").await;
                running.world.hold(true);
                running.call("edit", json!({ "message": id, "text": "chego às 21h" })).await.unwrap();
                let sends = |asked: &[pb::command::Kind]| {
                    asked.iter().filter(|k| matches!(k, pb::command::Kind::Send(_))).count()
                };
                running.world.until(|asked| sends(asked) == 2).await;
                at(&id);
            }
            "deletion behind" => {
                world.hold(true);
                let running = Running::on(home, world, true).await;
                running
                    .store
                    .apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }])
                    .await
                    .unwrap();
                running.chats(&[ANA]).await;
                let id = running.send(ANA, "hi").await;
                running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::Send(_)))).await;
                running.call("delete", json!({ "message": id, "for": "everyone" })).await.unwrap();
                at(&id);
            }
            "pinned behind a message" => {
                let running = Running::on(home, phone_with_app_state(Ahead::Names), true).await;
                until_fetched(&running, 1).await;
                // Ana has no picture, as the avatars looked for once the engine connects find, so
                // jawd started again looks for none.
                let pictureless = Chat {
                    id: ANA.into(),
                    avatar: Some(String::new()),
                    avatar_id: Some("none".into()),
                    ..Chat::default()
                };
                running.store.apply(Lane::Live, vec![Change::Chat(pictureless)]).await.unwrap();
                until_offline(&running).await;
                running.send(ANA, "on my way").await;
                // Ana's newer message keeps the one sent from being the chat's newest, whose status
                // its line shows, so its going later leaves the chat's row as it is.
                running.arrives(ANA, ANA, "m2", now_ms() + 1_000).await;
                running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap();
                at("pinned");
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
async fn an_action_on_its_way_when_jawd_stops_goes_again_with_its_own_id_once_it_starts() {
    let home = tempfile::tempdir().unwrap();
    let id = crash(home.path(), "edit held").await;
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    let (edit, text): (String, String) = db
        .query_row(
            "SELECT o.id, m.text FROM outbox o JOIN messages m ON m.n = o.target WHERE o.kind = 'edit'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(text, "chego às 20h", "the edit outlived jawd, not yet done to its message");
    drop(db);
    let running = Running::on(home.path(), world(), true).await;
    until_stored(&running).await;
    let sends = running.world.sends();
    assert_eq!(sends.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), [edit.as_str()], "only the edit, with its ID");
    assert_eq!(stored_text(&running, &id).await, "chego às 21h");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_action_refused_once_it_may_have_gone_and_no_receipt_comes_leaves_its_message_and_says_so_till_one_comes() {
    let home = tempfile::tempdir().unwrap();
    let id = crash(home.path(), "edit held").await;
    let world = world();
    world.refuse_next(1);
    let timing = Timing { unconfirmed_for: Duration::from_secs(3), ..quick() };
    let running = Running::timed(home.path(), world, true, true, timing).await;
    let mut pushes = running.pushes.subscribe();
    let unconfirmed = |db: &Connection| {
        Ok(db.query_row("SELECT count(*) FROM outbox WHERE kind = 'edit' AND state = 'unconfirmed'", [], |r| {
            r.get::<_, i64>(0)
        })? == 1)
    };
    eventually(&running, "the resent edit's refusal", unconfirmed).await;
    assert_eq!(text_of(&row(&running, ANA, &id).await.unwrap()), "chego às 21h", "shown while it may have gone");
    let notice = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let push = pushes.recv().await.unwrap();
            if push.method == "notice" {
                return push.params["text"].as_str().unwrap().to_string();
            }
        }
    })
    .await
    .expect("no receipt in its time is said");
    assert!(notice.starts_with("WhatsApp refused your edit"), "{notice}");
    let kept = |db: &Connection| {
        Ok(db.query_row("SELECT count(*) FROM outbox WHERE kind = 'edit' AND state = 'failed'", [], |r| {
            r.get::<_, i64>(0)
        })? == 1)
    };
    eventually(&running, "the edit kept for a late receipt", kept).await;
    assert_eq!(stored_text(&running, &id).await, "chego às 20h");
    let back = row(&running, ANA, &id).await.unwrap();
    assert_eq!(text_of(&back), "chego às 20h", "and the row is as it was");
    let edit: String =
        running.query(|db| Ok(db.query_row("SELECT id FROM outbox WHERE kind = 'edit'", [], |r| r.get(0))?)).await;
    let retried = running.call("send.retry", json!({ "message": edit })).await.unwrap_err();
    assert_eq!(retried.code, code::NOT_FOUND, "an action's own ID names no message to send again");
    // Ana's phone was off, and says now that it had the edit after all.
    running.receipt(ANA, &edit, ANA, "delivered").await;
    let done = row(&running, ANA, &id).await.unwrap();
    assert_eq!(
        (text_of(&done), stored_text(&running, &id).await),
        ("chego às 21h".to_string(), "chego às 21h".to_string()),
        "a late receipt does it"
    );
    assert!(done.v > back.v, "and every window is sent it");
    assert_eq!(running.store.count("outbox").unwrap(), 0);
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

/// The `connection` pushes' states until one says `state`, which must come within 30 seconds.
async fn until_said(told: &mut broadcast::Receiver<Push>, state: &str) -> Vec<String> {
    let mut said = Vec::new();
    let waited = tokio::time::timeout(Duration::from_secs(30), async {
        while let Ok(push) = told.recv().await {
            if push.method == "connection" {
                let now = push.params["state"].as_str().unwrap_or_default().to_string();
                said.push(now.clone());
                if now == state {
                    return;
                }
            }
        }
    });
    waited.await.unwrap_or_else(|_| panic!("jawd never said {state}"));
    said
}

/// What the outbox holds: each message's ID, its state and its attempts.
async fn attempts(running: &Running) -> Vec<(String, String, i64)> {
    running
        .query(|db| {
            Ok(db
                .prepare(
                    "SELECT m.id, o.state, o.attempts FROM outbox o JOIN messages m ON m.n = o.message ORDER BY o.n",
                )?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_sent_while_the_engine_restarts_waits_with_no_attempt_counted_and_goes_once_it_is_back() {
    use crate::engine::failing::{Failing, Failure};
    let home = tempfile::tempdir().unwrap();
    let world: Arc<World> = Arc::default();
    world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
    let failing = Arc::new(Failing::new(DemoEngine::sharing(vec![demo::connected()], world.clone()), Failure::Crash));
    let engine: Arc<dyn crate::engine::Engine> = failing.clone();
    let running = Running::running(home.path(), world, Some(engine), true, quick()).await;
    running.chats(&[ANA]).await;
    let mut told = running.pushes.subscribe();
    failing.turn(true).await;
    until_said(&mut told, "engine_restarting").await;
    let id = running.send(ANA, "while it starts again").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(running.world.sends().is_empty(), "nothing is tried with no engine to take it");
    assert_eq!(running.status(ANA, &id).await.as_deref(), Some("pending"));
    assert_eq!(
        attempts(&running).await,
        [(id.clone(), "queued".to_string(), 0)],
        "it waits, and nothing counts as tried"
    );
    failing.turn(false).await;
    running.until_status(ANA, &id, "read").await;
    assert_eq!(running.world.sends().len(), 1, "once the next engine is connected it goes, once");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_sent_after_a_keepalive_timeout_waits_uncounted_and_goes_once_when_the_pings_get_through_again() {
    use crate::engine::failing::{Failing, Failure};
    let home = tempfile::tempdir().unwrap();
    let world: Arc<World> = Arc::default();
    world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
    let failing =
        Arc::new(Failing::new(DemoEngine::sharing(vec![demo::connected()], world.clone()), Failure::Keepalive));
    let engine: Arc<dyn crate::engine::Engine> = failing.clone();
    let running = Running::running(home.path(), world, Some(engine), true, quick()).await;
    running.chats(&[ANA]).await;
    let mut told = running.pushes.subscribe();
    failing.turn(true).await;
    until_said(&mut told, "connecting").await;
    let id = running.send(ANA, "while the pings fail").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(running.world.sends().is_empty(), "nothing is tried while jawd says it's connecting");
    assert_eq!(running.status(ANA, &id).await.as_deref(), Some("pending"));
    assert_eq!(
        attempts(&running).await,
        [(id.clone(), "queued".to_string(), 0)],
        "it waits, and nothing counts as tried"
    );
    failing.turn(false).await;
    until_said(&mut told, "online").await;
    running.until_status(ANA, &id, "read").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(running.world.sends().len(), 1, "once the pings get through it goes, once");
    // Waiting counted no try, so to WhatsApp it's any message's first: refused, it fails at once,
    // where one counted as tried may have gone, and would wait a day for a receipt.
    failing.turn(true).await;
    until_said(&mut told, "connecting").await;
    let refused = running.send(ANA, "one WhatsApp refuses").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(attempts(&running).await, [(refused.clone(), "queued".to_string(), 0)]);
    running.world.refuse_next(1);
    failing.turn(false).await;
    running.until_status(ANA, &refused, "failed").await;
    assert_eq!(attempts(&running).await, [(refused, "failed".to_string(), 1)], "its one try counted");
    assert_eq!(running.world.sends().len(), 2);
}

/// Waits until `seen` says `chat` has so many typing, for 15 s at most.
async fn typists(seen: &crate::feed::typing::Watching, chat: &str, n: usize, what: &str) {
    let end = std::time::Instant::now() + Duration::from_secs(15);
    while seen.now(chat).map_or(0, |who| who.len()) != n {
        assert!(std::time::Instant::now() < end, "{what} never came");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Typing from WhatsApp, as the engine hands it to sync's loop, ends at a keepalive timeout's
/// connecting, isn't taken while the pings fail though it still arrives, and once they get through
/// again WhatsApp is told you're online once more, once, after which it's heard again.
#[tokio::test(flavor = "multi_thread")]
async fn typing_ends_at_a_keepalive_timeout_and_comes_back_once_youre_told_online_again_after_the_restore() {
    use crate::engine::failing::{Failing, Failure};
    use crate::engine::pb::chat_presence::State::{Composing, Recording};
    let home = tempfile::tempdir().unwrap();
    let world: Arc<World> = Arc::default();
    let failing =
        Arc::new(Failing::new(DemoEngine::sharing(vec![demo::connected()], world.clone()), Failure::Keepalive));
    let engine: Arc<dyn crate::engine::Engine> = failing.clone();
    let running = Running::running(home.path(), world.clone(), Some(engine), true, quick()).await;
    let ana = Person { jids: vec![ANA.into()], name: Some("Ana".into()), ..Person::default() };
    running
        .store
        .apply(Lane::Live, vec![Change::Person(ana), Change::Chat(Chat { id: ANA.into(), ..Chat::default() })])
        .await
        .unwrap();
    let (seen, mut hearing) = (running.typing.watching(), running.sending.hearing());
    async fn heard(hearing: &mut watch::Receiver<bool>, now: bool) {
        let waited = tokio::time::timeout(Duration::from_secs(15), hearing.wait_for(|heard| *heard == now)).await;
        assert!(waited.is_ok_and(|heard| heard.is_ok()), "never heard {now}");
    }
    let terminal = running.attention.viewer();
    terminal.focus(true);
    heard(&mut hearing, true).await;
    world.typing(ANA, ANA, Composing).await;
    typists(&seen, ANA, 1, "her typing, through the engine's link").await;

    let mut told = running.pushes.subscribe();
    failing.turn(true).await;
    until_said(&mut told, "connecting").await;
    typists(&seen, ANA, 0, "her typing's end at the timeout").await;
    // What she says while the pings fail still arrives, and isn't taken.
    world.typing(ANA, ANA, Recording).await;
    let end = std::time::Instant::now() + Duration::from_secs(15);
    while running.typing.stats().heard < 2 || running.typing.stats().waiting > 0 {
        assert!(std::time::Instant::now() < end, "what she said meanwhile never came: {:?}", running.typing.stats());
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(seen.now(ANA), None, "nothing said during the timeout shows");

    let before = running.online().len();
    failing.turn(false).await;
    until_said(&mut told, "online").await;
    heard(&mut hearing, true).await;
    world.typing(ANA, ANA, Composing).await;
    typists(&seen, ANA, 1, "her typing once you're told online again").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(running.online()[before..], [true], "told you're online once after the restore");
    drop(terminal);
}

/// Notifications, as a test sees them.
struct Notified(mpsc::UnboundedSender<crate::notify::Notice>);

impl crate::notify::Notifier for Notified {
    fn name(&self) -> &str {
        "to a test"
    }

    fn notify(&self, notice: &crate::notify::Notice) -> anyhow::Result<()> {
        let _ = self.0.send(notice.clone());
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn linked_again_after_a_logout_that_cut_a_catch_up_short_jawd_is_online_and_sends_and_notifications_go() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
    running.chats(&[ANA]).await;
    let (to, mut notified) = mpsc::unbounded_channel();
    crate::notify::start(
        running.store.clone(),
        running.attention.clone(),
        crate::feed::catching_up(running.live.clone()),
        running.pushes.subscribe(),
        Arc::new(Notified(to)),
        crate::notify::Timing {
            quiet: Duration::from_millis(20),
            most: Duration::from_millis(100),
            hold: Duration::from_millis(300),
        },
    );
    let mut told = running.pushes.subscribe();
    let connection = |state: pb::connection::State, reason: &str| {
        let mut event = demo::connected();
        if let Some(pb::event::Kind::Connection(connection)) = &mut event.kind {
            connection.state = state as i32;
            connection.reason = reason.into();
        }
        event
    };
    let preview = pb::OfflinePreview { total: 10, messages: 10, ..Default::default() };
    let linked = pb::Linked { jid: ME.into(), push_name: "Pedro".into(), ..Default::default() };
    for event in [
        demo::connected(),
        pb::Event { seq: 0, kind: Some(pb::event::Kind::OfflinePreview(preview)) },
        connection(pb::connection::State::LoggedOut, "logged out from another device"),
        pb::Event { seq: 0, kind: Some(pb::event::Kind::Link(pb::Link { step: Some(pb::link::Step::Done(linked)) })) },
        demo::connected(),
    ] {
        running.world.deliver(event).await;
    }
    let said = until_said(&mut told, "logged_out").await;
    assert_eq!(said, ["online", "catching_up", "logged_out"]);
    assert_eq!(until_said(&mut told, "online").await, ["online"], "linked again, it's online");
    assert_eq!(running.live.lock().unwrap().account.state, jaw_proto::AccountState::Linked);
    assert!(!crate::feed::catching_up(running.live.clone())(), "and no catch-up holds the chat list");
    let id = running.send(ANA, "linked again").await;
    running.until_status(ANA, &id, "read").await;
    // WhatsApp times a message to the second, so Ana's is a second on, after yours.
    running.world.deliver(demo::text(ANA, ANA, "3EB0WELCOME", now_ms() + 1_000, "welcome back")).await;
    let notice = tokio::time::timeout(Duration::from_secs(10), notified.recv()).await.expect("no notification came");
    assert!(notice.unwrap().body.contains("welcome back"), "a message that arrives is notified");
}

#[tokio::test(flavor = "multi_thread")]
async fn with_the_phone_offline_a_message_still_goes() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
    running.chats(&[ANA]).await;
    running.connection(jaw_proto::ConnectionState::PhoneOffline);
    let id = running.send(ANA, "the phone can wait").await;
    running.until_status(ANA, &id, "read").await;
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
        code::NOT_FOUND,
        "a reply to a message jawd doesn't have"
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
        ("send.location", json!({ "chat": ANA, "lat": 38.7, "lon": -9.1 })),
        ("send.event", json!({ "chat": ANA, "name": "Jantar", "start": 1_892_000_000_000_i64 })),
        (
            "send.contact",
            json!({ "chat": ANA, "cards": [{ "name": "Rui", "vcard": "BEGIN:VCARD\nFN:Rui\nEND:VCARD" }] }),
        ),
        ("draft.set", json!({ "chat": ANA, "text": "two" })),
        ("read", json!({ "chat": ANA, "up_to": "m1" })),
        ("chat.start", json!({ "person": format!("person:{rui}") })),
    ];
    for (method, params) in calls {
        let before = synced();
        running.call(method, params).await.unwrap();
        assert_eq!(synced() - before, 1, "{method}");
    }
    let id =
        running.query(|db| Ok(db.query_row("SELECT id FROM outbox WHERE n = 1", [], |r| r.get::<_, String>(0))?)).await;
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
    let actions = [
        ("react", json!({ "message": "m1", "emoji": "👍" })),
        ("edit", json!({ "message": id, "text": "three" })),
        ("forward", json!({ "messages": ["m1", id], "to": [ANA, RUI] })),
        ("delete", json!({ "message": id, "for": "everyone" })),
        ("delete", json!({ "message": "m1", "for": "me" })),
    ];
    for (method, params) in actions {
        let before = synced();
        running.call(method, params.clone()).await.unwrap();
        assert_eq!(synced() - before, 1, "{method} {params}");
    }
    for (method, params) in [("message.info", json!({ "message": id })), ("chat.info", json!({ "chat": ANA }))] {
        let before = synced();
        running.call(method, params).await.unwrap();
        assert_eq!(synced() - before, 0, "{method} keeps nothing");
    }
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

/// jawd's socket in `home`, serving `running`'s sending.
fn serve(running: &Running, home: &Path) -> std::path::PathBuf {
    serving(running, home, false)
}

/// As `serve`, with windows too.
fn serve_watched(running: &Running, home: &Path) -> std::path::PathBuf {
    serving(running, home, true)
}

fn serving(running: &Running, home: &Path, watched: bool) -> std::path::PathBuf {
    let socket = jaw_proto::local::socket(home);
    let listener = crate::sock::listen(&socket).unwrap();
    let mut state = crate::state::State::new(
        home,
        Ok(running.supervisor.clone().unwrap()),
        Vec::new(),
        Arc::default(),
        Some(running.store.clone()),
        running.pushes.clone(),
    )
    .unwrap()
    .with_attention(running.attention.clone())
    .with_sending(running.sending.clone());
    if watched {
        state = state.with_feed(crate::feed::Feed::start(
            running.store.clone(),
            Arc::new(|| false),
            crate::feed::Timing::default(),
        ));
    }
    tokio::spawn(crate::sock::serve(Arc::new(state), listener));
    socket
}

/// The rows a window on `chat`'s newest messages is sent through `socket`, as an interface
/// watching it gets them: the window's own first, then each row upserted into it.
fn window(socket: &Path, chat: &str) -> mpsc::UnboundedReceiver<Value> {
    use std::io::{BufRead, BufReader, Write};

    let (rows, sent) = mpsc::unbounded_channel();
    let (socket, chat) = (socket.to_path_buf(), chat.to_string());
    std::thread::spawn(move || {
        let mut stream = std::os::unix::net::UnixStream::connect(&socket).unwrap();
        let reader = BufReader::new(stream.try_clone().unwrap());
        let hello = json!({ "id": 1, "method": "hello", "params": { "protocol": jaw_proto::PROTOCOL, "client": "test", "version": "0" } });
        let watch = json!({ "id": 2, "method": "messages.watch", "params": { "chat": chat, "around": "newest", "before": 50 } });
        stream.write_all(format!("{hello}\n{watch}\n").as_bytes()).unwrap();
        for line in reader.lines() {
            let Ok(line) = line else { return };
            let line: Value = serde_json::from_str(&line).unwrap();
            let fresh = match (line["id"].as_i64(), line["method"].as_str()) {
                (Some(2), _) => line["result"]["rows"].clone(),
                (None, Some("messages")) => line["params"]["upsert"].clone(),
                _ => continue,
            };
            for row in fresh.as_array().cloned().unwrap_or_default() {
                if rows.send(row).is_err() {
                    return;
                }
            }
        }
    });
    sent
}

/// Waits for `window` to be sent message `id` showing as `shows` says.
async fn shown(
    window: &mut mpsc::UnboundedReceiver<Value>,
    id: &str,
    what: &str,
    shows: impl Fn(&Value) -> bool,
) -> Value {
    let found = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let row = window.recv().await.expect("the window closed");
            if row["id"] == id && shows(&row) {
                return row;
            }
        }
    });
    found.await.unwrap_or_else(|_| panic!("the window wasn't sent {id} {what}"))
}

/// Says hello on `socket`, then asks each of `requests` in turn, each with the next id, and gives
/// each one's reply, pushes aside.
async fn ask_socket(socket: &Path, requests: Vec<(&'static str, Value)>) -> Vec<Value> {
    ask_socket_open(socket, requests).await.0
}

/// As `ask_socket`, with the connection, still focused, open until what it gives back is dropped.
async fn ask_socket_open(
    socket: &Path,
    requests: Vec<(&'static str, Value)>,
) -> (Vec<Value>, std::os::unix::net::UnixStream) {
    use std::io::{BufRead, BufReader, Write};

    let socket = socket.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut stream = std::os::unix::net::UnixStream::connect(&socket).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let hello =
            ("hello", json!({ "protocol": jaw_proto::PROTOCOL, "client": "test", "version": "0", "focus": true }));
        let replies = std::iter::once(hello)
            .chain(requests)
            .enumerate()
            .map(|(id, (method, params))| {
                let line = json!({ "id": id, "method": method, "params": params });
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
            .skip(1)
            .collect::<Vec<_>>();
        (replies, stream)
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn sending_goes_through_the_socket_with_the_pending_row_in_its_answer() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    let socket = serve(&running, home.path());
    let requests = vec![
        ("send.text", json!({ "chat": ANA, "text": "olá" })),
        ("typing", json!({ "chat": ANA, "state": "composing" })),
        ("chat.start", json!({ "phone": "+351 913 333 333" })),
    ];
    let (answers, connection) = ask_socket_open(&socket, requests).await;
    let mut replies = vec![Value::Null];
    replies.extend(answers);
    assert_eq!(replies[1]["result"]["row"]["status"], "pending", "{}", replies[1]);
    assert_eq!(replies[2]["result"], json!({}));
    assert_eq!(replies[3]["result"]["chat"]["id"], RUI);
    // You're online once an interface has had focus for a moment, which it has while it's connected.
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::Presence(p) if p.available))).await;
    drop(connection);
}

/// The raw message the engine was asked to send `nth`, counting from 1, once it has been.
async fn sent(running: &Running, nth: usize) -> wa_proto::wa_web_protobufs_e2e::Message {
    let sends = |asked: &[pb::command::Kind]| asked.iter().filter(|k| matches!(k, pb::command::Kind::Send(_))).count();
    running.world.until(|asked| sends(asked) >= nth).await;
    let send = running.world.sends().swap_remove(nth - 1);
    wa_proto::wa_web_protobufs_e2e::Message::decode(send.raw.as_slice()).unwrap()
}

/// Someone's name, as their contact has it.
async fn named(running: &Running, jid: &str, name: &str) -> i64 {
    let person = Person { jids: vec![jid.into()], name: Some(name.into()), ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(person)]).await.unwrap();
    let jid = jid.to_string();
    running
        .query(move |db| Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [jid], |r| r.get(0))?))
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reply_quotes_its_message_as_whatsapp_does_and_its_row_names_who_wrote_it() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    named(&running, ANA, "Ana Ribeiro").await;
    running.arrives(ANA, ANA, "m1", now_ms() - 60_000).await;
    let answer = running.call("send.text", json!({ "chat": ANA, "text": "sim!", "reply": "m1" })).await.unwrap();
    assert_eq!(
        answer["row"]["reply"],
        json!({ "id": "m1", "from": "Ana Ribeiro", "summary": "message m1" }),
        "the quote, named"
    );
    let raw = sent(&running, 1).await;
    let text = raw.extended_text_message.expect("a reply is extended text");
    let context = text.context_info.unwrap();
    assert_eq!(text.text.as_deref(), Some("sim!"));
    assert_eq!((context.stanza_id.as_deref(), context.participant.as_deref()), (Some("m1"), Some(ANA)));
    assert_eq!(context.quoted_message.unwrap().conversation.as_deref(), Some("message m1"));
    let mine = answer["message"].as_str().unwrap().to_string();
    let again = running.call("send.text", json!({ "chat": ANA, "text": "isto", "reply": mine })).await.unwrap();
    assert_eq!(again["row"]["reply"]["from"], "You", "a quote of yours");
    let context = sent(&running, 2).await.extended_text_message.unwrap().context_info.unwrap();
    assert_eq!(context.participant.as_deref(), Some(ME));
    assert_eq!(
        context.quoted_message.unwrap().extended_text_message.unwrap().context_info,
        None,
        "no quote in a quote"
    );
    running.chats(&[RUI]).await;
    let elsewhere = running.call("send.text", json!({ "chat": RUI, "text": "x", "reply": "m1" })).await.unwrap_err();
    assert_eq!(elsewhere.code, code::NOT_FOUND, "a reply is to a message in the same chat");
    let revoke = Amend {
        chat: ANA.into(),
        target: "m1".into(),
        sender: Some(ANA.into()),
        mine: false,
        at: now_ms(),
        what: Amendment::Revoke,
        raw: None,
    };
    running.store.apply(Lane::Live, vec![Change::Amend(revoke)]).await.unwrap();
    let deleted = running.call("send.text", json!({ "chat": ANA, "text": "x", "reply": "m1" })).await.unwrap_err();
    assert_eq!(deleted.code, code::NOT_ALLOWED);
}

#[tokio::test(flavor = "multi_thread")]
async fn mentions_go_as_whatsapp_writes_them_and_the_row_shows_each_by_name() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let sofia = "351915555555@s.whatsapp.net";
    running.members(CLIMBING, &[RUI, sofia]).await;
    let rui = named(&running, RUI, "Rui Costa").await;
    let ana = named(&running, ANA, "Ana").await;
    let params =
        json!({ "chat": CLIMBING, "text": "@Rui Costa vens? @Rui Costa!", "mentions": [format!("person:{rui}")] });
    let answer = running.call("send.text", params).await.unwrap();
    let text = sent(&running, 1).await.extended_text_message.unwrap();
    assert_eq!(text.text.as_deref(), Some("@351913333333 vens? @351913333333!"), "as WhatsApp writes them");
    assert_eq!(text.context_info.unwrap().mentioned_jid, [RUI]);
    let spans = &answer["row"]["spans"];
    assert_eq!(spans[0], json!({ "t": "mention", "s": "@Rui Costa", "person": RUI }), "{spans}");
    assert_eq!(spans[1], json!({ "t": "text", "s": " vens? " }));
    let stranger =
        json!({ "chat": CLIMBING, "text": "@Ana olá", "mentions": [format!("person:{ana}")], "key": "compose-1" });
    let refused = running.call("send.text", stranger).await.unwrap_err();
    assert_eq!((refused.code, refused.data), (code::INVALID_PARAMS, Some(json!({ "param": "mentions" }))));
    assert_eq!(refused.message, "Ana isn't in this group, so they can't be mentioned there", "named, not by ID");
    let named_group = Chat { id: CLIMBING.into(), name: Some("Lisbon Climbing".into()), ..Chat::default() };
    running.store.apply(Lane::Live, vec![Change::Chat(named_group)]).await.unwrap();
    let unnamed = "351919999999@s.whatsapp.net";
    running
        .store
        .apply(Lane::Live, vec![Change::Person(Person { jids: vec![unnamed.into()], ..Person::default() })])
        .await
        .unwrap();
    let number = running
        .query(move |db| {
            Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [unnamed], |r| r.get::<_, i64>(0))?)
        })
        .await;
    let params = json!({ "chat": CLIMBING, "text": "@x olá", "mentions": [format!("person:{number}")] });
    let refused = running.call("send.text", params).await.unwrap_err();
    assert_eq!(refused.message, "+351919999999 isn't in Lisbon Climbing, so they can't be mentioned there");
    running.chats(&[ANA]).await;
    let direct = json!({ "chat": ANA, "text": "@Ana olá", "mentions": [format!("person:{ana}")] });
    running.call("send.text", direct).await.unwrap();
    assert_eq!(
        sent(&running, 2).await.conversation.as_deref(),
        Some("@Ana olá"),
        "outside a group, mentions are dropped"
    );
    let sends = running.world.sends().len();
    assert_eq!(sends, 2, "nothing went for the refused one");
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_info_gives_a_groups_members_once_each_with_their_numbers_and_admins_and_a_direct_chats_two() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let rui_lid = "100000000003@lid";
    running.members(CLIMBING, &[RUI, rui_lid]).await;
    let both = Person { jids: vec![RUI.into(), rui_lid.into()], name: Some("Rui".into()), ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(both)]).await.unwrap();
    let admin = Members { chat: CLIMBING.into(), all: false, set: vec![(rui_lid.into(), "admin")], remove: Vec::new() };
    running.store.apply(Lane::Live, vec![Change::Members(admin)]).await.unwrap();
    named(&running, ME, "Pedro").await;
    let info = running.call("chat.info", json!({ "chat": CLIMBING })).await.unwrap();
    let members = info["members"].as_array().unwrap();
    let shown: Vec<(&str, Option<&str>, bool)> = members
        .iter()
        .map(|m| (m["name"].as_str().unwrap(), m["phone"].as_str(), m["admin"].as_bool().unwrap()))
        .collect();
    assert_eq!(shown, [("Pedro", Some("+351910000001"), false), ("Rui", Some("+351913333333"), true)], "{info}");
    assert!(members.iter().all(|m| m["person"].as_str().unwrap().starts_with("person:")));
    assert_eq!((info["kind"].as_str(), info["ephemeral"].as_u64()), (Some("group"), Some(0)));
    named(&running, ANA, "Ana").await;
    running.chats(&[ANA]).await;
    let direct = running.call("chat.info", json!({ "chat": ANA })).await.unwrap();
    let names: Vec<&str> = direct["members"].as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Ana", "Pedro"], "the other person and you");
    let unknown = running.call("chat.info", json!({ "chat": "351999999999@s.whatsapp.net" })).await.unwrap_err();
    assert_eq!(unknown.code, code::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn every_message_action_goes_through_the_socket_as_jaw_asks_for_it() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.members(CLIMBING, &[RUI]).await;
    let rui = named(&running, RUI, "Rui").await;
    running.arrives(CLIMBING, RUI, "g1", now_ms() - 1_000).await;
    running.arrives(CLIMBING, RUI, "g2", now_ms() - 900).await;
    running.chats(&[ANA]).await;
    let socket = serve(&running, home.path());
    let replies = ask_socket(
        &socket,
        vec![
            ("chat.info", json!({ "chat": CLIMBING })),
            (
                "send.text",
                json!({ "chat": CLIMBING, "text": "@Rui sim", "key": "jaw-1", "reply": "g1", "mentions": [format!("person:{rui}")] }),
            ),
            ("chat.info", json!({ "chat": "nobody@g.us" })),
            ("react", json!({ "message": "g1", "emoji": "❤️" })),
            ("forward", json!({ "messages": ["g1"], "to": [ANA] })),
            ("delete", json!({ "message": "g2", "for": "me" })),
        ],
    )
    .await;
    let names: Vec<&str> =
        replies[0]["result"]["members"].as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Rui"), "{}", replies[0]);
    let reply = &replies[1]["result"]["row"];
    assert_eq!(reply["reply"], json!({ "id": "g1", "from": "Rui", "summary": "message g1" }), "{reply}");
    assert_eq!(reply["spans"][0], json!({ "t": "mention", "s": "@Rui", "person": RUI }), "{reply}");
    assert_eq!(replies[2]["error"]["code"], code::NOT_FOUND);
    assert_eq!((&replies[3]["result"], &replies[5]["result"]), (&json!({}), &json!({})));
    let forwarded =
        replies[4]["result"]["messages"][0].as_str().unwrap_or_else(|| panic!("{}", replies[4])).to_string();
    let mine = replies[1]["result"]["message"].as_str().unwrap().to_string();
    let more = ask_socket(
        &socket,
        vec![
            ("edit", json!({ "message": mine, "text": "sim, vou" })),
            ("message.info", json!({ "message": mine })),
            ("delete", json!({ "message": forwarded, "for": "everyone" })),
            ("react", json!({ "message": "nope", "emoji": "❤️" })),
        ],
    )
    .await;
    assert_eq!((&more[0]["result"], &more[2]["result"]), (&json!({}), &json!({})), "{more:?}");
    let named = more[1]["result"]["recipients"].as_array().unwrap().iter().any(|r| r["name"] == "Rui");
    assert!(named, "{}", more[1]);
    assert_eq!(more[3]["error"]["code"], code::NOT_FOUND);
    until_stored(&running).await;
    assert!(row(&running, CLIMBING, "g2").await.is_none());
    assert_eq!(stored_text(&running, &mine).await, "sim, vou");
}

/// Message `id` in `chat` as a window shows it, if jawd has it.
async fn row(running: &Running, chat: &str, id: &str) -> Option<jaw_proto::windows::MessageRow> {
    let (chat, id) = (chat.to_string(), id.to_string());
    running.query(move |db| Ok(read::message(db, &chat, &id)?.map(|(row, _)| crate::feed::message_row(row)))).await
}

/// The commands of `kind` the engine was asked, as `pick` finds them.
fn asked<T>(running: &Running, pick: impl Fn(pb::command::Kind) -> Option<T>) -> Vec<T> {
    running.world.asked().into_iter().filter_map(pick).collect()
}

/// The raw messages the engine was asked to send, decoded.
fn raws(running: &Running) -> Vec<wa_proto::wa_web_protobufs_e2e::Message> {
    running
        .world
        .sends()
        .into_iter()
        .map(|send| wa_proto::wa_web_protobufs_e2e::Message::decode(send.raw.as_slice()).unwrap())
        .collect()
}

fn reactions(row: &jaw_proto::windows::MessageRow) -> Vec<(String, u32, bool)> {
    row.reactions.iter().map(|r| (r.emoji.clone(), r.n, r.mine)).collect()
}

fn text_of(row: &jaw_proto::windows::MessageRow) -> String {
    row.spans.as_array().unwrap().iter().map(|s| s["s"].as_str().unwrap().to_string()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_goes_as_a_message_of_its_own_and_is_on_the_row_once_whatsapp_has_it() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.arrives(ANA, ANA, "m1", now_ms() - 60_000).await;
    let other = Amend {
        chat: ANA.into(),
        target: "m1".into(),
        sender: Some(ANA.into()),
        mine: false,
        at: now_ms() - 30_000,
        what: Amendment::React { emoji: "👍".into() },
        raw: None,
    };
    running.store.apply(Lane::Live, vec![Change::Amend(other)]).await.unwrap();
    assert_eq!(running.call("react", json!({ "message": "m1", "emoji": "❤️" })).await.unwrap(), json!({}));
    let reaction = sent(&running, 1).await.reaction_message.expect("a reaction of its own");
    until_stored(&running).await;
    let reacted = row(&running, ANA, "m1").await.unwrap();
    assert_eq!(reactions(&reacted), [("👍".into(), 1, false), ("❤️".into(), 1, true)], "with Ana's");
    assert_eq!(stored_reactions(&running).await, 2, "on the row itself once WhatsApp has it");
    let key = reaction.key.unwrap();
    assert_eq!((key.id.as_deref(), key.from_me, key.remote_jid.as_deref()), (Some("m1"), Some(false), Some(ANA)));
    assert_eq!(reaction.text.as_deref(), Some("❤️"));
    let id = running.world.sends()[0].id.clone();
    assert!(id.starts_with("3EB0") && id.len() == 22, "with an ID of its own: {id}");
    running.call("react", json!({ "message": "m1", "emoji": "👍", "chat": ANA })).await.unwrap();
    sent(&running, 2).await;
    until_stored(&running).await;
    assert_eq!(reactions(&row(&running, ANA, "m1").await.unwrap()), [("👍".into(), 2, true)], "another replaces it");
    running.call("react", json!({ "message": "m1", "emoji": "" })).await.unwrap();
    sent(&running, 3).await;
    until_stored(&running).await;
    assert_eq!(reactions(&row(&running, ANA, "m1").await.unwrap()), [("👍".into(), 1, false)], "and none removes it");
    assert_eq!(stored_reactions(&running).await, 1);
    // Ana's phone says it has each reaction, a receipt for each one's own ID.
    tokio::time::sleep(Duration::from_millis(100)).await;
    eventually(&running, "the receipts converted", |db| {
        Ok(db.query_row("SELECT count(*) FROM inbox", [], |r| r.get::<_, i64>(0))? == 0)
    })
    .await;
    let pending =
        running.query(|db| Ok(db.query_row("SELECT count(*) FROM pending", [], |r| r.get::<_, i64>(0))?)).await;
    assert_eq!(pending, 0, "the receipts for a reaction's own ID don't wait for a message that won't come");
    let refused = |params: Value| running.call("react", params);
    assert_eq!(refused(json!({ "message": "nope", "emoji": "❤️" })).await.unwrap_err().code, code::NOT_FOUND);
    assert_eq!(refused(json!({ "message": "m1", "emoji": "a b" })).await.unwrap_err().code, code::INVALID_PARAMS);
    let revoke = Amend { what: Amendment::Revoke, at: now_ms(), ..other_of(ANA, "m1") };
    running.store.apply(Lane::Live, vec![Change::Amend(revoke)]).await.unwrap();
    assert_eq!(refused(json!({ "message": "m1", "emoji": "❤️" })).await.unwrap_err().code, code::NOT_ALLOWED);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_action_finds_its_message_in_the_chat_named_or_in_the_one_chat_that_has_its_id() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.arrives(ANA, ANA, "only", now_ms()).await;
    running.arrives(ANA, ANA, "twice", now_ms()).await;
    running.arrives(RUI, RUI, "twice", now_ms()).await;
    let react = |params: Value| running.call("react", params);
    assert_eq!(react(json!({ "message": "only", "emoji": "👍" })).await.unwrap(), json!({}), "found in its one chat");
    let ambiguous = react(json!({ "message": "twice", "emoji": "👍" })).await.unwrap_err();
    assert_eq!((ambiguous.code, ambiguous.data), (code::INVALID_PARAMS, Some(json!({ "param": "chat" }))));
    assert_eq!(react(json!({ "message": "twice", "emoji": "👍", "chat": RUI })).await.unwrap(), json!({}));
    let elsewhere = react(json!({ "message": "only", "emoji": "👍", "chat": RUI })).await.unwrap_err();
    assert_eq!(elsewhere.code, code::NOT_FOUND, "not in the chat named, though another has it");
    sent(&running, 2).await;
    let chats: Vec<String> = running.world.sends().into_iter().map(|s| s.chat).collect();
    assert_eq!(chats, [ANA, RUI]);
}

/// An amendment from the other person in a one-to-one chat with them.
fn other_of(chat: &str, target: &str) -> Amend {
    Amend {
        chat: chat.into(),
        target: target.into(),
        sender: Some(chat.into()),
        mine: false,
        at: now_ms(),
        what: Amendment::Status { status: "delivered".into() },
        raw: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_is_on_the_row_once_whatsapp_has_it_and_one_whatsapp_refuses_leaves_it_as_it_was() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let mut pushes = running.pushes.subscribe();
    running.chats(&[ANA]).await;
    let id = running.send(ANA, "chego às 20h").await;
    running.until_status(ANA, &id, "sent").await;
    running.call("edit", json!({ "message": id, "text": "afinal chego às *21h*" })).await.unwrap();
    let raw = sent(&running, 2).await;
    until_stored(&running).await;
    assert_eq!(stored_text(&running, &id).await, "afinal chego às *21h*", "on the row itself once WhatsApp has it");
    let edited = row(&running, ANA, &id).await.unwrap();
    assert_eq!(text_of(&edited), "afinal chego às 21h");
    assert_eq!(edited.spans[1], json!({ "t": "bold", "s": "21h" }));
    let protocol = raw.edited_message.unwrap().message.unwrap().protocol_message.unwrap();
    assert_eq!(protocol.r#type, Some(wa_proto::wa_web_protobufs_e2e::protocol_message::Type::MessageEdit as i32));
    assert_eq!(
        (protocol.key.unwrap().id.as_deref(), protocol.edited_message.unwrap().conversation.as_deref()),
        (Some(id.as_str()), Some("afinal chego às *21h*"))
    );
    // WhatsApp's server has it from 25 minutes ago, past its window, though jaw has it as just now.
    running.world.sent_at(&id, now_ms() - 25 * 60 * 1000);
    running.call("edit", json!({ "message": id, "text": "não vou" })).await.unwrap();
    let notice = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let push = pushes.recv().await.unwrap();
            if push.method == "notice" {
                return push.params["text"].as_str().unwrap().to_string();
            }
        }
    })
    .await
    .expect("the refusal is said");
    assert!(notice.starts_with("WhatsApp refused your edit"), "{notice}");
    until_stored(&running).await;
    assert_eq!(text_of(&row(&running, ANA, &id).await.unwrap()), "afinal chego às 21h", "the refused edit left it");
    assert_eq!(stored_text(&running, &id).await, "afinal chego às *21h*");
    running.arrives(ANA, ANA, "theirs", now_ms()).await;
    let refused = |params: Value| running.call("edit", params);
    assert_eq!(refused(json!({ "message": "theirs", "text": "x" })).await.unwrap_err().code, code::NOT_ALLOWED);
    assert_eq!(refused(json!({ "message": id, "text": "  " })).await.unwrap_err().code, code::INVALID_PARAMS);
    let old = Message {
        chat: ANA.into(),
        live: true,
        status: Some("read"),
        ..mine_message("old", now_ms() - 16 * 60 * 1000)
    };
    running.store.apply(Lane::Live, vec![Change::Message(old)]).await.unwrap();
    let late = refused(json!({ "message": "old", "text": "x" })).await.unwrap_err();
    assert_eq!(
        (late.code, late.message.as_str()),
        (code::NOT_ALLOWED, "a message can be edited for 15 minutes after it's sent")
    );
}

/// A message of yours as it would come from your phone.
fn mine_message(id: &str, at: i64) -> Message {
    Message {
        id: id.into(),
        at,
        mine: true,
        kind: "text",
        status: Some("sent"),
        text: Some(format!("message {id}")),
        summary: format!("message {id}"),
        ..Message::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_for_everyone_leaves_you_deleted_this_message_once_whatsapp_has_it() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    let id = running.send(ANA, "ops").await;
    running.until_status(ANA, &id, "sent").await;
    running.call("delete", json!({ "message": id, "for": "everyone" })).await.unwrap();
    let raw = sent(&running, 2).await;
    until_stored(&running).await;
    let deleted = row(&running, ANA, &id).await.unwrap();
    assert_eq!((deleted.spans.clone(), deleted.reply.clone()), (json!([]), None));
    let chat = running.query(|db| Ok(read::chats(db, false, None, 10)?.remove(0))).await;
    assert_eq!(chat.last.unwrap().summary, "You deleted this message");
    let revoke = raw.protocol_message.unwrap();
    assert_eq!(revoke.r#type, Some(wa_proto::wa_web_protobufs_e2e::protocol_message::Type::Revoke as i32));
    assert_eq!(revoke.key.unwrap().id.as_deref(), Some(id.as_str()));
    let again = running.call("delete", json!({ "message": id, "for": "everyone" })).await.unwrap();
    assert_eq!(again, json!({}), "deleted already");
    running.arrives(ANA, ANA, "theirs", now_ms()).await;
    let theirs = running.call("delete", json!({ "message": "theirs", "for": "everyone" })).await.unwrap_err();
    assert_eq!(theirs.code, code::NOT_ALLOWED);
    let old = Message { chat: ANA.into(), live: true, ..mine_message("old", now_ms() - 61 * 3600 * 1000) };
    running.store.apply(Lane::Live, vec![Change::Message(old)]).await.unwrap();
    let late = running.call("delete", json!({ "message": "old", "for": "everyone" })).await.unwrap_err();
    assert_eq!(late.code, code::NOT_ALLOWED);
    assert_eq!(raws(&running).len(), 2, "nothing went for what was refused");
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_for_you_is_done_at_once_and_your_other_devices_are_told() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.members(CLIMBING, &[RUI]).await;
    running.arrives(CLIMBING, RUI, "g1", 1_000).await;
    running.call("delete", json!({ "message": "g1", "for": "me", "key": "jaw-del-1" })).await.unwrap();
    assert!(row(&running, CLIMBING, "g1").await.is_none(), "gone at once");
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::AppStatePatch(_)))).await;
    let patches = asked(&running, |k| match k {
        pb::command::Kind::AppStatePatch(p) => Some(p),
        _ => None,
    });
    assert_eq!(patches.len(), 1);
    assert_eq!((patches[0].name.as_str(), patches[0].version), ("regular_high", 3));
    assert_eq!(patches[0].index, ["deleteMessageForMe", CLIMBING, "g1", "0", RUI]);
    let value = SyncActionValue::decode(patches[0].value.as_slice()).unwrap();
    assert_eq!(value.delete_message_for_me_action.unwrap().message_timestamp, Some(1));
    until_stored(&running).await;
    let again = running.call("delete", json!({ "message": "g1", "for": "me", "key": "jaw-del-1" })).await.unwrap();
    assert_eq!(again, json!({}), "asked again with its key");
    let gone = running.call("delete", json!({ "message": "g1", "for": "me" })).await.unwrap_err();
    assert_eq!(gone.code, code::NOT_FOUND);
    let copy = Message {
        chat: CLIMBING.into(),
        id: "g1".into(),
        at: 1_000,
        sender: Some(RUI.into()),
        kind: "text",
        ..Message::default()
    };
    running.store.apply(Lane::Live, vec![Change::Message(copy)]).await.unwrap();
    assert!(row(&running, CLIMBING, "g1").await.is_none(), "a copy arriving later doesn't bring it back");
    assert_eq!(running.world.sends().len(), 0, "and no message went to anyone");
}

#[tokio::test(flavor = "multi_thread")]
async fn actions_wait_their_turn_behind_their_chats_earlier_sends_and_while_whatsapp_cant_be_reached() {
    use jaw_proto::ConnectionState::Online;
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.chats(&[ANA]).await;
    let id = running.send(ANA, "chego às 20h").await;
    running.call("react", json!({ "message": id, "emoji": "🙏" })).await.unwrap();
    running.call("edit", json!({ "message": id, "text": "chego às 21h" })).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(running.world.sends().is_empty(), "nothing goes while WhatsApp can't be reached");
    let waiting = row(&running, ANA, &id).await.unwrap();
    assert_eq!(
        (text_of(&waiting), reactions(&waiting)),
        ("chego às 21h".into(), vec![("🙏".into(), 1, true)]),
        "shown"
    );
    assert_eq!(stored_text(&running, &id).await, "chego às 20h", "but the row has nothing WhatsApp hasn't");
    running.connection(Online);
    until_stored(&running).await;
    let sent = raws(&running);
    assert_eq!(sent.len(), 3, "{sent:?}");
    assert!(
        sent[0].conversation.is_some() && sent[1].reaction_message.is_some() && sent[2].edited_message.is_some(),
        "in order"
    );
    assert_eq!(stored_text(&running, &id).await, "chego às 21h");
    assert_eq!(stored_reactions(&running).await, 1);
}

/// What each message the engine was asked to send is, in order: its text, or what kind of action.
fn kinds_sent(running: &Running) -> Vec<String> {
    raws(running)
        .iter()
        .map(|m| match (&m.conversation, &m.edited_message, &m.protocol_message, &m.reaction_message) {
            (Some(text), ..) => text.clone(),
            (_, Some(_), ..) => "an edit".into(),
            (_, _, Some(_), _) => "a deletion".into(),
            (.., Some(_)) => "a reaction".into(),
            _ => "something else".into(),
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_of_a_message_whatsapp_refuses_waits_for_it_and_goes_after_it_once_it_is_sent_again() {
    use jaw_proto::ConnectionState::Online;
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.world.replying(Replies { delivered: Duration::from_secs(3_600), read: Duration::from_secs(3_600) });
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.chats(&[ANA]).await;
    let id = running.send(ANA, "hi").await;
    running.call("edit", json!({ "message": id, "text": "hello" })).await.unwrap();
    running.world.refuse_next(1);
    running.connection(Online);
    running.until_status(ANA, &id, "failed").await;
    until_held(&running).await;
    assert_eq!(kinds_sent(&running), ["hi"], "the edit doesn't go ahead of its message");
    let waiting = row(&running, ANA, &id).await.unwrap();
    assert_eq!((text_of(&waiting), waiting.status.as_deref()), ("hello".into(), Some("failed")), "but it's shown");
    assert_eq!(stored_text(&running, &id).await, "hi");
    running.call("send.retry", json!({ "message": id })).await.unwrap();
    until_stored(&running).await;
    assert_eq!(kinds_sent(&running), ["hi", "hi", "an edit"], "it goes once its message has");
    assert_eq!(stored_text(&running, &id).await, "hello", "so jaw and the recipients both have hello");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deletion_for_everyone_of_a_message_whatsapp_refuses_leaves_nothing_of_it_to_send_again() {
    use jaw_proto::ConnectionState::Online;
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.world.replying(Replies { delivered: Duration::from_secs(3_600), read: Duration::from_secs(3_600) });
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.chats(&[ANA]).await;
    let id = running.send(ANA, "hi").await;
    running.call("delete", json!({ "message": id, "for": "everyone" })).await.unwrap();
    running.world.refuse_next(1);
    running.world.gate(true);
    running.connection(Online);
    running.world.until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::Send(_))).count() == 2).await;
    let retried = running.call("send.retry", json!({ "message": id })).await.unwrap_err();
    assert_eq!(retried.code, code::NOT_ALLOWED, "not while its deletion is on its way: {}", retried.message);
    running.world.gate(false);
    until_stored(&running).await;
    assert_eq!(kinds_sent(&running), ["hi", "a deletion"]);
    let deleted = row(&running, ANA, &id).await.unwrap();
    assert!(deleted.deleted, "deleted, as nobody has it");
    let retried = running.call("send.retry", json!({ "message": id })).await.unwrap_err();
    assert_eq!(retried.code, code::NOT_FOUND, "nothing of it is left to send again: {}", retried.message);
}

/// The outbox's rows as (kind, state), oldest first, and what counts toward its thousand.
async fn outbox_rows(running: &Running) -> (Vec<(String, String)>, (u64, u64)) {
    running
        .query(|db| {
            let rows = db
                .prepare("SELECT kind, state FROM outbox ORDER BY n")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            Ok((rows, crate::store::outbox::waiting(db)?))
        })
        .await
}

/// Message `id` of yours in Ana's chat deleted for everyone from your phone.
fn deleted_from_your_phone(id: &str) -> Change {
    Change::Amend(Amend {
        chat: ANA.into(),
        target: id.into(),
        sender: Some(ME.into()),
        mine: true,
        at: now_ms(),
        what: Amendment::Revoke,
        raw: None,
    })
}

/// Message `id` of yours in Ana's chat deleted for you on your phone, as its app state says.
fn deleted_for_you_on_your_phone(id: &str) -> Change {
    Change::Amend(Amend {
        chat: ANA.into(),
        target: id.into(),
        sender: None,
        mine: true,
        at: now_ms(),
        what: Amendment::DeleteForMe,
        raw: None,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_message_deleted_for_you_leaves_nothing_to_send_again() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    running.world.refuse_next(1);
    let id = running.send(ANA, "nobody home").await;
    running.until_status(ANA, &id, "failed").await;
    running.call("delete", json!({ "message": id, "for": "me" })).await.unwrap();
    let app_state_gone = |db: &Connection| {
        Ok(db.query_row("SELECT count(*) FROM outbox WHERE kind = 'app_state'", [], |r| r.get::<_, i64>(0))? == 0)
    };
    eventually(&running, "the app state patch to go", app_state_gone).await;
    assert_eq!(outbox_rows(&running).await, (Vec::new(), (0, 0)), "nothing holds a place in the thousand");
    let retried = running.call("send.retry", json!({ "message": id })).await.unwrap_err();
    assert_eq!(retried.code, code::NOT_FOUND, "{}", retried.message);
    assert_eq!(kinds_sent(&running), ["nobody home"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_message_deleted_for_everyone_from_another_device_leaves_nothing_to_send_again() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.chats(&[ANA]).await;
    running.world.refuse_next(1);
    let id = running.send(ANA, "and this").await;
    running.until_status(ANA, &id, "failed").await;
    running.store.apply(Lane::Live, vec![deleted_from_your_phone(&id)]).await.unwrap();
    assert_eq!(outbox_rows(&running).await, (Vec::new(), (0, 0)), "nothing holds a place in the thousand");
    assert!(row(&running, ANA, &id).await.unwrap().deleted);
    let retried = running.call("send.retry", json!({ "message": id })).await.unwrap_err();
    assert_eq!(retried.code, code::NOT_FOUND, "{}", retried.message);
    assert_eq!(kinds_sent(&running), ["and this"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unconfirmed_message_deleted_for_everyone_from_another_device_leaves_nothing_to_send_again() {
    let home = tempfile::tempdir().unwrap();
    let id = crash(home.path(), "held").await;
    let world = world();
    world.refuse_next(1);
    let timing = Timing { unconfirmed_for: Duration::from_millis(300), ..quick() };
    let running = Running::timed(home.path(), world, true, true, timing).await;
    let unconfirmed = |db: &Connection| {
        Ok(db.query_row("SELECT count(*) FROM outbox WHERE state = 'unconfirmed'", [], |r| r.get::<_, i64>(0))? == 1)
    };
    eventually(&running, "the resend's refusal", unconfirmed).await;
    running.store.apply(Lane::Live, vec![deleted_from_your_phone(&id)]).await.unwrap();
    assert_eq!(outbox_rows(&running).await, (Vec::new(), (0, 0)), "it no longer waits for a receipt");
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(outbox_rows(&running).await, (Vec::new(), (0, 0)), "and isn't left failed once its time has passed");
    let retried = running.call("send.retry", json!({ "message": id })).await.unwrap_err();
    assert_eq!(retried.code, code::NOT_FOUND, "{}", retried.message);
    assert_eq!(kinds_sent(&running), ["did it go?"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_message_whose_deletion_is_kept_for_a_late_receipt_leaves_nothing_to_send_again() {
    let home = tempfile::tempdir().unwrap();
    let id = crash(home.path(), "deletion behind").await;
    let world = world();
    world.replying(Replies { delivered: Duration::from_secs(3_600), read: Duration::from_secs(3_600) });
    world.refuse_next(2);
    let timing = Timing { unconfirmed_for: Duration::from_millis(300), ..quick() };
    let running = Running::timed(home.path(), world, true, true, timing).await;
    // Both are refused once jawd starts, when they may have gone, and no receipt comes in their time.
    let settled = |db: &Connection| {
        let count = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0));
        Ok(count("SELECT count(*) FROM outbox WHERE state = 'unconfirmed'")? == 0
            && count("SELECT count(*) FROM outbox WHERE kind = 'revoke' AND state = 'failed'")? == 1)
    };
    eventually(&running, "the deletion kept for a late receipt", settled).await;
    assert_eq!(
        outbox_rows(&running).await,
        (vec![("revoke".to_string(), "failed".to_string())], (0, 0)),
        "only the deletion, kept outside the thousand, and nothing of the message it deletes"
    );
    let retried = running.call("send.retry", json!({ "message": id })).await.unwrap_err();
    assert_eq!(retried.code, code::NOT_FOUND, "{}", retried.message);
    let deletion: String =
        running.query(|db| Ok(db.query_row("SELECT id FROM outbox WHERE kind = 'revoke'", [], |r| r.get(0))?)).await;
    running.receipt(ANA, &deletion, ANA, "delivered").await;
    assert!(row(&running, ANA, &id).await.unwrap().deleted, "a receipt for the deletion still does it");
    assert_eq!(outbox_rows(&running).await, (Vec::new(), (0, 0)));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deletion_held_across_a_merge_goes_once_its_message_is_unconfirmed_with_nothing_else_committed() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.world.replying(Replies { delivered: Duration::from_secs(3_600), read: Duration::from_secs(3_600) });
    let lid = "100000000001@lid";
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.chats(&[ANA, lid]).await;
    running.world.fail_chats(&[ANA.to_string(), lid.to_string()]);
    let id = running.send(lid, "um").await;
    running.world.until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::Send(_))).count() >= 2).await;
    let both = Person { jids: vec![ANA.into(), lid.into()], ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(both)]).await.unwrap();
    running.call("delete", json!({ "message": id, "for": "everyone" })).await.unwrap();
    until_held(&running).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Its next try is refused when it may have gone: kept waiting for a receipt, which writes no
    // event for anything that waits on commits to see.
    running.world.refuse_next(1);
    running.world.stop_failing_chats();
    let went = tokio::time::timeout(Duration::from_secs(5), async {
        while !kinds_sent(&running).iter().any(|sent| sent == "a deletion") {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(went.is_ok(), "the deletion goes once its message is unconfirmed, with nothing else committed");
    until_stored(&running).await;
    assert!(row(&running, ANA, &id).await.unwrap().deleted);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_outbox_full_of_messages_whatsapp_refuses_for_good_is_emptied_by_deleting_them_for_you() {
    use jaw_proto::ConnectionState::Online;
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.chats(&[ANA]).await;
    running.arrives(ANA, ANA, "theirs", now_ms()).await;
    {
        let mut db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        let tx = db.transaction().unwrap();
        // 998 messages of yours WhatsApp refused, one still to go and one waiting for a receipt.
        let failed = (0..998).map(|n| (format!("w{n}"), "failed", "failed"));
        let waiting =
            [("queued".to_string(), "pending", "queued"), ("unconfirmed".to_string(), "pending", "unconfirmed")];
        for (n, (id, status, state)) in failed.chain(waiting).enumerate() {
            tx.execute(
                "INSERT INTO messages (chat, id, at, t, kind, mine, status, v)
                VALUES ((SELECT n FROM chats WHERE id = ?3), ?1, ?2, ?2, 'text', 1, ?4, 1)",
                rusqlite::params![id, n as i64, ANA, status],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO outbox (message, jid, id, kind, raw, state, next_at, created_at)
                VALUES (last_insert_rowid(), ?1, ?2, 'text', x'', ?3, ?4, 0)",
                rusqlite::params![ANA, id, state, now_ms() + 3_600_000],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    assert_eq!(outbox_rows(&running).await.1, (1_000, 998));
    // A deletion for you that frees nothing still needs a place: of someone else's message, which
    // has no row, or of one of yours whose row stays, a queued one still to go as on the phone and
    // an unconfirmed one to wait out its day.
    for id in ["theirs", "queued", "unconfirmed"] {
        let refused = running.call("delete", json!({ "message": id, "for": "me" })).await.unwrap_err();
        assert_eq!(refused.code, code::LIMIT, "{id}");
        assert_eq!(outbox_rows(&running).await.1, (1_000, 998), "{id}: no place taken");
        assert!(row(&running, ANA, id).await.is_some(), "{id}: not deleted");
    }
    running.call("delete", json!({ "message": "w5", "for": "me" })).await.unwrap();
    assert!(row(&running, ANA, "w5").await.is_none(), "one that frees a place is taken, full as it is");
    let (rows, waiting) = outbox_rows(&running).await;
    assert_eq!(waiting, (1_000, 997), "its place goes to the patch that tells your other devices");
    assert_eq!(rows.iter().filter(|(kind, _)| kind == "app_state").count(), 1);
    running.connection(Online);
    let told = |db: &Connection| {
        Ok(db.query_row("SELECT count(*) FROM outbox WHERE kind = 'app_state'", [], |r| r.get::<_, i64>(0))? == 0)
    };
    eventually(&running, "the patch to go", told).await;
    running.send(ANA, "and this goes").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn past_a_thousand_a_deletion_for_you_of_a_message_with_a_reaction_kept_for_a_late_receipt_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.chats(&[ANA]).await;
    running.arrives(ANA, ANA, "theirs", now_ms()).await;
    {
        let mut db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        let tx = db.transaction().unwrap();
        // A thousand messages of yours WhatsApp refused, and your reaction to Ana's, which had no
        // receipt in its day and is kept for a late one, outside the thousand.
        for n in 0..1_000 {
            let id = format!("w{n}");
            tx.execute(
                "INSERT INTO messages (chat, id, at, t, kind, mine, status, v)
                VALUES ((SELECT n FROM chats WHERE id = ?3), ?1, ?2, ?2, 'text', 1, 'failed', 1)",
                rusqlite::params![id, n as i64, ANA],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO outbox (message, jid, id, kind, raw, state, next_at, created_at)
                VALUES (last_insert_rowid(), ?1, ?2, 'text', x'', 'failed', ?3, 0)",
                rusqlite::params![ANA, id, now_ms() + 3_600_000],
            )
            .unwrap();
        }
        tx.execute(
            "INSERT INTO outbox (target, jid, id, kind, raw, effect, state, next_at, created_at)
            VALUES ((SELECT n FROM messages WHERE id = 'theirs'), ?1, 'kept', 'reaction', x'',
                '{\"do\":\"react\",\"emoji\":\"👍\"}', 'failed', 0, ?2)",
            rusqlite::params![ANA, now_ms()],
        )
        .unwrap();
        tx.commit().unwrap();
    }
    assert_eq!(outbox_rows(&running).await.1, (1_000, 1_000));
    // The kept reaction frees no place: it doesn't count, and a deletion for you leaves it.
    let refused = running.call("delete", json!({ "message": "theirs", "for": "me" })).await.unwrap_err();
    assert_eq!(refused.code, code::LIMIT, "{}", refused.message);
    assert_eq!(outbox_rows(&running).await.1, (1_000, 1_000), "no place taken");
    assert!(row(&running, ANA, "theirs").await.is_some(), "not deleted");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_held_on_a_message_your_phone_then_deletes_for_everyone_isnt_sent() {
    let home = tempfile::tempdir().unwrap();
    let id = crash(home.path(), "held").await;
    let world = world();
    world.refuse_next(1);
    let running = Running::on(home.path(), world, true).await;
    let unconfirmed = |db: &Connection| {
        Ok(db.query_row("SELECT count(*) FROM outbox WHERE state = 'unconfirmed'", [], |r| r.get::<_, i64>(0))? == 1)
    };
    eventually(&running, "the resend's refusal", unconfirmed).await;
    let mut pushes = running.pushes.subscribe();
    running.call("react", json!({ "message": id, "emoji": "👍" })).await.unwrap();
    until_held(&running).await;
    running.store.apply(Lane::Live, vec![deleted_from_your_phone(&id)]).await.unwrap();
    until_stored(&running).await;
    assert_eq!(kinds_sent(&running), ["did it go?"], "the reaction isn't sent for a deleted message");
    let shown = row(&running, ANA, &id).await.unwrap();
    assert!(shown.deleted && shown.reactions.is_empty(), "nor kept on it: {:?}", shown.reactions);
    assert_eq!(stored_reactions(&running).await, 0);
    let said = notices(&mut pushes, Duration::from_millis(500)).await;
    assert_eq!(said, ["Your reaction wasn't sent: the message was deleted for everyone"], "said once");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_on_its_way_to_a_message_deleted_for_everyone_before_it_goes_isnt_sent() {
    use jaw_proto::ConnectionState::Online;
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
    running.arrives(ANA, ANA, "m1", now_ms() - 60_000).await;
    running.call("react", json!({ "message": "m1", "emoji": "❤️" })).await.unwrap();
    // Ana deletes it for everyone before WhatsApp can be reached.
    running
        .store
        .apply(Lane::Live, vec![Change::Amend(Amend { what: Amendment::Revoke, ..other_of(ANA, "m1") })])
        .await
        .unwrap();
    let shown = row(&running, ANA, "m1").await.unwrap();
    assert!(shown.deleted && shown.reactions.is_empty(), "nor shown while it waits: {:?}", shown.reactions);
    running.connection(Online);
    until_stored(&running).await;
    assert!(kinds_sent(&running).is_empty(), "{:?}", kinds_sent(&running));
    let shown = row(&running, ANA, "m1").await.unwrap();
    assert!(shown.deleted && shown.reactions.is_empty(), "{:?}", shown.reactions);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_whatsapp_turns_out_to_have_as_it_is_withdrawn_isnt_said_not_to_have_been_sent() {
    use jaw_proto::ConnectionState::Online;
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.arrives(ANA, ANA, "m1", now_ms() - 60_000).await;
    running.call("react", json!({ "message": "m1", "emoji": "❤️" })).await.unwrap();
    running
        .store
        .apply(Lane::Live, vec![Change::Amend(Amend { what: Amendment::Revoke, ..other_of(ANA, "m1") })])
        .await
        .unwrap();
    let reaction: String =
        running.query(|db| Ok(db.query_row("SELECT id FROM outbox WHERE kind = 'reaction'", [], |r| r.get(0))?)).await;
    let mut pushes = running.pushes.subscribe();
    let (stopped, go) = running.sending.pause_next_withdrawal();
    running.connection(Online);
    stopped.notified().await;
    // Its receipt lands once the attempt has read it as still to go, and before it withdraws it:
    // WhatsApp had it, from a try before jawd last stopped.
    running.receipt(ANA, &reaction, ANA, "delivered").await;
    go.notify_one();
    until_stored(&running).await;
    assert!(kinds_sent(&running).is_empty(), "{:?}", kinds_sent(&running));
    let said = notices(&mut pushes, Duration::from_millis(500)).await;
    assert!(said.is_empty(), "nothing is said not to have been sent: {said:?}");
    assert_eq!(stored_reactions(&running).await, 1, "it's done, as WhatsApp has it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_deleted_for_everyone_shows_no_reactions_though_jaw_db_keeps_those_whatsapp_has() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.arrives(ANA, ANA, "m1", now_ms() - 60_000).await;
    let hers = Amend { what: Amendment::React { emoji: "👍".into() }, at: now_ms() - 30_000, ..other_of(ANA, "m1") };
    running.store.apply(Lane::Live, vec![Change::Amend(hers)]).await.unwrap();
    // Yours is on its way when Ana deletes the message for everyone, and WhatsApp has it then.
    running.world.gate(true);
    running.call("react", json!({ "message": "m1", "emoji": "❤️" })).await.unwrap();
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::Send(_)))).await;
    running
        .store
        .apply(Lane::Live, vec![Change::Amend(Amend { what: Amendment::Revoke, ..other_of(ANA, "m1") })])
        .await
        .unwrap();
    let shown = row(&running, ANA, "m1").await.unwrap();
    assert!(shown.deleted && shown.reactions.is_empty(), "neither Ana's nor yours on its way: {:?}", shown.reactions);
    running.world.gate(false);
    until_stored(&running).await;
    assert_eq!(kinds_sent(&running), ["a reaction"]);
    let shown = row(&running, ANA, "m1").await.unwrap();
    assert!(shown.deleted && shown.reactions.is_empty(), "nor yours once WhatsApp has it: {:?}", shown.reactions);
    assert_eq!(stored_reactions(&running).await, 2, "both kept, as WhatsApp keeps them");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_youre_deleting_for_everyone_shows_no_reactions_while_your_deletion_is_on_its_way() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    let id = running.send(ANA, "chego às 20h").await;
    running.until_status(ANA, &id, "sent").await;
    let hers = Amend { what: Amendment::React { emoji: "😮".into() }, ..other_of(ANA, &id) };
    running.store.apply(Lane::Live, vec![Change::Amend(hers)]).await.unwrap();
    assert_eq!(reactions(&row(&running, ANA, &id).await.unwrap()), [("😮".into(), 1, false)]);
    until_offline(&running).await;
    running.call("delete", json!({ "message": id, "for": "everyone" })).await.unwrap();
    let shown = row(&running, ANA, &id).await.unwrap();
    assert!(shown.deleted && shown.reactions.is_empty(), "{:?}", shown.reactions);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_on_its_way_to_a_message_your_phone_then_deletes_for_everyone_isnt_sent() {
    use jaw_proto::ConnectionState::Online;
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    let id = running.send(ANA, "chego às 20h").await;
    running.until_status(ANA, &id, "sent").await;
    until_offline(&running).await;
    running.call("edit", json!({ "message": id, "text": "chego às 21h" })).await.unwrap();
    let mut pushes = running.pushes.subscribe();
    running.store.apply(Lane::Live, vec![deleted_from_your_phone(&id)]).await.unwrap();
    running.connection(Online);
    until_stored(&running).await;
    assert_eq!(kinds_sent(&running), ["chego às 20h"], "only the message went, not its edit");
    let said = notices(&mut pushes, Duration::from_millis(500)).await;
    assert_eq!(said, ["Your edit wasn't sent: the message was deleted for everyone"]);
    let shown = row(&running, ANA, &id).await.unwrap();
    assert!(shown.deleted && !shown.edited, "{shown:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_on_its_way_to_a_message_deleted_for_everyone_and_then_for_you_isnt_sent() {
    use jaw_proto::ConnectionState::Online;
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.arrives(ANA, ANA, "m1", now_ms() - 60_000).await;
    running.call("react", json!({ "message": "m1", "emoji": "❤️" })).await.unwrap();
    let mut pushes = running.pushes.subscribe();
    // Before WhatsApp can be reached, Ana deletes it for everyone, and then you delete it for you.
    running
        .store
        .apply(Lane::Live, vec![Change::Amend(Amend { what: Amendment::Revoke, ..other_of(ANA, "m1") })])
        .await
        .unwrap();
    running.call("delete", json!({ "message": "m1", "for": "me" })).await.unwrap();
    assert!(row(&running, ANA, "m1").await.is_none());
    running.connection(Online);
    until_stored(&running).await;
    assert!(kinds_sent(&running).is_empty(), "{:?}", kinds_sent(&running));
    let said = notices(&mut pushes, Duration::from_millis(500)).await;
    assert_eq!(said, ["Your reaction wasn't sent: the message was deleted for everyone"]);
    assert_eq!(stored_reactions(&running).await, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_on_its_way_to_a_message_your_phone_deletes_for_everyone_and_then_for_you_isnt_sent() {
    use jaw_proto::ConnectionState::Online;
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    let id = running.send(ANA, "chego às 20h").await;
    running.until_status(ANA, &id, "sent").await;
    until_offline(&running).await;
    running.call("edit", json!({ "message": id, "text": "chego às 21h" })).await.unwrap();
    let mut pushes = running.pushes.subscribe();
    running.store.apply(Lane::Live, vec![deleted_from_your_phone(&id)]).await.unwrap();
    running.store.apply(Lane::Live, vec![deleted_for_you_on_your_phone(&id)]).await.unwrap();
    assert!(row(&running, ANA, &id).await.is_none());
    running.connection(Online);
    until_stored(&running).await;
    assert_eq!(kinds_sent(&running), ["chego às 20h"], "only the message went, not its edit");
    let said = notices(&mut pushes, Duration::from_millis(500)).await;
    assert_eq!(said, ["Your edit wasn't sent: the message was deleted for everyone"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_on_its_way_to_a_message_only_deleted_for_you_still_goes() {
    use jaw_proto::ConnectionState::Online;
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.arrives(ANA, ANA, "m1", now_ms() - 60_000).await;
    running.call("react", json!({ "message": "m1", "emoji": "❤️" })).await.unwrap();
    let mut pushes = running.pushes.subscribe();
    running.call("delete", json!({ "message": "m1", "for": "me" })).await.unwrap();
    running.connection(Online);
    until_stored(&running).await;
    assert_eq!(kinds_sent(&running), ["a reaction"]);
    assert!(notices(&mut pushes, Duration::from_millis(500)).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_to_a_message_youre_deleting_for_everyone_is_refused_while_your_deletion_is_on_its_way() {
    let home = tempfile::tempdir().unwrap();
    let timing = Timing { unconfirmed_for: Duration::from_secs(1), ..quick() };
    let running = Running::timed(home.path(), world(), true, true, timing).await;
    running.chats(&[ANA]).await;
    let id = running.send(ANA, "chego às 20h").await;
    running.until_status(ANA, &id, "sent").await;
    // The deletion's first try fails on the way, and its next is refused when it may have gone.
    running.world.fail_chats(&[ANA.to_string()]);
    running.call("delete", json!({ "message": id, "for": "everyone" })).await.unwrap();
    let react = || running.call("react", json!({ "message": id, "emoji": "👍" }));
    let refused = react().await.unwrap_err();
    assert_eq!(refused.code, code::NOT_ALLOWED, "{}", refused.message);
    assert_eq!(outbox_rows(&running).await.0, [("revoke".to_string(), "queued".to_string())], "nothing written");
    running.world.until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::Send(_))).count() >= 2).await;
    eventually(&running, "the deletion's first try failed on the way", |db: &Connection| {
        Ok(db.query_row("SELECT attempts FROM outbox WHERE kind = 'revoke'", [], |r| r.get::<_, i64>(0))? > 0)
    })
    .await;
    running.world.refuse_next(1);
    running.world.stop_failing_chats();
    let revoke_is = |state: &'static str| {
        move |db: &Connection| {
            Ok(db.query_row("SELECT state FROM outbox WHERE kind = 'revoke'", [], |r| r.get::<_, String>(0))? == state)
        }
    };
    eventually(&running, "the deletion waiting for a receipt", revoke_is("unconfirmed")).await;
    assert_eq!(react().await.unwrap_err().code, code::NOT_ALLOWED, "while it waits for a receipt");
    // No receipt comes in its time, so it's kept for a late one, and the message shows as it was.
    eventually(&running, "the deletion kept for a late receipt", revoke_is("failed")).await;
    assert!(!row(&running, ANA, &id).await.unwrap().deleted);
    react().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_of_a_message_youre_deleting_for_everyone_is_refused_while_your_deletion_is_on_its_way() {
    use jaw_proto::ConnectionState::Online;
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    let id = running.send(ANA, "chego às 20h").await;
    running.until_status(ANA, &id, "sent").await;
    until_offline(&running).await;
    running.call("delete", json!({ "message": id, "for": "everyone" })).await.unwrap();
    let refused = running.call("edit", json!({ "message": id, "text": "chego às 21h" })).await.unwrap_err();
    assert_eq!(refused.code, code::NOT_ALLOWED, "{}", refused.message);
    assert_eq!(outbox_rows(&running).await.0, [("revoke".to_string(), "queued".to_string())], "nothing written");
    // Once WhatsApp refuses the deletion, the message shows as it was, and can be edited again.
    running.world.refuse_next(1);
    running.connection(Online);
    until_stored(&running).await;
    running.call("edit", json!({ "message": id, "text": "chego às 21h" })).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_or_an_edit_named_in_its_chat_or_not_is_refused_while_your_deletion_is_on_its_way() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.chats(&[ANA]).await;
    let message = Message { chat: ANA.into(), ..mine_message("m1", now_ms() - 60_000) };
    running.store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
    running.call("delete", json!({ "message": "m1", "chat": ANA, "for": "everyone" })).await.unwrap();
    for chat in [Some(ANA), None] {
        let named = |mut params: Value| {
            if let Some(chat) = chat {
                params["chat"] = json!(chat);
            }
            params
        };
        let reacted = running.call("react", named(json!({ "message": "m1", "emoji": "👍" }))).await.unwrap_err();
        let edited = running.call("edit", named(json!({ "message": "m1", "text": "chego às 21h" }))).await.unwrap_err();
        assert_eq!((reacted.code, edited.code), (code::NOT_ALLOWED, code::NOT_ALLOWED), "named in {chat:?}");
    }
    assert_eq!(outbox_rows(&running).await.0, [("revoke".to_string(), "queued".to_string())], "nothing written");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_or_an_edit_whose_message_is_deleted_for_everyone_once_it_has_found_it_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.chats(&[ANA]).await;
    for id in ["m1", "m2"] {
        let message = Message { chat: ANA.into(), ..mine_message(id, now_ms() - 60_000) };
        running.store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
    }
    running.arrives(ANA, ANA, "a1", now_ms() - 60_000).await;
    // Each action finds its message as it was, and its message is then deleted for everyone before
    // its step: by you, from another interface at the same moment, or by Ana.
    let asked = [
        ("react", json!({ "message": "m1", "chat": ANA, "emoji": "👍" }), true),
        ("edit", json!({ "message": "m2", "chat": ANA, "text": "chego às 21h" }), true),
        ("react", json!({ "message": "a1", "chat": ANA, "emoji": "👍" }), false),
    ];
    for (method, params, yours) in asked {
        let id = params["message"].as_str().unwrap().to_string();
        let (stopped, go) = running.sending.pause_next_action();
        let sending = running.sending.clone();
        let asking = tokio::spawn(async move { sending.call(method, params).await });
        stopped.notified().await;
        if yours {
            running.call("delete", json!({ "message": id, "chat": ANA, "for": "everyone" })).await.unwrap();
        } else {
            let hers = Amend { what: Amendment::Revoke, ..other_of(ANA, &id) };
            running.store.apply(Lane::Live, vec![Change::Amend(hers)]).await.unwrap();
        }
        go.notify_one();
        let refused = asking.await.unwrap().unwrap_err();
        assert_eq!(refused.code, code::NOT_ALLOWED, "{method} {id}: {}", refused.message);
    }
    let queued = ("revoke".to_string(), "queued".to_string());
    assert_eq!(outbox_rows(&running).await.0, [queued.clone(), queued], "nothing written behind your deletions");
}

/// A reaction stopped between finding your message and its step while your deletion of it for
/// everyone commits and reaches `state`: what the step answers, and the outbox after.
async fn reaction_stopped_while_your_deletion_is(state: &str) -> (Result<Value, Error>, Vec<(String, String)>) {
    let home = tempfile::tempdir().unwrap();
    let timing = Timing { unconfirmed_for: Duration::from_secs(1), ..quick() };
    let running = Running::timed(home.path(), world(), true, true, timing).await;
    running.chats(&[ANA]).await;
    let id = running.send(ANA, "chego às 20h").await;
    running.until_status(ANA, &id, "sent").await;
    let (stopped, go) = running.sending.pause_next_action();
    let sending = running.sending.clone();
    let params = json!({ "message": id, "chat": ANA, "emoji": "👍" });
    let asking = tokio::spawn(async move { sending.call("react", params).await });
    stopped.notified().await;
    let revoke_is = |state: &'static str| {
        move |db: &Connection| {
            let now: Option<String> =
                db.query_row("SELECT state FROM outbox WHERE kind = 'revoke'", [], |r| r.get(0)).optional()?;
            Ok(now.as_deref() == Some(state))
        }
    };
    let delete = || running.call("delete", json!({ "message": id, "chat": ANA, "for": "everyone" }));
    match state {
        "queued" => {
            until_offline(&running).await;
            delete().await.unwrap();
        }
        "handed to the engine" => {
            running.world.hold(true);
            delete().await.unwrap();
            running
                .world
                .until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::Send(_))).count() >= 2)
                .await;
        }
        "waiting for a receipt" | "kept for a late receipt" => {
            // The first try fails on the way, and the next is refused when it may have gone.
            running.world.fail_chats(&[ANA.to_string()]);
            delete().await.unwrap();
            running
                .world
                .until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::Send(_))).count() >= 2)
                .await;
            eventually(&running, "the deletion's first try failed on the way", |db: &Connection| {
                Ok(db.query_row("SELECT attempts FROM outbox WHERE kind = 'revoke'", [], |r| r.get::<_, i64>(0))? > 0)
            })
            .await;
            running.world.refuse_next(1);
            running.world.stop_failing_chats();
            eventually(&running, "the deletion waiting for a receipt", revoke_is("unconfirmed")).await;
            if state == "kept for a late receipt" {
                eventually(&running, "the deletion kept for a late receipt", revoke_is("failed")).await;
            }
        }
        "done" => {
            delete().await.unwrap();
            until_stored(&running).await;
        }
        "refused" => {
            running.world.refuse_next(1);
            delete().await.unwrap();
            until_stored(&running).await;
        }
        other => panic!("no state {other}"),
    }
    go.notify_one();
    let answered = asking.await.unwrap();
    (answered, outbox_rows(&running).await.0)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_step_refuses_a_reaction_in_each_state_of_your_deletion_for_everyone_that_windows_show_as_deleted() {
    let states = [
        ("queued", true),
        ("handed to the engine", true),
        ("waiting for a receipt", true),
        ("kept for a late receipt", false),
        ("done", true),
        ("refused", false),
    ];
    for (state, refused) in states {
        let (answered, rows) = reaction_stopped_while_your_deletion_is(state).await;
        let written = rows.iter().any(|(kind, _)| kind == "reaction");
        match answered {
            Err(e) => assert!(refused && e.code == code::NOT_ALLOWED && !written, "{state}: {} {rows:?}", e.message),
            Ok(_) => assert!(!refused && written, "{state}: taken, {rows:?}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_or_an_edit_in_a_chat_merging_away_is_refused_while_the_copy_kept_is_being_deleted() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    let lid = "100000000001@lid";
    // Your message is in both of Ana's chats, and her LID's is the older, so an action that names
    // no chat finds its copy there first.
    running.chats(&[lid, ANA]).await;
    for chat in [lid, ANA] {
        let message = Message { chat: chat.into(), ..mine_message("m1", now_ms() - 60_000) };
        running.store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
    }
    running.call("delete", json!({ "message": "m1", "chat": ANA, "for": "everyone" })).await.unwrap();
    // Her LID and number turn out to be one person's, so her LID's chat starts merging into her
    // number's, whose copy your deletion is aimed at; the copy in her LID's chat shows as it was.
    let both = Person { jids: vec![ANA.into(), lid.into()], ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(both)]).await.unwrap();
    let copies: i64 =
        running.query(|db| Ok(db.query_row("SELECT count(*) FROM messages WHERE id = 'm1'", [], |r| r.get(0))?)).await;
    assert_eq!(copies, 2, "the merge is still to move the copy in her LID's chat");
    for chat in [Some(lid), None] {
        let named = |mut params: Value| {
            if let Some(chat) = chat {
                params["chat"] = json!(chat);
            }
            params
        };
        let reacted = running.call("react", named(json!({ "message": "m1", "emoji": "👍" }))).await.unwrap_err();
        let edited = running.call("edit", named(json!({ "message": "m1", "text": "chego às 21h" }))).await.unwrap_err();
        assert_eq!((reacted.code, edited.code), (code::NOT_ALLOWED, code::NOT_ALLOWED), "named in {chat:?}");
    }
    assert_eq!(outbox_rows(&running).await.0, [("revoke".to_string(), "queued".to_string())], "nothing written");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_or_an_edit_in_a_chat_merging_away_is_refused_while_its_copy_there_is_being_deleted() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    let lid = "100000000001@lid";
    // Your message is in both of Ana's chats, her LID's the older, so an action that names no
    // chat finds its copy there first; you delete it for everyone in her LID's chat.
    running.chats(&[lid, ANA]).await;
    for chat in [lid, ANA] {
        let message = Message { chat: chat.into(), ..mine_message("m1", now_ms() - 60_000) };
        running.store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
    }
    running.call("delete", json!({ "message": "m1", "chat": lid, "for": "everyone" })).await.unwrap();
    // Her LID's chat then starts merging into her number's: your deletion is aimed at the copy
    // merging away, and the copy kept shows as it was.
    let both = Person { jids: vec![ANA.into(), lid.into()], ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(both)]).await.unwrap();
    for chat in [Some(lid), None] {
        let named = |mut params: Value| {
            if let Some(chat) = chat {
                params["chat"] = json!(chat);
            }
            params
        };
        let reacted = running.call("react", named(json!({ "message": "m1", "emoji": "👍" }))).await.unwrap_err();
        let edited = running.call("edit", named(json!({ "message": "m1", "text": "chego às 21h" }))).await.unwrap_err();
        assert_eq!((reacted.code, edited.code), (code::NOT_ALLOWED, code::NOT_ALLOWED), "named in {chat:?}");
    }
    let copies: i64 =
        running.query(|db| Ok(db.query_row("SELECT count(*) FROM messages WHERE id = 'm1'", [], |r| r.get(0))?)).await;
    assert_eq!(copies, 2, "the merge is still to move the copy in her LID's chat");
    assert_eq!(outbox_rows(&running).await.0, [("revoke".to_string(), "queued".to_string())], "nothing written");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_or_an_edit_in_the_chat_kept_is_refused_while_the_copy_merging_in_is_being_deleted() {
    use jaw_proto::ConnectionState::Online;
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    let lid = "100000000001@lid";
    // Your message is in both of Ana's chats, her number's the older, so an action that names no
    // chat finds its copy there first; you delete it for everyone in her LID's chat.
    running.chats(&[ANA, lid]).await;
    for chat in [ANA, lid] {
        let message = Message { chat: chat.into(), ..mine_message("m1", now_ms() - 60_000) };
        running.store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
    }
    running.call("delete", json!({ "message": "m1", "chat": lid, "for": "everyone" })).await.unwrap();
    // Her LID's chat then starts merging into her number's: the copy kept shows as it was, and your
    // deletion is aimed at the copy the merge is to fold into it.
    let both = Person { jids: vec![ANA.into(), lid.into()], ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(both)]).await.unwrap();
    for chat in [Some(ANA), None] {
        let named = |mut params: Value| {
            if let Some(chat) = chat {
                params["chat"] = json!(chat);
            }
            params
        };
        let reacted = running.call("react", named(json!({ "message": "m1", "emoji": "👍" }))).await.unwrap_err();
        let edited = running.call("edit", named(json!({ "message": "m1", "text": "chego às 21h" }))).await.unwrap_err();
        assert_eq!((reacted.code, edited.code), (code::NOT_ALLOWED, code::NOT_ALLOWED), "named in {chat:?}");
    }
    let copies: i64 =
        running.query(|db| Ok(db.query_row("SELECT count(*) FROM messages WHERE id = 'm1'", [], |r| r.get(0))?)).await;
    assert_eq!(copies, 2, "the merge is still to move the copy in her LID's chat");
    assert_eq!(outbox_rows(&running).await.0, [("revoke".to_string(), "queued".to_string())], "nothing written");
    // Once WhatsApp can be reached, only the deletion goes.
    running.connection(Online);
    until_stored(&running).await;
    assert_eq!(kinds_sent(&running), ["a deletion"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_in_the_chat_kept_is_refused_once_ana_has_deleted_the_copy_merging_in() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    let lid = "100000000001@lid";
    // Ana's message is in both of her chats, her number's the older, so an action that names no
    // chat finds its copy there first; her deletion for everyone lands on the copy in her LID's.
    running.chats(&[ANA, lid]).await;
    running.arrives(ANA, ANA, "a1", now_ms() - 60_000).await;
    running.arrives(lid, lid, "a1", now_ms() - 60_000).await;
    let hers = Amend { what: Amendment::Revoke, ..other_of(lid, "a1") };
    running.store.apply(Lane::Live, vec![Change::Amend(hers)]).await.unwrap();
    // Her LID's chat then starts merging into her number's, whose copy shows as it was.
    let both = Person { jids: vec![ANA.into(), lid.into()], ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(both)]).await.unwrap();
    for chat in [Some(ANA), None] {
        let mut params = json!({ "message": "a1", "emoji": "👍" });
        if let Some(chat) = chat {
            params["chat"] = json!(chat);
        }
        let refused = running.call("react", params).await.unwrap_err();
        assert_eq!(refused.code, code::NOT_ALLOWED, "named in {chat:?}: {}", refused.message);
    }
    let copies: i64 =
        running.query(|db| Ok(db.query_row("SELECT count(*) FROM messages WHERE id = 'a1'", [], |r| r.get(0))?)).await;
    assert_eq!(copies, 2, "the merge is still to move the copy in her LID's chat");
    assert!(outbox_rows(&running).await.0.is_empty(), "nothing written");
}

/// A reaction named in Ana's number's chat while her LID's merges into it, your deletion for
/// everyone of the copy in her LID's chat having reached `state` before the merge began: what the
/// step answers, and the outbox after.
async fn reaction_in_the_chat_kept_while_your_deletion_of_the_copy_merging_in_is(
    state: &str,
) -> (Result<Value, Error>, Vec<(String, String)>) {
    let home = tempfile::tempdir().unwrap();
    let unconfirmed_for = Duration::from_secs(if state == "kept for a late receipt" { 1 } else { 3_600 });
    // No receipt comes, so nothing the engine delivers moves the merge on.
    let world: Arc<World> = Arc::default();
    world.replying(Replies { delivered: Duration::from_secs(3_600), read: Duration::from_secs(3_600) });
    let running = Running::timed(home.path(), world, true, true, Timing { unconfirmed_for, ..quick() }).await;
    let lid = "100000000001@lid";
    running.chats(&[ANA, lid]).await;
    for chat in [ANA, lid] {
        let message = Message { chat: chat.into(), ..mine_message("m1", now_ms() - 60_000) };
        running.store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
    }
    let revoke_is = |state: &'static str| {
        move |db: &Connection| {
            let now: Option<String> =
                db.query_row("SELECT state FROM outbox WHERE kind = 'revoke'", [], |r| r.get(0)).optional()?;
            Ok(now.as_deref() == Some(state))
        }
    };
    let delete = || running.call("delete", json!({ "message": "m1", "chat": lid, "for": "everyone" }));
    match state {
        "queued" => {
            until_offline(&running).await;
            delete().await.unwrap();
        }
        "waiting for a receipt" | "kept for a late receipt" => {
            // The first try fails on the way, and the next is refused when it may have gone.
            running.world.fail_chats(&[lid.to_string()]);
            delete().await.unwrap();
            running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::Send(_)))).await;
            eventually(&running, "the deletion's first try failed on the way", |db: &Connection| {
                Ok(db.query_row("SELECT attempts FROM outbox WHERE kind = 'revoke'", [], |r| r.get::<_, i64>(0))? > 0)
            })
            .await;
            running.world.refuse_next(1);
            running.world.stop_failing_chats();
            eventually(&running, "the deletion waiting for a receipt", revoke_is("unconfirmed")).await;
            if state == "kept for a late receipt" {
                eventually(&running, "the deletion kept for a late receipt", revoke_is("failed")).await;
            }
        }
        "done" => {
            delete().await.unwrap();
            until_stored(&running).await;
        }
        "refused" => {
            running.world.refuse_next(1);
            delete().await.unwrap();
            until_stored(&running).await;
        }
        "a reaction, not a deletion" => {
            until_offline(&running).await;
            running.call("react", json!({ "message": "m1", "chat": lid, "emoji": "❤️" })).await.unwrap();
        }
        other => panic!("no state {other}"),
    }
    // With WhatsApp out of reach from here, a reaction taken stays in the outbox.
    until_offline(&running).await;
    let both = Person { jids: vec![ANA.into(), lid.into()], ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(both)]).await.unwrap();
    let answered = running.call("react", json!({ "message": "m1", "chat": ANA, "emoji": "👍" })).await;
    let copies: i64 =
        running.query(|db| Ok(db.query_row("SELECT count(*) FROM messages WHERE id = 'm1'", [], |r| r.get(0))?)).await;
    assert_eq!(copies, 2, "{state}: the merge is still to move the copy in her LID's chat");
    (answered, outbox_rows(&running).await.0)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_step_reads_the_copy_merging_in_as_windows_show_it_in_each_state_of_your_deletion_of_it() {
    let states = [
        ("queued", true),
        ("waiting for a receipt", true),
        ("kept for a late receipt", false),
        ("done", true),
        ("refused", false),
        ("a reaction, not a deletion", false),
    ];
    for (state, refused) in states {
        let (answered, rows) = reaction_in_the_chat_kept_while_your_deletion_of_the_copy_merging_in_is(state).await;
        let before = usize::from(state == "a reaction, not a deletion");
        let written = rows.iter().filter(|(kind, _)| kind == "reaction").count() > before;
        match answered {
            Err(e) => assert!(refused && e.code == code::NOT_ALLOWED && !written, "{state}: {} {rows:?}", e.message),
            Ok(_) => assert!(!refused && written, "{state}: taken, {rows:?}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_or_an_edit_in_the_chat_kept_is_taken_while_other_messages_in_the_chat_merging_in_are_deleted() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    let lid = "100000000001@lid";
    // Your message is in both of Ana's chats, her number's the older. In her LID's chat, another
    // message of yours is being deleted for everyone, and Ana has deleted one of hers.
    running.chats(&[ANA, lid]).await;
    for chat in [ANA, lid] {
        let message = Message { chat: chat.into(), ..mine_message("m1", now_ms() - 60_000) };
        running.store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
    }
    let other = Message { chat: lid.into(), ..mine_message("m2", now_ms() - 60_000) };
    running.store.apply(Lane::Live, vec![Change::Message(other)]).await.unwrap();
    running.arrives(lid, lid, "a1", now_ms() - 60_000).await;
    running.call("delete", json!({ "message": "m2", "chat": lid, "for": "everyone" })).await.unwrap();
    let hers = Amend { what: Amendment::Revoke, ..other_of(lid, "a1") };
    running.store.apply(Lane::Live, vec![Change::Amend(hers)]).await.unwrap();
    // Her LID's chat then starts merging into her number's: the copies of m1 show as they were.
    let both = Person { jids: vec![ANA.into(), lid.into()], ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(both)]).await.unwrap();
    for chat in [Some(ANA), None] {
        let named = |mut params: Value| {
            if let Some(chat) = chat {
                params["chat"] = json!(chat);
            }
            params
        };
        running.call("react", named(json!({ "message": "m1", "emoji": "👍" }))).await.unwrap();
        running.call("edit", named(json!({ "message": "m1", "text": "chego às 21h" }))).await.unwrap();
    }
    let copies: i64 =
        running.query(|db| Ok(db.query_row("SELECT count(*) FROM messages WHERE id = 'm1'", [], |r| r.get(0))?)).await;
    assert_eq!(copies, 2, "the merge is still to move the copy in her LID's chat");
    let kinds: Vec<String> = outbox_rows(&running).await.0.into_iter().map(|(kind, _)| kind).collect();
    assert_eq!(kinds, ["revoke", "reaction", "edit", "reaction", "edit"], "taken each way");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_or_an_edit_named_in_any_chat_of_a_chain_of_merges_is_refused_while_your_deletion_is_on_its_way() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    let (first, second) = ("100000000001@lid", "100000000002@lid");
    // Ana's number's chat, the oldest, and her two LIDs' chats; your m1, m2 and m3 are in her
    // second LID's chat and her number's.
    running.chats(&[ANA, first, second]).await;
    for chat in [second, ANA] {
        for id in ["m1", "m2", "m3"] {
            let message = Message { chat: chat.into(), ..mine_message(id, now_ms() - 60_000) };
            running.store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
        }
    }
    // You delete m1 for everyone in her second LID's chat; a reaction to m2 and an edit of m3, named
    // there, find their message before you delete it there too.
    let delete = |id: &str| running.call("delete", json!({ "message": id, "chat": second, "for": "everyone" }));
    delete("m1").await.unwrap();
    let mut stopped_after_finding = Vec::new();
    for (method, params) in [
        ("react", json!({ "message": "m2", "chat": second, "emoji": "👍" })),
        ("edit", json!({ "message": "m3", "chat": second, "text": "chego às 21h" })),
    ] {
        let id = params["message"].as_str().unwrap().to_string();
        let (stopped, go) = running.sending.pause_next_action();
        let sending = running.sending.clone();
        stopped_after_finding.push((method, go, tokio::spawn(async move { sending.call(method, params).await })));
        stopped.notified().await;
        delete(&id).await.unwrap();
    }
    // Her two LIDs turn out to be one person's, so her second's chat starts merging into her
    // first's, the older; then her number turns out to be hers too, and both go into its chat.
    for jids in [vec![first, second], vec![ANA, first, second]] {
        let person = Person { jids: jids.into_iter().map(String::from).collect(), ..Person::default() };
        running.store.apply(Lane::Live, vec![Change::Person(person)]).await.unwrap();
    }
    let merging: Vec<(String, String)> = running
        .query(|db| {
            Ok(db
                .prepare(
                    "SELECT g.id, k.id FROM chat_merges m JOIN chats g ON g.n = m.gone JOIN chats k ON k.n = m.keep
                    ORDER BY g.id",
                )?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await;
    assert_eq!(merging, [(first.to_string(), ANA.to_string()), (second.to_string(), ANA.to_string())]);
    let by_the_step = "it's deleted for everyone, or being deleted, so it can't be reacted to, edited or voted in";
    for (method, go, asking) in stopped_after_finding {
        go.notify_one();
        let refused = asking.await.unwrap().unwrap_err();
        assert_eq!((refused.code, refused.message.as_str()), (code::NOT_ALLOWED, by_the_step), "{method}");
    }
    // Asked now, in her second LID's chat the reader finds your deletion itself; in the other two
    // it finds her number's copy, and the step refuses.
    let found = "it's being deleted for everyone, so it can't be";
    for (chat, react, edit) in [
        (second, format!("{found} reacted to"), format!("{found} edited")),
        (first, by_the_step.to_string(), by_the_step.to_string()),
        (ANA, by_the_step.to_string(), by_the_step.to_string()),
    ] {
        let (reacting, editing) = (
            json!({ "message": "m1", "chat": chat, "emoji": "👍" }),
            json!({ "message": "m1", "chat": chat, "text": "chego às 21h" }),
        );
        let reacted = running.call("react", reacting).await.unwrap_err();
        let edited = running.call("edit", editing).await.unwrap_err();
        assert_eq!((reacted.code, reacted.message), (code::NOT_ALLOWED, react), "react named in {chat}");
        assert_eq!((edited.code, edited.message), (code::NOT_ALLOWED, edit), "edit named in {chat}");
    }
    let copies: i64 =
        running.query(|db| Ok(db.query_row("SELECT count(*) FROM messages WHERE id = 'm1'", [], |r| r.get(0))?)).await;
    assert_eq!(copies, 2, "the merges are still to move the copy in her second LID's chat");
    let queued = ("revoke".to_string(), "queued".to_string());
    assert_eq!(outbox_rows(&running).await.0, [queued.clone(), queued.clone(), queued], "nothing written");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_action_on_a_message_still_to_go_from_a_chat_being_merged_goes_after_it() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let lid = "100000000001@lid";
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.chats(&[ANA, lid]).await;
    running.world.gate(true);
    running.send(lid, "um").await;
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::Send(_)))).await;
    // Unanswered, "um" holds its chat's queue, and "dois" waits behind it.
    let dois = running.send(lid, "dois").await;
    // Ana's LID and number turn out to be one person: her LID's chat starts merging into her number's,
    // where the edit is queued, apart from "dois".
    let both = Person { jids: vec![ANA.into(), lid.into()], ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(both)]).await.unwrap();
    running.call("edit", json!({ "message": dois, "text": "dois!" })).await.unwrap();
    until_held(&running).await;
    running.world.gate(false);
    until_stored(&running).await;
    assert_eq!(kinds_sent(&running), ["um", "dois", "an edit"], "the edit goes after its message");
    assert_eq!(stored_text(&running, &dois).await, "dois!");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_of_a_message_waiting_for_a_receipt_goes_once_one_says_it_went() {
    let home = tempfile::tempdir().unwrap();
    let id = crash(home.path(), "held").await;
    let world = world();
    world.refuse_next(1);
    let running = Running::on(home.path(), world, true).await;
    let unconfirmed = |db: &Connection| {
        Ok(db.query_row("SELECT count(*) FROM outbox WHERE state = 'unconfirmed'", [], |r| r.get::<_, i64>(0))? == 1)
    };
    eventually(&running, "the resend's refusal", unconfirmed).await;
    running.call("edit", json!({ "message": id, "text": "did it go??" })).await.unwrap();
    until_held(&running).await;
    assert_eq!(kinds_sent(&running), ["did it go?"], "the edit waits while its message may not have gone");
    running.receipt(ANA, &id, ANA, "delivered").await;
    until_stored(&running).await;
    assert_eq!(kinds_sent(&running), ["did it go?", "an edit"], "and goes once a receipt says it did");
    assert_eq!(stored_text(&running, &id).await, "did it go??");
}

/// The chat's unread, read position and how far WhatsApp was told, as jaw.db holds them.
async fn read_state(running: &Running, chat: &'static str) -> (i64, Option<i64>, Option<i64>, Option<i64>) {
    running
        .query(move |db| {
            Ok(db.query_row("SELECT unread, read_at, read_n, told_at FROM chats WHERE id = ?1", [chat], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?)
        })
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forward_reads_none_of_the_chats_it_goes_to() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA, RUI]).await;
    running.arrives(ANA, ANA, "m1", now_ms() - 60_000).await;
    running.arrives(RUI, RUI, "r1", now_ms() - 50_000).await;
    running.arrives(RUI, RUI, "r2", now_ms() - 40_000).await;
    let before = read_state(&running, RUI).await;
    assert_eq!(before.0, 2);
    let forwarded = running.call("forward", json!({ "messages": ["m1"], "to": [RUI] })).await.unwrap();
    assert_eq!(read_state(&running, RUI).await, before, "its unread, its read position and what WhatsApp was told");
    let id = forwarded["messages"][0].as_str().unwrap().to_string();
    running.until_status(RUI, &id, "sent").await;
    running.call("read", json!({ "chat": RUI, "up_to": "r1" })).await.unwrap();
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::MarkRead(_)))).await;
    assert_eq!(running.marked_read()[0].ids, ["r1"], "WhatsApp is told only of what you read");
    assert_eq!(read_state(&running, RUI).await.0, 1);
}

/// A text message from someone, as WhatsApp sent it, with its raw protobuf.
async fn text_from(running: &Running, chat: &str, id: &str, text: &str) {
    use wa_proto::wa_web_protobufs_web::WebMessageInfo;
    let info = WebMessageInfo {
        key: wa_proto::wa_common::MessageKey {
            remote_jid: Some(chat.into()),
            from_me: Some(false),
            id: Some(id.into()),
            participant: None,
        },
        message: Some(wa_proto::wa_web_protobufs_e2e::Message {
            conversation: Some(text.into()),
            ..Default::default()
        }),
        message_timestamp: Some((now_ms() / 1000 - 120) as u64),
        ..Default::default()
    };
    let changes = crate::convert::message(&info, &crate::convert::Me::default(), true);
    running.store.apply(Lane::Live, changes).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forward_and_a_reply_of_an_edited_message_say_what_it_says_now() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.chats(&[ANA, RUI]).await;
    text_from(&running, ANA, "m1", "chego às 20h").await;
    photo_from(&running, ANA, "p1", 0).await;
    let edited = |target: &str, text: &str| {
        let edit = Amendment::Edit { text: Some(text.into()), spans: None, summary: text.into() };
        Change::Amend(Amend { what: edit, ..other_of(ANA, target) })
    };
    let edits = vec![edited("m1", "chego às 21h"), edited("p1", "o lago ao fim da tarde")];
    running.store.apply(Lane::Live, edits).await.unwrap();
    let forwarded = running.call("forward", json!({ "messages": ["m1", "p1"], "to": [RUI] })).await.unwrap();
    let reply = running.call("send.text", json!({ "chat": ANA, "text": "ok", "reply": "m1" })).await.unwrap();
    sent(&running, 3).await;
    let raws = raws(&running);
    let forwarded_text = raws.iter().find_map(|m| {
        m.extended_text_message.as_ref().filter(|e| e.context_info.as_ref().is_some_and(|c| c.quoted_message.is_none()))
    });
    assert_eq!(
        forwarded_text.and_then(|e| e.text.as_deref()),
        Some("chego às 21h"),
        "the forward says what it says now"
    );
    let photo = raws.iter().find_map(|m| m.image_message.as_deref()).expect("the photo");
    assert_eq!(
        (photo.caption.as_deref(), photo.url.as_deref()),
        (Some("o lago ao fim da tarde"), Some("https://mmg.whatsapp.net/photo")),
        "with its caption as edited and its media as it came"
    );
    let quoted = raws.iter().find_map(|m| {
        m.extended_text_message.as_ref()?.context_info.as_ref()?.quoted_message.as_ref()?.conversation.clone()
    });
    assert_eq!(quoted.as_deref(), Some("chego às 21h"), "and the reply quotes what it says now");
    let ids: Vec<String> = serde_json::from_value(forwarded["messages"].clone()).unwrap();
    assert_eq!(text_of(&row(&running, RUI, &ids[0]).await.unwrap()), "chego às 21h", "as jaw's own rows say");
    assert_eq!(reply["row"]["reply"]["summary"], "chego às 21h");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_is_sent_the_message_again_as_an_action_on_it_is_queued_done_and_refused() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    // No receipts come, so nothing else changes the message meanwhile.
    running.world.replying(Replies { delivered: Duration::from_secs(3_600), read: Duration::from_secs(3_600) });
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.arrives(ANA, ANA, "m1", now_ms() - 60_000).await;
    let socket = serve_watched(&running, home.path());
    let mut window = window(&socket, ANA);
    let first = shown(&mut window, "m1", "as it is", |_| true).await;
    let version = |row: &Value| row["v"].as_i64().unwrap();
    let thumbs = json!([{ "emoji": "👍", "n": 1, "mine": true }]);
    running.world.gate(true);
    running.call("react", json!({ "message": "m1", "emoji": "👍" })).await.unwrap();
    let queued = shown(&mut window, "m1", "with the reaction on its way", |row| row["reactions"] == thumbs).await;
    assert!(version(&queued) > version(&first));
    running.world.gate(false);
    let done = shown(&mut window, "m1", "once it's done", |row| version(row) > version(&queued)).await;
    assert_eq!(done["reactions"], thumbs);
    until_stored(&running).await;
    // WhatsApp can't be reached, so the next waits until it's seen on its way, and is refused then.
    until_offline(&running).await;
    running.call("react", json!({ "message": "m1", "emoji": "😂" })).await.unwrap();
    let laughing = json!([{ "emoji": "😂", "n": 1, "mine": true }]);
    let second = shown(&mut window, "m1", "with the next on its way", |row| row["reactions"] == laughing).await;
    running.world.refuse_next(1);
    running.connection(jaw_proto::ConnectionState::Online);
    let refused = shown(&mut window, "m1", "as it was once that's refused", |row| row["reactions"] == thumbs).await;
    assert!(version(&refused) > version(&second));
}

#[tokio::test(flavor = "multi_thread")]
async fn past_a_thousand_waiting_an_action_or_a_forward_is_refused_and_nothing_is_written() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.chats(&[ANA]).await;
    running.arrives(ANA, ANA, "m1", now_ms() - 60_000).await;
    let mine = Message { chat: ANA.into(), live: true, ..mine_message("mine", now_ms() - 60_000) };
    running.store.apply(Lane::Live, vec![Change::Message(mine)]).await.unwrap();
    {
        let mut db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        let tx = db.transaction().unwrap();
        for n in 0..1_000i64 {
            tx.execute(
                "INSERT INTO outbox (jid, id, kind, raw, next_at, created_at) VALUES (?1, ?2, 'reaction', x'', 0, 0)",
                rusqlite::params![ANA, format!("w{n}")],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let counts = || {
        running.query(|db| {
            let count =
                |table: &str| db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get::<_, i64>(0));
            Ok((count("outbox")?, count("messages")?, count("tombstones")?))
        })
    };
    let before = counts().await;
    let actions = [
        ("react", json!({ "message": "m1", "emoji": "👍" })),
        ("edit", json!({ "message": "mine", "text": "outra coisa" })),
        ("delete", json!({ "message": "mine", "for": "everyone" })),
        ("delete", json!({ "message": "m1", "for": "me" })),
        ("forward", json!({ "messages": ["m1"], "to": [ANA] })),
    ];
    for (method, params) in actions {
        let refused = running.call(method, params.clone()).await.unwrap_err();
        assert_eq!(
            (refused.code, refused.data.clone()),
            (code::LIMIT, Some(json!({ "limit": "outbox", "waiting": 1_000, "failed": 0 }))),
            "{method} {params}"
        );
        assert_eq!(counts().await, before, "{method} {params} wrote nothing");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_or_an_edit_asked_again_with_its_key_goes_once_even_asked_twice_at_once() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.chats(&[ANA]).await;
    running.arrives(ANA, ANA, "m1", now_ms() - 60_000).await;
    let id = running.send(ANA, "chego às 20h").await;
    running.until_status(ANA, &id, "sent").await;
    let react = |emoji: &str, key: &str| json!({ "message": "m1", "emoji": emoji, "key": key });
    let edit = |text: &str, key: &str| json!({ "message": id, "text": text, "key": key });
    for _ in 0..2 {
        assert_eq!(running.call("react", react("👍", "jaw-react-1")).await.unwrap(), json!({}));
        assert_eq!(running.call("edit", edit("chego às 21h", "jaw-edit-1")).await.unwrap(), json!({}));
    }
    // Asked twice at once while jaw.db can't be written, so each finds its key names nothing yet,
    // and only the writer, taking them one at a time, sees the first.
    let holder = Connection::open(home.path().join("jaw.db")).unwrap();
    holder.busy_timeout(Duration::from_secs(5)).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let release = tokio::task::spawn_blocking(move || {
        std::thread::sleep(Duration::from_millis(300));
        holder.execute_batch("COMMIT").unwrap();
    });
    let (one, two, three, four) = tokio::join!(
        running.call("react", react("😂", "jaw-react-2")),
        running.call("react", react("😂", "jaw-react-2")),
        running.call("edit", edit("chego às 22h", "jaw-edit-2")),
        running.call("edit", edit("chego às 22h", "jaw-edit-2")),
    );
    release.await.unwrap();
    for answer in [one, two, three, four] {
        assert_eq!(answer.unwrap(), json!({}));
    }
    until_stored(&running).await;
    let sent = kinds_sent(&running);
    let each = |kind: &str| sent.iter().filter(|sent| *sent == kind).count();
    assert_eq!((sent.len(), each("a reaction"), each("an edit")), (5, 2, 2), "one of each for each key: {sent:?}");
    assert_eq!(stored_text(&running, &id).await, "chego às 22h");
}

/// Waits until the outbox is empty: everything sent has been answered and kept. It looks again
/// and again rather than at each commit, since what empties it may write no event, as an app
/// state patch's answer doesn't.
async fn until_stored(running: &Running) {
    eventually(running, "the outbox to empty", |db| {
        Ok(db.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))? == 0)
    })
    .await;
}

/// Waits until an action is held for its message to reach WhatsApp.
async fn until_held(running: &Running) {
    for _ in 0..1_500 {
        if !running.sending.inner.held.rows().is_empty() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no action was held for its message");
}

/// Takes WhatsApp out of reach, and waits until the sender sees it so.
async fn until_offline(running: &Running) {
    running.connection(jaw_proto::ConnectionState::Offline);
    for _ in 0..500 {
        if !*running.sending.inner.connected.borrow() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the sender never saw WhatsApp out of reach");
}

/// The notices pushed within `within`, in order.
async fn notices(pushes: &mut broadcast::Receiver<Push>, within: Duration) -> Vec<String> {
    let end = tokio::time::Instant::now() + within;
    let mut said = Vec::new();
    loop {
        match tokio::time::timeout_at(end, pushes.recv()).await {
            Ok(Ok(push)) if push.method == "notice" => said.push(push.params["text"].as_str().unwrap().to_string()),
            Ok(Ok(_) | Err(broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(broadcast::error::RecvError::Closed)) | Err(_) => return said,
        }
    }
}

/// Message `id`'s text as its row holds it, whatever windows show over it.
async fn stored_text(running: &Running, id: &str) -> String {
    let id = id.to_string();
    running.query(move |db| Ok(db.query_row("SELECT text FROM messages WHERE id = ?1", [id], |r| r.get(0))?)).await
}

/// How many reactions the rows hold, whatever windows show over them.
async fn stored_reactions(running: &Running) -> i64 {
    running.query(|db| Ok(db.query_row("SELECT count(*) FROM reactions", [], |r| r.get(0))?)).await
}

#[tokio::test(flavor = "multi_thread")]
async fn an_action_shows_in_windows_at_once_and_its_row_holds_it_only_once_whatsapp_has_it() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await.unwrap();
    running.chats(&[ANA]).await;
    let id = running.send(ANA, "chego às 20h").await;
    running.until_status(ANA, &id, "sent").await;
    running.world.gate(true);
    let before = row(&running, ANA, &id).await.unwrap().v;
    running.call("edit", json!({ "message": id, "text": "chego às 21h" })).await.unwrap();
    running.call("react", json!({ "message": id, "emoji": "👍" })).await.unwrap();
    let shown = row(&running, ANA, &id).await.unwrap();
    assert!(shown.v > before, "with a version of its own, so a window watched again from before gets it");
    assert_eq!((text_of(&shown), shown.edited), ("chego às 21h".into(), true));
    assert_eq!(reactions(&shown), [("👍".into(), 1, true)]);
    let chat = running.query(|db| Ok(read::chats(db, false, None, 10)?.remove(0))).await;
    assert_eq!(chat.last.unwrap().summary, "chego às 21h", "and the chat list says the same");
    assert_eq!(
        (stored_text(&running, &id).await, stored_reactions(&running).await),
        ("chego às 20h".into(), 0),
        "while its row has none of it"
    );
    running.call("delete", json!({ "message": id, "for": "everyone" })).await.unwrap();
    let gone = row(&running, ANA, &id).await.unwrap();
    assert_eq!((gone.deleted, gone.spans.clone()), (true, json!([])));
    let chat = running.query(|db| Ok(read::chats(db, false, None, 10)?.remove(0))).await;
    assert_eq!(chat.last.unwrap().summary, "You deleted this message");
    running.world.gate(false);
    until_stored(&running).await;
    let kept = row(&running, ANA, &id).await.unwrap();
    assert!(kept.deleted && kept.spans == json!([]), "once WhatsApp has them all, the row holds them");
    let deleted: bool = {
        let id = id.clone();
        running
            .query(move |db| Ok(db.query_row("SELECT deleted FROM messages WHERE id = ?1", [id], |r| r.get(0))?))
            .await
    };
    assert!(deleted);
}

/// A photo from Ana, as WhatsApp sent it, with what downloading it again needs.
async fn photo_from(running: &Running, chat: &str, id: &str, forwarded: u32) {
    use wa_proto::wa_web_protobufs_web::WebMessageInfo;
    let context = (forwarded > 0).then(|| {
        Box::new(wa_proto::wa_web_protobufs_e2e::ContextInfo {
            is_forwarded: Some(true),
            forwarding_score: Some(forwarded),
            ..Default::default()
        })
    });
    let image = wa_proto::wa_web_protobufs_e2e::ImageMessage {
        url: Some("https://mmg.whatsapp.net/photo".into()),
        media_key: Some(vec![7; 32]),
        caption: Some("o lago".into()),
        mimetype: Some("image/jpeg".into()),
        context_info: context,
        ..Default::default()
    };
    let info = WebMessageInfo {
        key: wa_proto::wa_common::MessageKey {
            remote_jid: Some(chat.into()),
            from_me: Some(false),
            id: Some(id.into()),
            participant: None,
        },
        message: Some(wa_proto::wa_web_protobufs_e2e::Message {
            image_message: Some(Box::new(image)),
            ..Default::default()
        }),
        message_timestamp: Some((now_ms() / 1000) as u64),
        ..Default::default()
    };
    let changes = crate::convert::message(&info, &crate::convert::Me::default(), true);
    running.store.apply(Lane::Live, changes).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forward_sends_each_message_to_each_chat_in_order_as_new_ones_of_yours_marked_forwarded() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.members(CLIMBING, &[RUI]).await;
    running.chats(&[RUI]).await;
    running.arrives(ANA, ANA, "m1", now_ms() - 60_000).await;
    photo_from(&running, ANA, "p1", 4).await;
    running.call("draft.set", json!({ "chat": RUI, "text": "e tu?" })).await.unwrap();
    let params = json!({ "messages": ["m1", "p1"], "to": [RUI, CLIMBING], "key": "jaw-fwd-1" });
    let answer = running.call("forward", params.clone()).await.unwrap();
    let ids: Vec<String> = serde_json::from_value(answer["messages"].clone()).unwrap();
    assert_eq!(ids.len(), 4, "each message to each chat: {answer}");
    let again = running.call("forward", params).await.unwrap();
    assert_eq!(again, answer, "asked again with its key, the same messages");
    sent(&running, 4).await;
    until_stored(&running).await;
    let sends = running.world.sends();
    assert_eq!(sends.len(), 4, "and nothing more went");
    let order: Vec<(&str, &str)> = sends.iter().map(|s| (s.chat.as_str(), s.id.as_str())).collect();
    assert_eq!(
        order.iter().filter(|(chat, _)| *chat == RUI).map(|(_, id)| *id).collect::<Vec<_>>(),
        [ids[0].as_str(), ids[1].as_str()],
        "chat by chat, in order"
    );
    let raws = raws(&running);
    let text = raws.iter().find_map(|m| m.extended_text_message.as_deref()).expect("the text, as extended text");
    let context = text.context_info.as_deref().unwrap();
    assert_eq!(
        (text.text.as_deref(), context.is_forwarded, context.forwarding_score),
        (Some("message m1"), Some(true), Some(1))
    );
    let image = raws.iter().find_map(|m| m.image_message.as_deref()).expect("the photo, as it came");
    assert_eq!(
        (image.url.as_deref(), image.media_key.as_deref()),
        (Some("https://mmg.whatsapp.net/photo"), Some(&[7u8; 32][..]))
    );
    assert_eq!(image.context_info.as_deref().and_then(|c| c.forwarding_score), Some(5), "once more than it had been");
    let photo = row(&running, RUI, &ids[1]).await.unwrap();
    assert_eq!((photo.kind.as_str(), photo.mine, photo.forwarded), ("image", true, 5));
    assert!(matches!(photo.status.as_deref(), Some("sent" | "delivered" | "read")), "with ticks of its own");
    let chats = running.query(|db| read::chats(db, false, None, 10)).await;
    let rui = chats.iter().find(|c| c.id == RUI).unwrap();
    assert_eq!(rui.draft.as_deref(), Some("e tu?"), "what was typed there stays");
    let refused = |params: Value| running.call("forward", params);
    let six: Vec<String> = (0..6).map(|n| format!("35193000000{n}@s.whatsapp.net")).collect();
    assert_eq!(refused(json!({ "messages": ["m1"], "to": six })).await.unwrap_err().code, code::LIMIT);
    assert_eq!(refused(json!({ "messages": ["m1"], "to": ["nobody@g.us"] })).await.unwrap_err().code, code::NOT_FOUND);
    assert_eq!(refused(json!({ "messages": ["nope"], "to": [RUI] })).await.unwrap_err().code, code::NOT_FOUND);
    assert_eq!(refused(json!({ "messages": [], "to": [RUI] })).await.unwrap_err().code, code::INVALID_PARAMS);
    let revoke = Amend { what: Amendment::Revoke, ..other_of(ANA, "m1") };
    running.store.apply(Lane::Live, vec![Change::Amend(revoke)]).await.unwrap();
    assert_eq!(refused(json!({ "messages": ["m1"], "to": [RUI] })).await.unwrap_err().code, code::NOT_ALLOWED);
    let once = Message { view_once: true, ..message_from(ANA, "once") };
    let line = Message { kind: "system", ..message_from(ANA, "line") };
    running.store.apply(Lane::Live, vec![Change::Message(once), Change::Message(line)]).await.unwrap();
    for id in ["once", "line"] {
        assert_eq!(
            refused(json!({ "messages": [id], "to": [RUI] })).await.unwrap_err().code,
            code::NOT_ALLOWED,
            "{id}"
        );
    }
    assert_eq!(running.world.sends().len(), 4, "nothing went for what was refused");
}

/// A text message from someone else in a one-to-one chat with them.
fn message_from(chat: &str, id: &str) -> Message {
    Message {
        chat: chat.into(),
        id: id.into(),
        at: now_ms(),
        sender: Some(chat.into()),
        kind: "text",
        text: Some(format!("message {id}")),
        summary: format!("message {id}"),
        live: true,
        ..Message::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn message_info_gives_each_recipients_times_from_their_receipts_and_keeps_them_once_the_message_settles() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.world.replying(Replies { delivered: Duration::from_secs(3_600), read: Duration::from_secs(3_600) });
    let (sofia, sofia_lid) = ("351915555555@s.whatsapp.net", "100000000005@lid");
    running.members(CLIMBING, &[RUI, sofia]).await;
    named(&running, RUI, "Rui").await;
    let both = Person { jids: vec![sofia.into(), sofia_lid.into()], name: Some("Sofia".into()), ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(both)]).await.unwrap();
    running.world.gate(true);
    let id = running.send(CLIMBING, "quem vem?").await;
    let info = |id: String| running.call("message.info", json!({ "message": id }));
    let pending = info(id.clone()).await.unwrap();
    assert_eq!(pending["sent"], Value::Null, "not sent yet");
    let waiting: Vec<(&str, &Value)> = pending["recipients"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["name"].as_str().unwrap(), &r["delivered"]))
        .collect();
    assert_eq!(waiting, [("Rui", &Value::Null), ("Sofia", &Value::Null)], "everyone it goes to, you apart");
    running.world.gate(false);
    running.until_status(CLIMBING, &id, "sent").await;
    running.receipt(CLIMBING, &id, RUI, "delivered").await;
    running.receipt(CLIMBING, &id, sofia_lid, "delivered").await;
    running.receipt(CLIMBING, &id, sofia, "read").await;
    let half = info(id.clone()).await.unwrap();
    let sofia_row = half["recipients"].as_array().unwrap().iter().find(|r| r["name"] == "Sofia").unwrap().clone();
    assert!(sofia_row["delivered"].is_i64() && sofia_row["read"].is_i64(), "by either of her JIDs: {sofia_row}");
    assert!(half["sent"].is_i64());
    running.receipt(CLIMBING, &id, RUI, "read").await;
    running.until_status(CLIMBING, &id, "read").await;
    // Read by all, who it went to is let go of; the receipts say the same.
    loop {
        let settled = running.query(|db| crate::store::outbox::settled_messages(db, 8)).await;
        let Some(&message) = settled.first() else { break };
        running.store.apply(Lane::Background, vec![Change::Outbox(Step::LetGo { message })]).await.unwrap();
    }
    assert_eq!(running.store.count("recipients").unwrap(), 0);
    let settled = info(id.clone()).await.unwrap();
    let read: Vec<(&str, bool)> = settled["recipients"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["name"].as_str().unwrap(), r["read"].is_i64()))
        .collect();
    assert_eq!(read, [("Rui", true), ("Sofia", true)], "{settled}");
    running.arrives(CLIMBING, RUI, "theirs", 5_000).await;
    let theirs = info("theirs".into()).await.unwrap();
    assert_eq!((theirs["sent"].as_i64(), theirs["recipients"].as_array().unwrap().len()), (Some(5_000), 0));
    running.chats(&[ANA]).await;
    let direct = running.send(ANA, "olá").await;
    running.until_status(ANA, &direct, "sent").await;
    running.receipt(ANA, &direct, ANA, "delivered").await;
    let one = info(direct).await.unwrap();
    assert_eq!(one["recipients"].as_array().unwrap().len(), 1, "the other person in a one-to-one chat: {one}");
    assert!(one["recipients"][0]["delivered"].is_i64());
    assert_eq!(info("nope".into()).await.unwrap_err().code, code::NOT_FOUND);
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
        (
            outbox::CHAT_TO_SEND,
            "SEARCH c USING INDEX sqlite_autoindex_chats_1 (id=?)\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN",
        ),
        (outbox::ME, "SEARCH sync_state USING PRIMARY KEY (key=?)"),
        (
            quote::QUOTED,
            "SEARCH m USING INDEX messages_by_id (chat=? AND id=?)\nSEARCH j USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN",
        ),
        (
            quote::QUOTED_MERGING,
            "SEARCH c USING COVERING INDEX chat_merges_by_keep (keep=?)\nSEARCH m USING INDEX messages_by_id (chat=? AND id=?)\nSEARCH j USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN",
        ),
        (
            quote::MEMBER_JID,
            "SEARCH j USING INDEX person_jids_by_person (person=?)\nSEARCH g USING COVERING INDEX group_members_by_member (member=? AND chat=?)",
        ),
        (quote::NAMES, "SEARCH people USING INTEGER PRIMARY KEY (rowid=?)"),
        (quote::CHAT_NAME, "SEARCH chats USING INTEGER PRIMARY KEY (rowid=?)"),
        (info::CHAT, "SEARCH chats USING INDEX sqlite_autoindex_chats_1 (id=?)"),
        (info::GROUP_MEMBERS, "SEARCH g USING PRIMARY KEY (chat=?)\nSEARCH j USING INTEGER PRIMARY KEY (rowid=?)"),
        (info::PERSON, "SEARCH people USING INTEGER PRIMARY KEY (rowid=?)"),
        (info::PHONE, "SEARCH person_jids USING INDEX person_jids_by_person (person=?)"),
        (info::PERSON_OF, "SEARCH person_jids USING INDEX sqlite_autoindex_person_jids_1 (jid=?)"),
        (info::RECEIPTS, "SEARCH r USING PRIMARY KEY (message=?)\nSEARCH j USING INTEGER PRIMARY KEY (rowid=?)"),
        (info::SENT_TO, "SEARCH s USING PRIMARY KEY (message=?)\nSEARCH j USING INTEGER PRIMARY KEY (rowid=?)"),
        (info::DIRECT_PERSON, "SEARCH chats USING INTEGER PRIMARY KEY (rowid=?)"),
        (
            actions::TARGET_IN,
            "SEARCH c USING INDEX sqlite_autoindex_chats_1 (id=?)\nSEARCH m USING INDEX messages_by_id (chat=? AND id=?)\nSEARCH j USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH cm USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH k USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nCORRELATED SCALAR SUBQUERY 1\nSEARCH a USING INDEX outbox_by_target (target=?)",
        ),
        (
            actions::CHAT_KEPT,
            "SEARCH c USING COVERING INDEX sqlite_autoindex_chats_1 (id=?)\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH k USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN",
        ),
        (
            actions::TARGET_ANYWHERE,
            "SCAN c\nSEARCH m USING INDEX messages_by_id (chat=? AND id=?)\nSEARCH j USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH cm USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH k USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nCORRELATED SCALAR SUBQUERY 1\nSEARCH a USING INDEX outbox_by_target (target=?)",
        ),
        (
            chat_actions::CHAT_TO_ACT_ON,
            "SEARCH c USING INDEX sqlite_autoindex_chats_1 (id=?)\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH k USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN",
        ),
        (
            chat_actions::NEWEST,
            "SEARCH c USING INDEX sqlite_autoindex_chats_1 (id=?)\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH j USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN",
        ),
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

/// Others' typing is heard only while WhatsApp says it: once it has been told you're online, not
/// while it can't be reached, until it's told again as the connection comes back, and not once it's
/// told you're away.
#[tokio::test(flavor = "multi_thread")]
async fn others_typing_is_heard_only_while_whatsapp_has_been_told_youre_online_and_can_be_reached() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let mut hearing = running.sending.hearing();
    async fn until(hearing: &mut watch::Receiver<bool>, now: bool) {
        let waited = tokio::time::timeout(Duration::from_secs(15), hearing.wait_for(|heard| *heard == now)).await;
        assert!(waited.is_ok_and(|heard| heard.is_ok()), "never heard {now}");
    }
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::Presence(_)))).await;
    assert!(!*hearing.borrow(), "not while it's told you're away, with nothing in front");
    let terminal = running.attention.viewer();
    terminal.focus(true);
    until(&mut hearing, true).await;
    assert_eq!(running.online(), [false, true], "once it's been told you're online");
    running.connection(jaw_proto::ConnectionState::Offline);
    until(&mut hearing, false).await;
    running.connection(jaw_proto::ConnectionState::Online);
    until(&mut hearing, true).await;
    assert_eq!(running.online(), [false, true, true], "told again as the connection came back");
    drop(terminal);
    until(&mut hearing, false).await;
    assert_eq!(running.online(), [false, true, true, false], "once it's been told you're away");
}

/// The app-state patches the engine was asked to send, in order.
fn patches_sent(running: &Running) -> Vec<pb::AppStatePatch> {
    asked(running, |k| match k {
        pb::command::Kind::AppStatePatch(patch) => Some(patch),
        _ => None,
    })
}

/// The collections the engine was asked to fetch again, each fetch's names.
fn fetched_again(running: &Running) -> Vec<Vec<String>> {
    asked(running, |k| match k {
        pb::command::Kind::AppStateResync(resync) => Some(resync.names),
        _ => None,
    })
}

fn value_of(patch: &pb::AppStatePatch) -> SyncActionValue {
    SyncActionValue::decode(patch.value.as_slice()).unwrap()
}

/// Chat `id` as the chat list shows it.
async fn chat_row(running: &Running, id: &str) -> read::ChatRow {
    let id = id.to_string();
    running
        .query(move |db| {
            let n = read::chat_n(db, &id)?.expect("a chat jawd has");
            Ok(read::chat(db, n)?.expect("its row"))
        })
        .await
}

/// When chat `id` was pinned, as jaw.db keeps it; 0 when it isn't.
async fn pinned_at(running: &Running, id: &str) -> i64 {
    let id = id.to_string();
    running.query(move |db| Ok(db.query_row("SELECT pinned FROM chats WHERE id = ?1", [id], |r| r.get(0))?)).await
}

/// Waits until WhatsApp's echo of `patch` through app state has been converted, which keeps its value
/// as the latest for its index.
async fn until_echoed(running: &Running, patch: &pb::AppStatePatch) {
    let (key, value) = (serde_json::to_string(&patch.index).unwrap(), patch.value.clone());
    eventually(running, "the patch's echo", move |db| {
        let kept: Option<Vec<u8>> =
            db.query_row("SELECT value FROM app_state WHERE key = ?1", [&key], |r| r.get(0)).optional()?;
        Ok(kept.as_ref() == Some(&value))
    })
    .await;
}

/// Fills the outbox with `rows` reactions waiting to send, which nothing sends.
fn fill_outbox(home: &Path, rows: i64) {
    let mut db = Connection::open(home.join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    let tx = db.transaction().unwrap();
    for n in 0..rows {
        tx.execute(
            "INSERT INTO outbox (jid, id, kind, raw, next_at, created_at) VALUES (?1, ?2, 'reaction', x'', 0, 0)",
            rusqlite::params![ANA, format!("w{n}")],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn pinning_a_chat_pins_it_now_tells_your_other_devices_and_its_echo_changes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    let before = chat_row(&running, ANA).await.v;
    let asked_at = now_ms();
    assert_eq!(running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap(), json!({}));
    let pinned = chat_row(&running, ANA).await;
    let at = pinned_at(&running, ANA).await;
    assert!(pinned.pinned && pinned.v > before && at >= asked_at, "pinned now, with a new version");
    until_stored(&running).await;
    let sent = patches_sent(&running);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        (sent[0].name.as_str(), sent[0].index.as_slice(), sent[0].version),
        ("regular_low", &["pin_v1".to_string(), ANA.to_string()][..], 5)
    );
    let value = value_of(&sent[0]);
    assert_eq!((value.timestamp, value.pin_action.unwrap().pinned), (Some(at), Some(true)), "the time jawd wrote");
    until_echoed(&running, &sent[0]).await;
    let echoed = chat_row(&running, ANA).await;
    assert_eq!((echoed.v, pinned_at(&running, ANA).await), (pinned.v, at), "the echo found it as it was");
    assert_eq!(running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap(), json!({}));
    assert_eq!(chat_row(&running, ANA).await.v, pinned.v, "asked again, nothing changes");
    assert_eq!((patches_sent(&running).len(), running.store.count("outbox").unwrap()), (1, 0), "nothing goes");
    running.call("chat.pin", json!({ "chat": ANA, "pinned": false })).await.unwrap();
    until_stored(&running).await;
    let sent = patches_sent(&running);
    assert_eq!((sent.len(), value_of(&sent[1]).pin_action.unwrap().pinned), (2, Some(false)));
    assert!(!chat_row(&running, ANA).await.pinned);
}

#[tokio::test(flavor = "multi_thread")]
async fn at_most_three_chats_are_pinned_and_archived_ones_dont_count() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let chats: Vec<String> = (1..=5).map(|n| format!("35191000001{n}@s.whatsapp.net")).collect();
    let ids: Vec<&str> = chats.iter().map(String::as_str).collect();
    running.chats(&ids).await;
    // Pinned on the phone and archived there, as jaw shows it.
    let archived = Chat { id: chats[4].clone(), pinned: Some(1_000), archived: Some(true), ..Chat::default() };
    running.store.apply(Lane::Live, vec![Change::Chat(archived)]).await.unwrap();
    for id in &ids[..3] {
        running.call("chat.pin", json!({ "chat": id, "pinned": true })).await.unwrap();
    }
    let fourth = running.call("chat.pin", json!({ "chat": ids[3], "pinned": true })).await.unwrap_err();
    assert_eq!((fourth.code, fourth.data), (code::LIMIT, Some(json!({ "limit": "pinned", "most": 3 }))));
    assert!(!chat_row(&running, ids[3]).await.pinned, "not pinned");
    until_stored(&running).await;
    assert_eq!(patches_sent(&running).len(), 3, "and nothing sent for it");
    running.call("chat.pin", json!({ "chat": ids[0], "pinned": false })).await.unwrap();
    running.call("chat.pin", json!({ "chat": ids[3], "pinned": true })).await.unwrap();
    assert!(chat_row(&running, ids[3]).await.pinned, "once one is unpinned");
}

#[tokio::test(flavor = "multi_thread")]
async fn archiving_a_pinned_chat_unpins_it_and_tells_your_other_devices_in_two_patches_in_order() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.members(CLIMBING, &[RUI]).await;
    let at = now_ms() - 60_000;
    running.arrives(CLIMBING, RUI, "g1", at).await;
    running.call("chat.pin", json!({ "chat": CLIMBING, "pinned": true })).await.unwrap();
    running.call("chat.archive", json!({ "chat": CLIMBING, "archived": true })).await.unwrap();
    let group = chat_row(&running, CLIMBING).await;
    assert!(group.archived && !group.pinned, "archived, and unpinned as WhatsApp's apps do");
    until_stored(&running).await;
    let sent = patches_sent(&running);
    let kinds: Vec<(&str, i32)> = sent.iter().map(|p| (p.index[0].as_str(), p.version)).collect();
    assert_eq!(kinds, [("pin_v1", 5), ("archive", 3), ("pin_v1", 5)], "the archive, then the unpin");
    let archive = value_of(&sent[1]).archive_chat_action.unwrap();
    let named = archive.message_range.unwrap();
    let key = named.messages[0].key.clone().unwrap();
    assert_eq!((archive.archived, named.last_message_timestamp), (Some(true), Some(at / 1000)));
    assert_eq!((key.id.as_deref(), key.participant.as_deref()), (Some("g1"), Some(RUI)), "naming its newest");
    assert_eq!(value_of(&sent[2]).pin_action.unwrap().pinned, Some(false));
    until_echoed(&running, &sent[2]).await;
    assert_eq!(chat_row(&running, CLIMBING).await.v, group.v, "the echoes changed nothing");
    running.call("chat.archive", json!({ "chat": CLIMBING, "archived": false })).await.unwrap();
    until_stored(&running).await;
    let sent = patches_sent(&running);
    assert_eq!((sent.len(), value_of(&sent[3]).archive_chat_action.unwrap().archived), (4, Some(false)));
    let back = chat_row(&running, CLIMBING).await;
    assert!(!back.archived && !back.pinned, "brought back with one patch, and still unpinned");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mute_lasts_until_a_time_or_for_ever_and_a_time_already_past_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    let until = now_ms() + 8 * 3_600_000;
    running.call("chat.mute", json!({ "chat": ANA, "until": until })).await.unwrap();
    assert_eq!(chat_row(&running, ANA).await.muted_until, Some(until));
    running.call("chat.mute", json!({ "chat": ANA, "until": -1 })).await.unwrap();
    let forever = crate::convert::app_state::FOREVER;
    assert_eq!(chat_row(&running, ANA).await.muted_until, Some(forever), "for ever");
    running.call("chat.mute", json!({ "chat": ANA, "until": i64::MAX })).await.unwrap();
    running.call("chat.mute", json!({ "chat": ANA, "until": null })).await.unwrap();
    assert_eq!(chat_row(&running, ANA).await.muted_until, None, "unmuted");
    let past = running.call("chat.mute", json!({ "chat": ANA, "until": now_ms() - 1_000 })).await.unwrap_err();
    assert_eq!((past.code, past.data), (code::INVALID_PARAMS, Some(json!({ "param": "until" }))));
    until_stored(&running).await;
    let sent = patches_sent(&running);
    let ends: Vec<(Option<bool>, Option<i64>)> = sent
        .iter()
        .map(|p| {
            let action = value_of(p).mute_action.unwrap();
            (action.muted, action.mute_end_timestamp)
        })
        .collect();
    assert_eq!(
        ends,
        [(Some(true), Some(until)), (Some(true), Some(-1)), (Some(false), None)],
        "the largest time was for ever already"
    );
    assert!(sent.iter().all(|p| (p.name.as_str(), p.version) == ("regular_high", 2)));
}

/// The latest patch the engine was asked to send for `index`.
fn latest_for(running: &Running, index: [&str; 2]) -> pb::AppStatePatch {
    patches_sent(running).into_iter().rev().find(|patch| patch.index == index).expect("a patch for the index")
}

#[tokio::test(flavor = "multi_thread")]
async fn pinning_unpinning_and_pinning_again_at_once_ends_pinned_here_and_on_every_device() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    for pinned in [true, false, true] {
        running.call("chat.pin", json!({ "chat": ANA, "pinned": pinned })).await.unwrap();
    }
    let pinned = chat_row(&running, ANA).await;
    assert!(pinned.pinned, "pinned, as asked last");
    until_stored(&running).await;
    let sent = patches_sent(&running);
    assert_eq!(sent.len(), 3, "none taken as so already");
    until_echoed(&running, &sent[2]).await;
    assert_eq!(chat_row(&running, ANA).await.v, pinned.v, "the echoes changed nothing");
    let latest = value_of(&latest_for(&running, ["pin_v1", ANA]));
    assert_eq!(latest.pin_action, Some(PinAction { pinned: Some(true) }), "and WhatsApp's latest pins it");
}

#[tokio::test(flavor = "multi_thread")]
async fn toggling_a_pin_while_whatsapp_cant_be_reached_ends_as_asked_last_once_it_can() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA]).await;
    until_offline(&running).await;
    for pinned in [true, false, true, false, true] {
        running.call("chat.pin", json!({ "chat": ANA, "pinned": pinned })).await.unwrap();
    }
    let asked = chat_row(&running, ANA).await;
    assert!(asked.pinned, "pinned, as asked last");
    assert!(patches_sent(&running).is_empty(), "nothing goes while WhatsApp can't be reached");
    running.connection(jaw_proto::ConnectionState::Online);
    until_stored(&running).await;
    let sent = patches_sent(&running);
    let pins: Vec<Option<bool>> = sent.iter().map(|p| value_of(p).pin_action.unwrap().pinned).collect();
    assert_eq!(pins, [Some(true), Some(false), Some(true), Some(false), Some(true)], "each, in the order asked");
    until_echoed(&running, &sent[4]).await;
    let ana = chat_row(&running, ANA).await;
    assert_eq!((ana.pinned, ana.v), (true, asked.v), "as asked last, the earlier echoes changing nothing");
    let latest = value_of(&latest_for(&running, ["pin_v1", ANA]));
    assert_eq!(latest.pin_action, Some(PinAction { pinned: Some(true) }), "and WhatsApp's latest pins it");
}

/// How many mutations the phone's app state holds ahead of Ana's chat.
const NAMES: usize = 300;

/// What the phone's app state holds ahead of Ana's chat.
#[derive(Clone, Copy)]
enum Ahead {
    /// NAMES people's names, in a collection of their own.
    Names,
    /// NAMES other chats unpinned, in her chat's collection, so a fetch of it alone reaches her
    /// chat only once every one of them is in.
    Chats,
}

/// A phone whose app state holds NAMES mutations and, after them, Ana's chat unpinned an hour ago,
/// so a full fetch of it reaches her chat only once every one of them is in.
struct Phone(Ahead);

impl demo::Account for Phone {
    fn contacts(&self) -> pb::ContactList {
        pb::ContactList::default()
    }

    fn groups(&self) -> pb::GroupList {
        pb::GroupList::default()
    }

    fn members(&self, _: &str) -> Vec<String> {
        Vec::new()
    }

    fn lid_pairs(&self, _: &[String]) -> pb::LidPairs {
        pb::LidPairs::default()
    }

    fn history(&self, _: &wa_proto::wa_web_protobufs_e2e::HistorySyncNotification) -> Option<demo::Blob> {
        None
    }

    fn app_state(&self) -> Vec<pb::Event> {
        let an_hour_ago = Some(now_ms() - 3_600_000);
        let mutation = |index: [String; 2], value: SyncActionValue| pb::Event {
            seq: 0,
            kind: Some(pb::event::Kind::AppState(pb::AppState { index: index.into(), value: value.encode_to_vec() })),
        };
        let unpinned = SyncActionValue {
            timestamp: an_hour_ago,
            pin_action: Some(PinAction { pinned: Some(false) }),
            ..Default::default()
        };
        let mut events: Vec<pb::Event> = (0..NAMES)
            .map(|n| {
                let jid = format!("35192{n:07}@s.whatsapp.net");
                if let Ahead::Chats = self.0 {
                    return mutation(["pin_v1".into(), jid], unpinned.clone());
                }
                let named = ContactAction { full_name: Some(format!("Person {n}")), ..Default::default() };
                let value =
                    SyncActionValue { timestamp: an_hour_ago, contact_action: Some(named), ..Default::default() };
                mutation(["contact".into(), jid], value)
            })
            .collect();
        events.push(mutation(["pin_v1".into(), ANA.into()], unpinned));
        events
    }
}

/// WhatsApp holding the phone's app state, whose people take an hour to answer a message.
fn phone_with_app_state(ahead: Ahead) -> Arc<World> {
    let world = DemoEngine::for_account(Vec::new(), Arc::new(Phone(ahead))).world();
    world.replying(Replies { delivered: Duration::from_secs(3_600), read: Duration::from_secs(3_600) });
    world
}

/// Waits until the engine has played `fetches` full fetches of the app state, and every mutation
/// they brought is converted.
async fn until_fetched(running: &Running, fetches: usize) {
    for _ in 0..1_500 {
        if running.world.fetched().len() >= fetches {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(running.world.fetched().len() >= fetches, "the engine didn't fetch the app state again in full");
    eventually(running, "the fetch's mutations to be converted", |db| {
        Ok(db.query_row("SELECT count(*) FROM inbox", [], |r| r.get::<_, i64>(0))? == 0)
    })
    .await;
}

/// Ana's chat's pinned and v.
fn ana_pinned(db: &Connection) -> rusqlite::Result<(i64, i64)> {
    db.query_row("SELECT pinned, v FROM chats WHERE id = ?1", [ANA], |r| Ok((r.get(0)?, r.get(1)?)))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pin_that_goes_while_the_engine_start_fetches_app_state_again_ends_pinned_as_asked() {
    let home = tempfile::tempdir().unwrap();
    crash(home.path(), "pinned behind a message").await;
    let pinned = ana_pinned(&Connection::open(home.path().join("jaw.db")).unwrap()).unwrap();
    assert!(pinned.0 > 0, "pinned while WhatsApp couldn't be reached");
    let world = phone_with_app_state(Ahead::Names);
    // The message ahead of the pin waits at WhatsApp until the engine start's full fetch has begun,
    // so the pin goes while the fetch is still handing over the names ahead of Ana's chat.
    world.gate(true);
    let running = Running::on(home.path(), world, true).await;
    running.world.until(|asked| asked.iter().any(|kind| matches!(kind, pb::command::Kind::AppStateResync(_)))).await;
    running.world.gate(false);
    until_stored(&running).await;
    until_fetched(&running, 1).await;
    assert_eq!(
        running.query(|db| Ok(ana_pinned(db)?)).await,
        pinned,
        "pinned at jaw's time, unchanged since the pin, the unpin the fetch brought held back"
    );
    let latest = value_of(&latest_for(&running, ["pin_v1", ANA]));
    assert_eq!(latest.pin_action, Some(PinAction { pinned: Some(true) }), "and WhatsApp's latest pins it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_jawd_doesnt_have_is_not_found_and_a_communitys_own_row_is_not_allowed() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let community = "120363000000000009@g.us";
    let own = Chat { id: community.into(), kind: Some("community"), ..Chat::default() };
    running.store.apply(Lane::Live, vec![Change::Chat(own)]).await.unwrap();
    for (chat, expected) in [("351999999999@s.whatsapp.net", code::NOT_FOUND), (community, code::NOT_ALLOWED)] {
        for (method, params) in [
            ("chat.pin", json!({ "chat": chat, "pinned": true })),
            ("chat.archive", json!({ "chat": chat, "archived": true })),
            ("chat.mute", json!({ "chat": chat, "until": -1 })),
        ] {
            assert_eq!(running.call(method, params).await.unwrap_err().code, expected, "{method} {chat}");
        }
    }
    assert_eq!(running.store.count("outbox").unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_action_needs_a_linked_account() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::unconnected(home.path()).await;
    running.chats(&[ANA]).await;
    for (method, params) in [
        ("chat.pin", json!({ "chat": ANA, "pinned": true })),
        ("chat.archive", json!({ "chat": ANA, "archived": true })),
        ("chat.mute", json!({ "chat": ANA, "until": null })),
    ] {
        assert_eq!(running.call(method, params).await.unwrap_err().code, code::NOT_LINKED, "{method}");
    }
    assert_eq!(running.store.count("outbox").unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn past_a_thousand_waiting_a_chat_action_is_refused_and_nothing_is_written() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA, CLIMBING]).await;
    running.call("chat.pin", json!({ "chat": CLIMBING, "pinned": true })).await.unwrap();
    until_stored(&running).await;
    until_offline(&running).await;
    fill_outbox(home.path(), 999);
    let full = |waiting: u64| Some(json!({ "limit": "outbox", "waiting": waiting, "failed": 0 }));
    let refused = running.call("chat.archive", json!({ "chat": CLIMBING, "archived": true })).await.unwrap_err();
    assert_eq!((refused.code, refused.data), (code::LIMIT, full(999)), "archiving a pinned chat takes two places");
    let group = chat_row(&running, CLIMBING).await;
    assert!(group.pinned && !group.archived, "nothing written");
    running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap();
    let refused = running.call("chat.mute", json!({ "chat": ANA, "until": -1 })).await.unwrap_err();
    assert_eq!((refused.code, refused.data), (code::LIMIT, full(1_000)));
    assert_eq!(chat_row(&running, ANA).await.muted_until, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn marking_a_chat_unread_marks_it_and_marking_it_read_reads_it_its_receipts_going_first() {
    let home = tempfile::tempdir().unwrap();
    // A second between a chat's receipts, so a patch that didn't wait would go well before them.
    let timing = Timing { read_every: Duration::from_secs(1), ..quick() };
    let running = Running::timed(home.path(), world(), true, true, timing).await;
    running.arrives(ANA, ANA, "m1", now_ms() - 2_000).await;
    running.call("read", json!({ "chat": ANA, "up_to": "m1" })).await.unwrap();
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::MarkRead(_)))).await;
    running.arrives(ANA, ANA, "m2", now_ms() - 1_000).await;
    running.call("chat.mark_unread", json!({ "chat": ANA, "unread": true })).await.unwrap();
    let marked = chat_row(&running, ANA).await;
    assert!(marked.marked_unread && marked.unread == 1);
    running.call("chat.mark_unread", json!({ "chat": ANA, "unread": false })).await.unwrap();
    let read = chat_row(&running, ANA).await;
    assert!(!read.marked_unread && read.unread == 0, "the mark gone, and read up to its newest");
    let receipt = |asked: &[pb::command::Kind]| {
        asked.iter().position(|k| matches!(k, pb::command::Kind::MarkRead(m) if m.ids == ["m2"]))
    };
    tokio::time::timeout(Duration::from_secs(5), running.world.until(|asked| receipt(asked).is_some()))
        .await
        .expect("m2's read receipt never went");
    until_stored(&running).await;
    let sent = patches_sent(&running);
    let reads: Vec<Option<bool>> = sent.iter().map(|p| value_of(p).mark_chat_as_read_action.unwrap().read).collect();
    assert_eq!(reads, [Some(false), Some(true)]);
    assert_eq!(
        (sent[1].name.as_str(), sent[1].index[0].as_str(), sent[1].version),
        ("regular_low", "markChatAsRead", 3)
    );
    let named = value_of(&sent[1]).mark_chat_as_read_action.unwrap().message_range.unwrap();
    assert_eq!(named.messages[0].key.clone().unwrap().id.as_deref(), Some("m2"));
    let asked = running.world.asked();
    let patch = asked.iter().position(|k| matches!(k, pb::command::Kind::AppStatePatch(p) if p == &sent[1]));
    assert!(matches!((receipt(&asked), patch), (Some(r), Some(p)) if r < p), "its receipt went before it");
    until_echoed(&running, &sent[1]).await;
    let echoed = chat_row(&running, ANA).await;
    assert_eq!((echoed.v, echoed.unread, echoed.marked_unread), (read.v, 0, false), "the echo changed nothing");
    running.call("chat.mark_unread", json!({ "chat": ANA, "unread": false })).await.unwrap();
    assert_eq!(patches_sent(&running).len(), 2, "read already, nothing more goes");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mark_read_whose_chat_merges_into_another_before_it_goes_sends_the_kept_chats_receipts_first() {
    let home = tempfile::tempdir().unwrap();
    // A minute between a chat's receipts, so only the patch's own wait sends them in time.
    let timing = Timing { read_every: Duration::from_secs(60), ..quick() };
    let running = Running::timed(home.path(), world(), true, true, timing).await;
    let lid = "100000000001@lid";
    running.chats(&[ANA]).await;
    running.arrives(lid, lid, "l0", now_ms() - 3_000).await;
    running.call("read", json!({ "chat": lid, "up_to": "l0" })).await.unwrap();
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::MarkRead(_)))).await;
    running.arrives(lid, lid, "l1", now_ms() - 2_000).await;
    until_offline(&running).await;
    running.call("chat.mark_unread", json!({ "chat": lid, "unread": false })).await.unwrap();
    // Ana's LID and number turn out to be one person before the patch can go: her LID's chat merges
    // into her number's, and goes.
    let both = Person { jids: vec![ANA.into(), lid.into()], ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(both)]).await.unwrap();
    let merging =
        || running.query(|db| Ok(db.query_row("SELECT count(*) FROM chat_merges", [], |r| r.get::<_, i64>(0))?));
    while merging().await > 0 {
        running.store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
    }
    assert_eq!(running.query(move |db| read::chat_n(db, lid)).await, None, "the LID's chat has gone");
    running.connection(jaw_proto::ConnectionState::Online);
    let receipt = |asked: &[pb::command::Kind]| {
        asked.iter().position(|k| matches!(k, pb::command::Kind::MarkRead(m) if m.ids.iter().any(|id| id == "l1")))
    };
    tokio::time::timeout(Duration::from_secs(5), running.world.until(|asked| receipt(asked).is_some()))
        .await
        .expect("l1's read receipt never went");
    until_stored(&running).await;
    let asked = running.world.asked();
    let patch = asked.iter().position(|k| matches!(k, pb::command::Kind::AppStatePatch(_)));
    assert!(matches!((receipt(&asked), patch), (Some(r), Some(p)) if r < p), "its receipt went before it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mark_read_that_goes_while_its_chat_merges_waits_for_the_merge_then_sends_the_kept_chats_receipts_first() {
    let home = tempfile::tempdir().unwrap();
    // A minute between a chat's receipts, so only the patch's own wait sends them in time; a patch
    // that fails its try is tried again within 20 ms.
    let timing = Timing { read_every: Duration::from_secs(60), ..quick() };
    let running = Running::timed(home.path(), world(), true, true, timing).await;
    let lid = "100000000001@lid";
    running.chats(&[ANA]).await;
    running.arrives(lid, lid, "l0", now_ms() - 3_000).await;
    running.call("read", json!({ "chat": lid, "up_to": "l0" })).await.unwrap();
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::MarkRead(_)))).await;
    running.arrives(lid, lid, "l1", now_ms() - 2_000).await;
    until_offline(&running).await;
    running.call("chat.mark_unread", json!({ "chat": lid, "unread": false })).await.unwrap();
    // Ana's LID and number turn out to be one person: her LID's chat starts merging into her
    // number's, and its messages can't move until the test lets them, whatever wakes the merge.
    let side = Connection::open(home.path().join("jaw.db")).unwrap();
    side.busy_timeout(Duration::from_secs(5)).unwrap();
    side.execute_batch(
        "CREATE TRIGGER moving_waits BEFORE UPDATE OF chat ON messages BEGIN SELECT RAISE(ABORT, 'not yet'); END;",
    )
    .unwrap();
    let both = Person { jids: vec![ANA.into(), lid.into()], ..Person::default() };
    running.store.apply(Lane::Live, vec![Change::Person(both)]).await.unwrap();
    running.connection(jaw_proto::ConnectionState::Online);
    eventually(&running, "the patch to fail its try while its chat merges, and be tried again, or to go", |db| {
        let row: Option<(i64, Option<String>)> = db
            .query_row("SELECT attempts, error FROM outbox WHERE kind = 'app_state'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        Ok(row.is_none_or(|(attempts, error)| attempts >= 2 && error.is_some_and(|e| e.contains("being merged"))))
    })
    .await;
    let early = patches_sent(&running).len();
    side.execute_batch("DROP TRIGGER moving_waits").unwrap();
    let merging =
        || running.query(|db| Ok(db.query_row("SELECT count(*) FROM chat_merges", [], |r| r.get::<_, i64>(0))?));
    while merging().await > 0 {
        running.store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
    }
    let receipt = |asked: &[pb::command::Kind]| {
        asked.iter().position(|k| matches!(k, pb::command::Kind::MarkRead(m) if m.ids.iter().any(|id| id == "l1")))
    };
    tokio::time::timeout(Duration::from_secs(5), running.world.until(|asked| receipt(asked).is_some()))
        .await
        .expect("l1's read receipt never went");
    until_stored(&running).await;
    let asked = running.world.asked();
    let patch = asked.iter().position(|k| matches!(k, pb::command::Kind::AppStatePatch(_)));
    assert!(matches!((receipt(&asked), patch), (Some(r), Some(p)) if r < p), "its receipt went before it");
    assert_eq!(early, 0, "nothing went while the chat merged");
}

#[tokio::test(flavor = "multi_thread")]
async fn marking_read_a_chat_jaw_holds_none_of_the_messages_of_keeps_its_count_until_whatsapps_echo_reads_it() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let phone =
        crate::store::Read { chat: ANA.into(), at: now_ms() - 60_000, phone: Some((3, 0)), ..Default::default() };
    running.store.apply(Lane::Live, vec![Change::Read(phone)]).await.unwrap();
    let before = chat_row(&running, ANA).await;
    assert_eq!(before.unread, 3, "the phone's count, with none of its messages here");
    until_offline(&running).await;
    running.call("chat.mark_unread", json!({ "chat": ANA, "unread": false })).await.unwrap();
    let asked = chat_row(&running, ANA).await;
    assert_eq!((asked.unread, asked.v), (3, before.v), "its count kept until the patch's echo, and nothing new");
    running.connection(jaw_proto::ConnectionState::Online);
    until_stored(&running).await;
    let sent = patches_sent(&running);
    assert_eq!(sent.len(), 1);
    until_echoed(&running, &sent[0]).await;
    let echoed = chat_row(&running, ANA).await;
    assert_eq!(echoed.unread, 0, "WhatsApp's echo of the patch reads it");
    let since = before.v;
    let versions: Vec<i64> = running
        .query(move |db| {
            let n = read::chat_n(db, ANA)?.expect("a chat jawd has");
            Ok(db
                .prepare("SELECT seq FROM events WHERE type = 'chat' AND key = ?1 AND seq > ?2 ORDER BY seq")?
                .query_map([n, since], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await;
    assert_eq!(versions, [echoed.v], "the echo's version is the only new one");
}

#[tokio::test(flavor = "multi_thread")]
async fn starring_a_message_stars_it_and_names_it_as_whatsapp_does() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.members(CLIMBING, &[RUI]).await;
    running.arrives(CLIMBING, RUI, "g1", now_ms() - 3_000).await;
    running.arrives(ANA, ANA, "d1", now_ms() - 2_000).await;
    let mine = running.send(ANA, "o meu").await;
    running.until_status(ANA, &mine, "sent").await;
    let before = row(&running, CLIMBING, "g1").await.unwrap().v;
    // Starred while WhatsApp can't be reached, so the rows are read before any patch goes.
    until_offline(&running).await;
    for (message, chat) in [("g1", Some(CLIMBING)), ("d1", None), (mine.as_str(), Some(ANA))] {
        let mut params = json!({ "message": message, "starred": true });
        if let Some(chat) = chat {
            params["chat"] = json!(chat);
        }
        assert_eq!(running.call("star", params).await.unwrap(), json!({}), "{message}");
    }
    let g1 = row(&running, CLIMBING, "g1").await.unwrap();
    assert!(g1.starred && g1.v > before, "starred, with a new version");
    let d1 = row(&running, ANA, "d1").await.unwrap();
    let own = row(&running, ANA, &mine).await.unwrap();
    running.connection(jaw_proto::ConnectionState::Online);
    until_stored(&running).await;
    let sent = patches_sent(&running);
    let index = |parts: [&str; 5]| parts.iter().map(|part| part.to_string()).collect::<Vec<_>>();
    // Each chat's patches go in the order asked, but the two chats' race for the app-state turn.
    let sent_in = |chat: &str| -> Vec<Vec<String>> {
        sent.iter().filter(|p| p.index[1] == chat).map(|p| p.index.clone()).collect()
    };
    assert_eq!(sent.len(), 3);
    assert_eq!(sent_in(CLIMBING), [index(["star", CLIMBING, "g1", "0", RUI])]);
    assert_eq!(sent_in(ANA), [index(["star", ANA, "d1", "0", "0"]), index(["star", ANA, mine.as_str(), "1", "0"])]);
    assert!(sent.iter().all(|p| (p.name.as_str(), p.version) == ("regular_high", 2)));
    for patch in &sent {
        until_echoed(&running, patch).await;
    }
    let echoed = (
        row(&running, CLIMBING, "g1").await.unwrap().v,
        row(&running, ANA, "d1").await.unwrap().v,
        row(&running, ANA, &mine).await.unwrap().v,
    );
    assert_eq!(echoed, (g1.v, d1.v, own.v), "the echoes changed nothing");
    running.call("star", json!({ "message": "g1", "starred": true })).await.unwrap();
    running.call("star", json!({ "message": "g1", "starred": false })).await.unwrap();
    assert!(!row(&running, CLIMBING, "g1").await.unwrap().starred);
    until_stored(&running).await;
    let sent = patches_sent(&running);
    assert_eq!((sent.len(), value_of(&sent[3]).star_action.unwrap().starred), (4, Some(false)), "asked again, nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_line_about_the_chat_or_a_deleted_message_isnt_starred_but_a_star_can_always_go() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let line = Message {
        chat: ANA.into(),
        id: "s1".into(),
        at: now_ms() - 3_000,
        kind: "system",
        summary: "Ana changed her number".into(),
        live: true,
        ..Message::default()
    };
    running.store.apply(Lane::Live, vec![Change::Message(line)]).await.unwrap();
    running.arrives(ANA, ANA, "gone", now_ms() - 2_000).await;
    running.call("star", json!({ "message": "gone", "starred": true })).await.unwrap();
    let revoke = Amend {
        chat: ANA.into(),
        target: "gone".into(),
        sender: Some(ANA.into()),
        mine: false,
        at: now_ms(),
        what: Amendment::Revoke,
        raw: None,
    };
    running.store.apply(Lane::Live, vec![Change::Amend(revoke)]).await.unwrap();
    for message in ["s1", "gone"] {
        let refused = running.call("star", json!({ "message": message, "starred": true, "chat": ANA })).await;
        assert_eq!(refused.unwrap_err().code, code::NOT_ALLOWED, "{message}");
    }
    running.call("star", json!({ "message": "gone", "starred": false })).await.unwrap();
    assert!(!row(&running, ANA, "gone").await.unwrap().starred, "a deleted message's star can go");
}

#[tokio::test(flavor = "multi_thread")]
async fn marking_and_starring_refuse_what_the_other_chat_actions_refuse() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    let community = "120363000000000009@g.us";
    let own = Chat { id: community.into(), kind: Some("community"), ..Chat::default() };
    running.store.apply(Lane::Live, vec![Change::Chat(own)]).await.unwrap();
    running.arrives(ANA, ANA, "m1", now_ms() - 1_000).await;
    let mark = |chat: &str| json!({ "chat": chat, "unread": true });
    let missing = running.call("chat.mark_unread", mark("351999999999@s.whatsapp.net")).await.unwrap_err();
    let own_row = running.call("chat.mark_unread", mark(community)).await.unwrap_err();
    let nothing = running.call("star", json!({ "message": "nope", "starred": true })).await.unwrap_err();
    assert_eq!((missing.code, own_row.code, nothing.code), (code::NOT_FOUND, code::NOT_ALLOWED, code::NOT_FOUND));
    until_offline(&running).await;
    fill_outbox(home.path(), 1_000);
    let full = Some(json!({ "limit": "outbox", "waiting": 1_000, "failed": 0 }));
    for (method, params) in [("chat.mark_unread", mark(ANA)), ("star", json!({ "message": "m1", "starred": true }))] {
        let refused = running.call(method, params).await.unwrap_err();
        assert_eq!((refused.code, refused.data), (code::LIMIT, full.clone()), "{method}");
    }
    assert!(!chat_row(&running, ANA).await.marked_unread && !row(&running, ANA, "m1").await.unwrap().starred);
    let elsewhere = tempfile::tempdir().unwrap();
    let unlinked = Running::unconnected(elsewhere.path()).await;
    unlinked.arrives(ANA, ANA, "m1", now_ms()).await;
    for (method, params) in [("chat.mark_unread", mark(ANA)), ("star", json!({ "message": "m1", "starred": true }))] {
        assert_eq!(unlinked.call(method, params).await.unwrap_err().code, code::NOT_LINKED, "{method}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_patch_whatsapp_refuses_is_kept_in_quarantine_put_back_as_the_phone_has_it_and_said() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.chats(&[ANA, CLIMBING]).await;
    let mut pushes = running.pushes.subscribe();
    running.call("chat.pin", json!({ "chat": CLIMBING, "pinned": true })).await.unwrap();
    until_stored(&running).await;
    // WhatsApp won't take the unpin, so the group ends pinned, as WhatsApp keeps it.
    running.world.refuse_patches(1);
    running.call("chat.pin", json!({ "chat": CLIMBING, "pinned": false })).await.unwrap();
    assert!(!chat_row(&running, CLIMBING).await.pinned, "shown at once");
    running
        .until("the group pinned again", |db| {
            Ok(db.query_row("SELECT pinned FROM chats WHERE id = ?1", [CLIMBING], |r| r.get::<_, i64>(0))? > 0)
        })
        .await;
    // Nor the pin of a chat nothing ever pinned, which ends unpinned, as WhatsApp has it.
    running.world.refuse_patches(1);
    running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap();
    until_stored(&running).await;
    running
        .until("Ana's chat unpinned again", |db| {
            Ok(db.query_row("SELECT pinned FROM chats WHERE id = ?1", [ANA], |r| r.get::<_, i64>(0))? == 0)
        })
        .await;
    type Kept = (String, String, Vec<u8>);
    let kept: Vec<Kept> = running
        .query(|db| {
            Ok(db
                .prepare("SELECT source, error, raw FROM quarantine ORDER BY n")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await;
    let indexes: Vec<(String, String, Vec<String>)> = kept
        .into_iter()
        .map(|(source, error, raw)| (source, error, pb::AppStatePatch::decode(raw.as_slice()).unwrap().index))
        .collect();
    let index = |chat: &str| vec!["pin_v1".to_string(), chat.to_string()];
    let refused = |chat: &str| ("app_state_patch".to_string(), "server returned error 400".to_string(), index(chat));
    assert_eq!(indexes, [refused(CLIMBING), refused(ANA)], "each kept with why and its patch, never dropped");
    let said = notices(&mut pushes, Duration::from_millis(300)).await;
    assert_eq!(
        said,
        [
            "WhatsApp refused unpinning a chat: server returned error 400",
            "WhatsApp refused pinning a chat: server returned error 400"
        ]
    );
    let low = |asked: &[pb::command::Kind]| {
        asked.iter().filter(|k| matches!(k, pb::command::Kind::AppStateResync(r) if r.names == ["regular_low"])).count()
    };
    // Each refusal fetches its collection again.
    running.world.until(|asked| low(asked) == 2).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn telling_your_other_devices_of_a_deletion_for_you_that_whatsapp_refuses_is_kept_in_quarantine() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::new(home.path()).await;
    running.arrives(ANA, ANA, "m1", now_ms() - 1_000).await;
    let mut pushes = running.pushes.subscribe();
    running.world.refuse_patches(1);
    running.call("delete", json!({ "message": "m1", "for": "me" })).await.unwrap();
    until_stored(&running).await;
    assert!(row(&running, ANA, "m1").await.is_none(), "deleted here all the same");
    assert_eq!(running.store.count("quarantine").unwrap(), 1, "what WhatsApp refused is kept, not dropped");
    let said = notices(&mut pushes, Duration::from_millis(300)).await;
    assert_eq!(said, ["WhatsApp refused telling your other devices you deleted a message: server returned error 400"]);
    let high = fetched_again(&running).iter().any(|names| names == &["regular_high".to_string()]);
    assert!(!high, "nothing to put back, so nothing fetched");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_patch_that_runs_out_of_tries_is_kept_in_quarantine_fetched_again_and_said_as_one_whatsapp_may_have() {
    let home = tempfile::tempdir().unwrap();
    // A try that fails is tried again within a millisecond, so the pin uses up its tries quickly.
    let timing = Timing { retry_first: Duration::from_millis(1), retry_most: Duration::from_millis(1), ..quick() };
    let running = Running::timed(home.path(), world(), true, true, timing).await;
    running.chats(&[ANA]).await;
    let mut pushes = running.pushes.subscribe();
    // WhatsApp takes the pin each time it's tried, but its echo never comes back in time.
    running.world.lose_echoes(outbox::ATTEMPTS_MOST as u32);
    running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap();
    let at = pinned_at(&running, ANA).await;
    until_stored(&running).await;
    assert_eq!(patches_sent(&running).len() as i64, outbox::ATTEMPTS_MOST, "tried as often as a failure that may pass");
    let kept: Vec<(String, String, Vec<u8>)> = running
        .query(|db| {
            Ok(db
                .prepare("SELECT source, error, raw FROM quarantine ORDER BY n")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await;
    let lost = "WhatsApp took the pin_v1 change, but its echo didn't come back within 30s";
    let indexes: Vec<(String, String, Vec<String>)> = kept
        .into_iter()
        .map(|(source, error, raw)| (source, error, pb::AppStatePatch::decode(raw.as_slice()).unwrap().index))
        .collect();
    let index = vec!["pin_v1".to_string(), ANA.to_string()];
    assert_eq!(indexes, [("app_state_patch".into(), lost.into(), index)], "kept with why and its patch, never dropped");
    let said = notices(&mut pushes, Duration::from_millis(300)).await;
    assert_eq!(said, [format!("jaw couldn't confirm pinning a chat with WhatsApp: {lost}")]);
    // Put back and fetched again, it ends as WhatsApp has it: pinned, at the time jaw pinned it.
    running
        .until("Ana's chat pinned again", move |db| {
            Ok(db.query_row("SELECT pinned FROM chats WHERE id = ?1", [ANA], |r| r.get::<_, i64>(0))? == at)
        })
        .await;
    let low = |asked: &[pb::command::Kind]| {
        asked.iter().filter(|k| matches!(k, pb::command::Kind::AppStateResync(r) if r.names == ["regular_low"])).count()
    };
    running.world.until(|asked| low(asked) == 1).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pin_asked_while_a_refusals_fetch_again_is_handed_over_goes_after_it_and_ends_pinned_as_asked() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::on(home.path(), phone_with_app_state(Ahead::Chats), true).await;
    running.chats(&[ANA, CLIMBING]).await;
    until_fetched(&running, 1).await;
    // WhatsApp refuses the group's pin, so its collection, Ana's chat's too, is fetched again.
    running.world.refuse_patches(1);
    running.call("chat.pin", json!({ "chat": CLIMBING, "pinned": true })).await.unwrap();
    let low = |asked: &[pb::command::Kind]| {
        asked.iter().any(|k| matches!(k, pb::command::Kind::AppStateResync(r) if r.names == ["regular_low"]))
    };
    running.world.until(low).await;
    // Ana's chat is pinned while that fetch still hands over the chats ahead of hers, and her chat
    // unpinned an hour ago after them.
    running.call("chat.pin", json!({ "chat": ANA, "pinned": true })).await.unwrap();
    let pinned = running.query(|db| Ok(ana_pinned(db)?)).await;
    assert!(running.world.fetched().len() < 2, "the fetch again was over before the pin was asked");
    until_stored(&running).await;
    until_fetched(&running, 2).await;
    assert_eq!(
        running.query(|db| Ok(ana_pinned(db)?)).await,
        pinned,
        "pinned at jaw's time, unchanged since the pin, the unpin the fetch brought held back"
    );
    let latest = value_of(&latest_for(&running, ["pin_v1", ANA]));
    assert_eq!(latest.pin_action, Some(PinAction { pinned: Some(true) }), "and WhatsApp's latest pins it");
}
