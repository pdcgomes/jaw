//! Each frontend's settings, kept in jaw.db as jaw's own in a domain named by the client its hello
//! gives. The terminal's, in the `jaw` domain, are the only ones so far: the theme and where your
//! messages sit. They're read once at start and changed one at a time: checked, kept with a synced
//! commit, then published on the domain's watch channel, which always holds the latest, for each
//! of its connections to push, and answered before the next change can be published.

use std::collections::BTreeMap;

use jaw_proto::settings::{Ignored, Messages, Settings as Terminal, TERMINAL};
use jaw_proto::{Error, code, themes};
use serde_json::{Value, json};
use tokio::sync::{Mutex, watch};

use crate::store::{Change, Store};

/// One of the terminal's settings, in the order of their names, which is the order `status` lists
/// those whose values this jawd doesn't take.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Messages,
    Theme,
}

impl Key {
    const ALL: [Key; 2] = [Key::Messages, Key::Theme];

    /// Its name, as it's asked for and kept.
    fn name(self) -> &'static str {
        match self {
            Key::Messages => "messages",
            Key::Theme => "theme",
        }
    }

    /// The values `settings.set` takes for it.
    fn allowed(self) -> Vec<&'static str> {
        match self {
            Key::Messages => Messages::ALL.iter().map(|m| m.name()).collect(),
            Key::Theme => themes::all().iter().map(|t| t.id.as_str()).collect(),
        }
    }

    /// Puts `value`, as jaw.db holds it, into `terminal` if this jawd takes it, and says whether it
    /// did.
    fn read(self, value: &Value, terminal: &mut Terminal) -> bool {
        match self {
            Key::Messages => match serde_json::from_value::<Messages>(value.clone()) {
                Ok(messages) => {
                    terminal.messages = messages;
                    true
                }
                Err(_) => false,
            },
            Key::Theme => match value.as_str().filter(|id| themes::get(id).is_some()) {
                Some(id) => {
                    terminal.theme = id.to_string();
                    true
                }
                None => false,
            },
        }
    }
}

pub struct Settings {
    store: Store,
    /// What jaw.db holds for each of the terminal's settings chosen, as its JSON. A change holds it
    /// from its check to its answer, so changes are made one at a time, in the order their commits
    /// are, and each is answered before the next is published.
    held: Mutex<BTreeMap<Key, String>>,
    published: watch::Sender<Terminal>,
    ignored: std::sync::Mutex<Vec<Ignored>>,
    /// In tests, where the next change waits once it's published, until it's let go.
    #[cfg(test)]
    gate: std::sync::Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
}

impl Settings {
    /// Reads what jaw.db holds for each setting of each domain jawd knows: the terminal's.
    pub fn load(store: Store) -> anyhow::Result<Settings> {
        let mut held = BTreeMap::new();
        for key in Key::ALL {
            if let Some(value) = store.setting(TERMINAL, key.name())? {
                held.insert(key, value);
            }
        }
        let (terminal, ignored) = in_effect(&held);
        Ok(Settings {
            store,
            held: Mutex::new(held),
            published: watch::channel(terminal).0,
            ignored: std::sync::Mutex::new(ignored),
            #[cfg(test)]
            gate: std::sync::Mutex::new(None),
        })
    }

    /// The terminal's settings in effect.
    pub fn current(&self) -> Terminal {
        self.published.borrow().clone()
    }

    /// A receiver for the settings of `client`'s domain, which has seen those in effect now and
    /// wakes for each later change; none for a client jawd keeps no settings for.
    pub fn subscribe(&self, client: &str) -> Option<watch::Receiver<Terminal>> {
        (client == TERMINAL).then(|| self.published.subscribe())
    }

