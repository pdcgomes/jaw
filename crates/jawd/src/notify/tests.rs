use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use jaw_proto::{Connection, ConnectionState};
use tokio::sync::mpsc;

use super::*;
use crate::store::{Change, Chat, Lane, Message, Read};

const ANA: &str = "351912345678@s.whatsapp.net";
const RUI: &str = "351913333333@s.whatsapp.net";
const CLIMBING: &str = "120363012345678901@g.us";

struct Fake(mpsc::UnboundedSender<Notice>);

impl Notifier for Fake {
    fn name(&self) -> &str {
        "to a test"
    }

    fn notify(&self, notice: &Notice) -> Result<()> {
        let _ = self.0.send(notice.clone());
        Ok(())
    }
}

const TIMING: Timing =
    Timing { quiet: Duration::from_millis(60), most: Duration::from_millis(200), hold: Duration::from_millis(600) };

/// Long enough for anything that would go to have gone.
const SETTLED: Duration = Duration::from_millis(250);

/// Notifications following a store of their own, with what they send and the knobs a test turns.
struct Running {
    _home: tempfile::TempDir,
    store: Store,
    attention: Arc<Attention>,
    catching_up: Arc<AtomicBool>,
    pushes: broadcast::Sender<Push>,
    sent: mpsc::UnboundedReceiver<Notice>,
}

impl Running {
    async fn new() -> Running {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let attention = Arc::new(Attention::default());
        let catching_up = Arc::new(AtomicBool::new(false));
        let (pushes, _) = broadcast::channel(16);
        let (to, sent) = mpsc::unbounded_channel();
        let flag = catching_up.clone();
        start(
            store.clone(),
            attention.clone(),
            Arc::new(move || flag.load(Ordering::SeqCst)),
            pushes.subscribe(),
            Arc::new(Fake(to)),
            TIMING,
        );
        let running = Running { _home: home, store, attention, catching_up, pushes, sent };
        for (chat, name, kind) in [(ANA, "Ana Ribeiro", "direct"), (CLIMBING, "Lisbon Climbing", "group")] {
            running.chat(Chat { id: chat.into(), name: Some(name.into()), kind: Some(kind), ..Chat::default() }).await;
        }
        running
    }

    async fn chat(&self, chat: Chat) {
        self.store.apply(Lane::Live, vec![Change::Chat(chat)]).await.unwrap();
    }

    /// A message arriving live from `sender` in `chat`, sent just now.
    async fn arrives(&self, chat: &str, sender: &str, id: &str, text: &str) {
        self.write(Message { sender: Some(sender.into()), live: true, ..message(chat, id, text, 0) }).await;
    }

    async fn write(&self, message: Message) {
        self.store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
    }

    /// A catch-up starts, and sync says so.
    fn catch_up(&self) {
        self.catching_up.store(true, Ordering::SeqCst);
        self.connection(ConnectionState::CatchingUp);
    }

    /// The catch-up ends, and sync says so.
    fn caught_up(&self) {
        self.catching_up.store(false, Ordering::SeqCst);
        self.connection(ConnectionState::Online);
    }

    fn connection(&self, state: ConnectionState) {
        let params = serde_json::to_value(Connection::new(state)).unwrap();
        self.pushes.send(Push { method: "connection".into(), params }).unwrap();
    }

    /// The next notification, however long a hold makes it wait.
    async fn next(&mut self) -> Notice {
        tokio::time::timeout(Duration::from_secs(5), self.sent.recv()).await.unwrap().unwrap()
    }

    /// The next `n` notifications however long they take, then anything more until things settle.
    async fn expect(&mut self, n: usize) -> Vec<Notice> {
        let mut sent = Vec::new();
        for _ in 0..n {
            sent.push(self.next().await);
        }
        sent.extend(self.sent().await);
        sent
    }

    /// What's sent from now until things settle.
    async fn sent(&mut self) -> Vec<Notice> {
        let mut sent = Vec::new();
        while let Ok(Some(notice)) = tokio::time::timeout(SETTLED, self.sent.recv()).await {
            sent.push(notice);
        }
        sent
    }
}