    /// Each domain's settings in effect, by its name, as `status` says them.
    pub fn by_domain(&self) -> BTreeMap<String, serde_json::Map<String, Value>> {
        match serde_json::to_value(self.current()) {
            Ok(Value::Object(terminal)) => BTreeMap::from([(TERMINAL.to_string(), terminal)]),
            _ => BTreeMap::new(),
        }
    }

    /// Each setting whose value in jaw.db this jawd doesn't take.
    pub fn ignored(&self) -> Vec<Ignored> {
        self.ignored.lock().expect("the ignored settings").clone()
    }

    /// `settings.set` from a connection of `client`: checks that its domain is one jawd keeps and
    /// every setting asked before changing any, keeps those that differ from what jaw.db holds in
    /// one synced commit, and only then publishes the domain's settings in effect, if they changed,
    /// and gives `answer` the settings to answer with, or why not, once. It answers while it still
    /// holds the settings, so no later change is published before the answer is with the
    /// connection. What jaw.db holds already is answered at once, and written again never. It must
    /// run to completion: dropped while the writer has its change, it would leave jaw.db keeping
    /// the change while what's held and published stay as they were.
    pub async fn set(&self, client: &str, params: Value, answer: impl FnOnce(Result<Terminal, Error>)) {
        if client != TERMINAL {
            let mut error = Error::new(code::NOT_FOUND, format!("jawd keeps no settings for {client}"));
            error.data = Some(json!({ "client": client }));
            return answer(Err(error));
        }
        let asked = match checked(params) {
            Ok(asked) => asked,
            Err(error) => return answer(Err(error)),
        };
        let mut held = self.held.lock().await;
        let at = now_ms();
        let values: Vec<(String, String)> = asked
            .iter()
            .filter(|(key, value)| held.get(key) != Some(value))
            .map(|(key, value)| (key.name().to_string(), value.clone()))
            .collect();
        if values.is_empty() {
            return answer(Ok(self.current()));
        }
        let change = Change::Settings { domain: TERMINAL.to_string(), values, at };
        match self.store.apply_durable(vec![change]).await {
            Ok(applied) if applied.quarantined == 0 => {}
            Ok(applied) => {
                let why = applied.error.unwrap_or_default();
                return answer(Err(Error::new(
                    code::INTERNAL,
                    format!("jawd couldn't keep that setting, so nothing changed: {why}"),
                )));
            }
            Err(e) => {
                return answer(Err(Error::new(code::INTERNAL, format!("jawd couldn't keep that setting: {e:#}"))));
            }
        }
        held.extend(asked);
        let (terminal, ignored) = in_effect(&held);
        *self.ignored.lock().expect("the ignored settings") = ignored;
        self.published.send_if_modified(|published| std::mem::replace(published, terminal.clone()) != terminal);
        #[cfg(test)]
        self.gated().await;
        answer(Ok(terminal));
    }

    /// In tests, has the next change wait once it's published, before it's answered, until `open`
    /// is sent or dropped.
    #[cfg(test)]
    pub fn gate_next(&self, open: tokio::sync::oneshot::Receiver<()>) {
        *self.gate.lock().expect("the gate") = Some(open);
    }

    #[cfg(test)]
    async fn gated(&self) {
        let gate = self.gate.lock().expect("the gate").take();
        if let Some(gate) = gate {
            let _ = gate.await;
        }
    }
}

/// `settings.set`'s params checked: at least one setting, each one jawd knows with a value it
/// takes, as the JSON jaw.db keeps.
fn checked(params: Value) -> Result<Vec<(Key, String)>, Error> {
    let asked = match params {
        Value::Object(asked) if !asked.is_empty() => asked,
        _ => {
            return Err(Error::new(
                code::INVALID_PARAMS,
                "settings.set takes one setting or more, such as {\"theme\": \"gruvbox\"}",
            ));
        }
    };
    let mut out = Vec::new();
    for (name, value) in asked {
        let Some(key) = Key::ALL.into_iter().find(|k| k.name() == name) else {
            let mut error = Error::new(code::INVALID_PARAMS, format!("jawd doesn't know the setting {name}"));
            error.data = Some(json!({ "param": name }));
            return Err(error);
        };
        let allowed = key.allowed();
        match value.as_str().filter(|v| allowed.contains(v)) {
            Some(v) => out.push((key, Value::from(v).to_string())),
            None => {
                let mut error = Error::new(code::INVALID_PARAMS, format!("{name} takes one of {}", allowed.join(", ")));
                error.data = Some(json!({ "param": name, "allowed": allowed }));
                return Err(error);
            }
        }
    }
    Ok(out)
}

/// The settings in effect from what jaw.db holds, and each value held that this jawd doesn't take,
/// whose setting has its default instead.
fn in_effect(held: &BTreeMap<Key, String>) -> (Terminal, Vec<Ignored>) {
    let mut terminal = Terminal::default();
    let mut ignored = Vec::new();
    for (key, json) in held {
        let value: Value = serde_json::from_str(json).unwrap_or(Value::Null);
        if !key.read(&value, &mut terminal) {
            ignored.push(Ignored { domain: TERMINAL.to_string(), key: key.name().to_string(), value });
        }
    }
    (terminal, ignored)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use jaw_proto::settings::{Messages, TERMINAL};
    use serde_json::json;

    fn opened() -> (tempfile::TempDir, Store, Settings) {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let settings = Settings::load(store.clone()).unwrap();
        (home, store, settings)
    }

    /// What a change from a connection of `client` is answered with.
    async fn set(settings: &Settings, client: &str, params: Value) -> Result<Terminal, Error> {
        let mut answered = None;
        settings.set(client, params, |answer| answered = Some(answer)).await;
        answered.expect("every change is answered")
    }

    async fn refused(settings: &Settings, params: Value) -> Error {
        set(settings, TERMINAL, params).await.unwrap_err()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_change_is_checked_kept_with_a_synced_commit_and_then_published() {
        let (_home, store, settings) = opened();
        assert_eq!(settings.current(), Terminal::default());
        let mut heard = settings.subscribe(TERMINAL).unwrap();
        let synced = store.storage().synced_commits;
        let now = set(&settings, TERMINAL, json!({"theme": "gruvbox"})).await.unwrap();
        assert_eq!(now, Terminal { theme: "gruvbox".into(), messages: Messages::Bubbles });
        assert_eq!(store.storage().synced_commits, synced + 1);
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().as_deref(), Some("\"gruvbox\""));
        assert_eq!(store.setting(TERMINAL, "messages").unwrap(), None, "only what was asked is kept");
        assert!(heard.has_changed().unwrap());
        assert_eq!(*heard.borrow_and_update(), now);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_client_without_a_domain_has_no_settings_and_cant_change_any() {
        let (_home, store, settings) = opened();
        assert!(settings.subscribe("test").is_none());
        let e = set(&settings, "test", json!({"theme": "gruvbox"})).await.unwrap_err();
        assert_eq!((e.code, e.data), (code::NOT_FOUND, Some(json!({"client": "test"}))));
        assert_eq!(store.setting(TERMINAL, "theme").unwrap(), None, "nothing kept");
        let in_effect = settings.by_domain();
        assert_eq!(in_effect.keys().collect::<Vec<_>>(), [TERMINAL], "the terminal's is the only domain");
        assert_eq!(in_effect[TERMINAL]["theme"], "tokyo-night");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn every_setting_asked_is_checked_before_any_is_kept() {
        let (_home, store, settings) = opened();
        let e = refused(&settings, json!({"colour": "teal"})).await;
        assert_eq!((e.code, e.data), (code::INVALID_PARAMS, Some(json!({"param": "colour"}))));
        let e = refused(&settings, json!({"theme": "solarized"})).await;
        assert_eq!(e.code, code::INVALID_PARAMS);
        let data = e.data.unwrap();
        assert_eq!(data["param"], "theme");
        assert_eq!(data["allowed"].as_array().unwrap().len(), 23);
        let e = refused(&settings, json!({"messages": 2})).await;
        assert_eq!(e.data.unwrap()["allowed"], json!(["bubbles", "irc"]));
        for nothing in [json!({}), Value::Null, json!(["theme"])] {
            assert_eq!(refused(&settings, nothing).await.code, code::INVALID_PARAMS);
        }
        let heard = settings.subscribe(TERMINAL).unwrap();
        // Checked in the order of their names, so the setting it takes is checked before the one it
        // refuses.
        let e = refused(&settings, json!({"messages": "irc", "theme": "solarized"})).await;
        assert_eq!(e.data.unwrap()["param"], "theme");
        assert_eq!(store.setting(TERMINAL, "messages").unwrap(), None, "a refused ask keeps none of its settings");
        assert!(!heard.has_changed().unwrap(), "and publishes none");
        assert_eq!(settings.current(), Terminal::default());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn choosing_the_settings_in_effect_keeps_the_choice_and_publishes_nothing() {
        let (_home, store, settings) = opened();
        let heard = settings.subscribe(TERMINAL).unwrap();
        let synced = store.storage().synced_commits;
        assert_eq!(set(&settings, TERMINAL, json!({"theme": "tokyo-night"})).await.unwrap(), Terminal::default());
        assert_eq!(store.storage().synced_commits, synced + 1);
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().as_deref(), Some("\"tokyo-night\""), "the choice is kept");
        assert!(!heard.has_changed().unwrap(), "but nothing in effect changed, so nothing is published");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_value_jaw_db_holds_already_is_answered_at_once_and_writes_nothing() {
        let (_home, store, settings) = opened();
        set(&settings, TERMINAL, json!({"messages": "irc"})).await.unwrap();
        let synced = store.storage().synced_commits;
        let heard = settings.subscribe(TERMINAL).unwrap();
        let now = set(&settings, TERMINAL, json!({"messages": "irc"})).await.unwrap();
        assert_eq!(now.messages, Messages::Irc);
        assert_eq!(store.storage().synced_commits, synced, "nothing written");
        assert!(!heard.has_changed().unwrap(), "and nothing published");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_restart_keeps_them_and_a_value_jawd_doesnt_take_is_kept_and_read_as_the_default() {
        let home = tempfile::tempdir().unwrap();
        {
            let settings = Settings::load(Store::open(home.path()).unwrap()).unwrap();
            set(&settings, TERMINAL, json!({"theme": "nord", "messages": "irc"})).await.unwrap();
        }
        {
            let settings = Settings::load(Store::open(home.path()).unwrap()).unwrap();
            assert_eq!(settings.current(), Terminal { theme: "nord".into(), messages: Messages::Irc });
            assert!(settings.ignored().is_empty());
        }
        let outside = rusqlite::Connection::open(home.path().join("jaw.db")).unwrap();
        outside
            .execute("UPDATE settings SET value = '\"rose-pine-moon\"' WHERE domain = 'jaw' AND key = 'theme'", [])
            .unwrap();
        outside
            .execute(
                "INSERT INTO settings (domain, key, value, at)
                VALUES ('jaw', 'font', '\"Berkeley Mono\"', 1), ('jaw-mac', 'theme', '\"lumon\"', 1)",
                [],
            )
            .unwrap();
        drop(outside);
        let store = Store::open(home.path()).unwrap();
        let settings = Settings::load(store.clone()).unwrap();
        assert_eq!(settings.current(), Terminal { theme: themes::DEFAULT.into(), messages: Messages::Irc });
        let ignored = Ignored { domain: TERMINAL.into(), key: "theme".into(), value: json!("rose-pine-moon") };
        assert_eq!(settings.ignored(), vec![ignored]);
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().as_deref(), Some("\"rose-pine-moon\""), "kept as it was");
        assert_eq!(store.count("settings").unwrap(), 4, "a setting or a domain jawd doesn't know is left alone");
        assert_eq!(settings.by_domain().keys().collect::<Vec<_>>(), [TERMINAL], "and not read");
        let heard = settings.subscribe(TERMINAL).unwrap();
        set(&settings, TERMINAL, json!({"theme": "tokyo-night"})).await.unwrap();
        assert!(settings.ignored().is_empty(), "choosing the default replaces what wasn't understood");
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().as_deref(), Some("\"tokyo-night\""));
        assert!(!heard.has_changed().unwrap(), "and the default was in effect already, so nothing is published");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_change_that_cant_be_kept_publishes_nothing() {
        let (home, store, settings) = opened();
        let outside = rusqlite::Connection::open(home.path().join("jaw.db")).unwrap();
        outside
            .execute_batch("CREATE TRIGGER refused BEFORE INSERT ON settings BEGIN SELECT RAISE(ABORT, 'refused'); END")
            .unwrap();
        let heard = settings.subscribe(TERMINAL).unwrap();
        let e = refused(&settings, json!({"theme": "nord"})).await;
        assert_eq!(e.code, code::INTERNAL);
        assert!(e.message.starts_with("jawd couldn't keep that setting, so nothing changed: "), "{}", e.message);
        assert!(e.message.contains("refused"), "the answer says why: {}", e.message);
        assert_eq!(settings.current(), Terminal::default());
        assert!(!heard.has_changed().unwrap(), "nothing published");
        assert_eq!(store.setting(TERMINAL, "theme").unwrap(), None);
        outside.execute_batch("DROP TRIGGER refused").unwrap();
        assert_eq!(
            set(&settings, TERMINAL, json!({"theme": "nord"})).await.unwrap().theme,
            "nord",
            "and it can be asked again"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_ask_waits_for_the_one_being_kept_and_compares_with_what_it_kept() {
        let (home, store, settings) = opened();
        let settings = Arc::new(settings);
        // jaw.db's write lock, held from outside, so the first ask can't be kept until it's let go.
        let holder = rusqlite::Connection::open(home.path().join("jaw.db")).unwrap();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();
        let synced = store.storage().synced_commits;
        let ask = || {
            let settings = settings.clone();
            tokio::spawn(async move { set(&settings, TERMINAL, json!({"theme": "nord"})).await })
        };
        let first = ask();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let second = ask();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!first.is_finished() && !second.is_finished(), "neither is kept while jaw.db is locked");
        holder.execute_batch("COMMIT").unwrap();
        for ask in [first, second] {
            assert_eq!(ask.await.unwrap().unwrap().theme, "nord");
        }
        assert_eq!(
            store.storage().synced_commits - synced,
            1,
            "the second compared with what the first kept, so wrote nothing"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn changes_from_many_interfaces_at_once_end_with_jaw_db_and_what_was_published_agreeing() {
        let (_home, store, settings) = opened();
        let settings = Arc::new(settings);
        let asks: Vec<_> = (0..24)
            .map(|i| {
                let settings = settings.clone();
                let theme = themes::all()[i % 23].id.clone();
                let messages = if i % 2 == 0 { "irc" } else { "bubbles" };
                tokio::spawn(async move {
                    set(&settings, TERMINAL, json!({"theme": theme, "messages": messages})).await.unwrap()
                })
            })
            .collect();
        let mut answers = Vec::new();
        for ask in asks {
            answers.push(ask.await.unwrap());
        }
        let now = settings.current();
        assert!(answers.contains(&now), "what's in effect is the answer to the change kept last: {now:?}");
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().unwrap(), serde_json::to_string(&now.theme).unwrap());
        assert_eq!(
            store.setting(TERMINAL, "messages").unwrap().unwrap(),
            serde_json::to_string(&now.messages).unwrap()
        );
    }
}