fn message(chat: &str, id: &str, text: &str, ago: i64) -> Message {
    Message {
        chat: chat.into(),
        id: id.into(),
        at: now_ms() - ago,
        kind: "text",
        text: Some(text.into()),
        summary: text.into(),
        ..Message::default()
    }
}

fn notice(title: &str, subtitle: Option<&str>, body: &str) -> Notice {
    Notice { title: title.into(), subtitle: subtitle.map(str::to_string), body: body.into() }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_burst_in_a_chat_is_one_notification_with_its_newest_message() {
    let mut running = Running::new().await;
    for (n, text) in ["are you", "coming", "tonight?"].iter().enumerate() {
        running.arrives(ANA, ANA, &format!("m{n}"), text).await;
    }
    assert_eq!(running.expect(1).await, vec![notice("Ana Ribeiro", Some("3 unread"), "tonight?")]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_group_names_who_wrote_and_media_says_its_kind() {
    let mut running = Running::new().await;
    let photo = Message { kind: "image", text: None, summary: String::new(), ..message(CLIMBING, "p1", "", 0) };
    running.write(Message { sender: Some(RUI.into()), live: true, ..photo }).await;
    assert_eq!(running.expect(1).await, vec![notice("Lisbon Climbing", None, "+351913333333: Photo")]);
}

#[tokio::test(flavor = "multi_thread")]
async fn muted_archived_yours_older_and_history_stay_quiet() {
    let mut running = Running::new().await;
    let muted = "351914444444@s.whatsapp.net";
    let archived = "351915555555@s.whatsapp.net";
    running.chat(Chat { id: muted.into(), muted_until: Some(now_ms() + 3_600_000), ..Chat::default() }).await;
    running.chat(Chat { id: archived.into(), archived: Some(true), ..Chat::default() }).await;
    running.arrives(muted, muted, "m1", "muted").await;
    running.arrives(archived, archived, "a1", "archived").await;
    running.write(Message { mine: true, live: true, ..message(ANA, "y1", "mine", 0) }).await;
    let older = message(RUI, "o1", "sent before jawd started, arriving now", 60_000);
    running.write(Message { sender: Some(RUI.into()), live: true, ..older }).await;
    let history = message(CLIMBING, "h1", "history, which isn't unread", 0);
    running.write(Message { sender: Some(RUI.into()), ..history }).await;
    assert_eq!(running.sent().await, Vec::new());
    running.arrives(ANA, ANA, "m2", "and this one?").await;
    assert_eq!(running.expect(1).await, vec![notice("Ana Ribeiro", None, "and this one?")]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_chat_an_interface_in_front_is_reading_stays_quiet() {
    let mut running = Running::new().await;
    let terminal = running.attention.viewer();
    terminal.watched("w2".into(), ANA.into());
    terminal.focus(true);
    running.arrives(ANA, ANA, "m1", "seen it?").await;
    running.arrives(CLIMBING, RUI, "c1", "Saturday at 9?").await;
    assert_eq!(running.expect(1).await, vec![notice("Lisbon Climbing", None, "+351913333333: Saturday at 9?")]);
    terminal.focus(false);
    running.arrives(ANA, ANA, "m2", "hello?").await;
    assert_eq!(running.expect(1).await, vec![notice("Ana Ribeiro", Some("2 unread"), "hello?")]);
}

#[tokio::test(flavor = "multi_thread")]
async fn what_comes_during_a_hold_goes_as_one_when_it_ends() {
    let mut running = Running::new().await;
    running.arrives(ANA, ANA, "m1", "one").await;
    assert_eq!(running.expect(1).await, vec![notice("Ana Ribeiro", None, "one")]);
    running.arrives(ANA, ANA, "m2", "two").await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    running.arrives(ANA, ANA, "m3", "three").await;
    let started = Instant::now();
    assert_eq!(running.next().await, notice("Ana Ribeiro", Some("3 unread"), "three"));
    assert!(started.elapsed() >= Duration::from_millis(150), "it went before the hold ended");
    assert_eq!(running.sent().await, Vec::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_catch_up_waits_and_ends_in_one_summary() {
    let mut running = Running::new().await;
    running.catch_up();
    let others: Vec<String> = (0..4).map(|n| format!("35191666666{n}@s.whatsapp.net")).collect();
    running.arrives(ANA, ANA, "m1", "hi").await;
    running.arrives(ANA, ANA, "m2", "there").await;
    running.arrives(CLIMBING, RUI, "c1", "Saturday?").await;
    for other in &others {
        running.arrives(other, other, "x1", "hello").await;
    }
    assert_eq!(running.sent().await, Vec::new(), "nothing goes while catching up");
    running.caught_up();
    let sent = running.expect(1).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!((sent[0].title.as_str(), sent[0].subtitle.as_deref()), ("jaw", Some("7 new messages in 6 chats")));
    assert!(sent[0].body.ends_with(" and 3 more"), "{}", sent[0].body);
}

#[tokio::test(flavor = "multi_thread")]
async fn what_came_while_jawd_was_off_is_one_summary_once_its_first_catch_up_ends() {
    let mut running = Running::new().await;
    let muted = "351914444444@s.whatsapp.net";
    let archived = "351915555555@s.whatsapp.net";
    let mum = "351916666666@s.whatsapp.net";
    running.chat(Chat { id: muted.into(), muted_until: Some(now_ms() + 3_600_000), ..Chat::default() }).await;
    running.chat(Chat { id: archived.into(), archived: Some(true), ..Chat::default() }).await;
    running.catch_up();
    let away = |chat: &str, sender: &str, id: &str, ago: i64| Message {
        sender: Some(sender.into()),
        live: true,
        ..message(chat, id, &format!("sent {ago} ms ago"), ago)
    };
    running.write(away(ANA, ANA, "m1", 3_000_000)).await;
    running.write(away(ANA, ANA, "m2", 2_900_000)).await;
    running.write(away(muted, muted, "u1", 2_800_000)).await;
    running.write(away(archived, archived, "a1", 2_700_000)).await;
    running.write(Message { sender: None, mine: true, ..away(mum, mum, "y1", 2_600_000) }).await;
    // App state for a chat with old unread messages touches it without bringing any.
    let read =
        Read { chat: "351917777777@s.whatsapp.net".into(), at: 0, ids: Vec::new(), phone: Some((4, 0)), here: false };
    running.store.apply(Lane::Live, vec![Change::Read(read)]).await.unwrap();
    assert_eq!(running.sent().await, Vec::new(), "nothing goes while catching up");
    running.caught_up();
    // WhatsApp says it's done before the last of it is converted.
    running.write(away(CLIMBING, RUI, "c1", 3_600_000)).await;
    assert_eq!(
        running.expect(1).await,
        vec![notice("jaw", Some("3 new messages in 2 chats"), "Ana Ribeiro, Lisbon Climbing")]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_catch_up_that_brought_one_chat_ends_in_that_chats_own_notification() {
    let mut running = Running::new().await;
    running.catch_up();
    let away = message(ANA, "m1", "while you were away", 600_000);
    running.write(Message { sender: Some(ANA.into()), live: true, ..away }).await;
    running.caught_up();
    assert_eq!(running.expect(1).await, vec![notice("Ana Ribeiro", None, "while you were away")]);
}

#[test]
fn a_long_message_is_cut_on_one_line() {
    let last = Last {
        from: None,
        mine: false,
        summary: format!("first line\n{}", "a".repeat(200)),
        kind: "text".into(),
        status: None,
        at: 0,
    };
    let shown = preview(&last);
    assert!(shown.starts_with("first line a") && shown.ends_with('…'), "{shown}");
    assert_eq!(shown.chars().count(), PREVIEW + 1);
}
