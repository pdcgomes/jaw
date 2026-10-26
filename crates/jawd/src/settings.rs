//! Each frontend's settings, kept in jaw.db as jaw's own in a domain named by the client its hello
//! gives. The terminal's, in the `jaw` domain, are the only ones so far: the theme and where your
//! messages sit. They're read once at start and changed one at a time: checked, kept with a synced
//! commit, then published on the domain's watch channel, which always holds the latest, for each
//! of its connections to push.

use std::collections::BTreeMap;

use jaw_proto::settings::{Ignored, Messages, Settings as Shared, TERMINAL};
use jaw_proto::{Error, code, themes};
use serde_json::{Value, json};
use tokio::sync::{Mutex, watch};

use crate::store::{Change, Store};

/// The terminal's settings, by the names they're asked and kept by.
const KNOWN: [&str; 2] = ["theme", "messages"];

pub struct Settings {
    store: Store,
    /// What jaw.db holds for each of the terminal's settings chosen, as its JSON. A change holds it
    /// from its check to its publishing, so changes are made one at a time, in the order their
    /// commits are.
    held: Mutex<BTreeMap<&'static str, String>>,
    published: watch::Sender<Shared>,
    ignored: std::sync::Mutex<Vec<Ignored>>,
}

impl Settings {
    /// Reads what jaw.db holds for each setting of each domain jawd knows: the terminal's.
    pub fn load(store: Store) -> anyhow::Result<Settings> {
        let mut held = BTreeMap::new();
        for key in KNOWN {
            if let Some(value) = store.setting(TERMINAL, key)? {
                held.insert(key, value);
            }
        }
        let (shared, ignored) = in_effect(&held);
        Ok(Settings {
            store,
            held: Mutex::new(held),
            published: watch::channel(shared).0,
            ignored: std::sync::Mutex::new(ignored),
        })
    }

    /// The terminal's settings in effect.
    pub fn current(&self) -> Shared {
        self.published.borrow().clone()
    }

    /// A receiver for the settings of `client`'s domain, which has seen those in effect now and
    /// wakes for each later change; none for a client jawd keeps no settings for.
    pub fn subscribe(&self, client: &str) -> Option<watch::Receiver<Shared>> {
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
    /// one synced commit, and only then publishes the domain's settings in effect and answers with
    /// them. What jaw.db holds already is answered at once, and written again never.
    pub async fn set(&self, client: &str, params: Value) -> Result<Shared, Error> {
        if client != TERMINAL {
            let mut error = Error::new(code::NOT_FOUND, format!("jawd keeps no settings for {client}"));
            error.data = Some(json!({ "client": client }));
            return Err(error);
        }
        let asked = checked(params)?;
        let mut held = self.held.lock().await;
        let at = now_ms();
        let values: Vec<(String, String)> = asked
            .iter()
            .filter(|(key, value)| held.get(key) != Some(value))
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect();
        if values.is_empty() {
            return Ok(self.current());
        }
        let change = Change::Settings { domain: TERMINAL.to_string(), values, at };
        match self.store.apply_durable(vec![change]).await {
            Ok(applied) if applied.quarantined == 0 => {}
            Ok(_) => return Err(Error::new(code::INTERNAL, "jawd couldn't keep that setting, so nothing changed")),
            Err(e) => return Err(Error::new(code::INTERNAL, format!("jawd couldn't keep that setting: {e:#}"))),
        }
        held.extend(asked);
        let (shared, ignored) = in_effect(&held);
        *self.ignored.lock().expect("the ignored settings") = ignored;
        self.published.send_replace(shared.clone());
        Ok(shared)
    }
}

/// `settings.set`'s params checked: at least one setting, each one jawd knows with a value it
/// takes, as the JSON jaw.db keeps.
fn checked(params: Value) -> Result<Vec<(&'static str, String)>, Error> {
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
    for (key, value) in asked {
        let Some(known) = KNOWN.into_iter().find(|k| *k == key) else {
            let mut error = Error::new(code::INVALID_PARAMS, format!("jawd doesn't know the setting {key}"));
            error.data = Some(json!({ "param": key }));
            return Err(error);
        };
        let allowed: Vec<&str> = match known {
            "theme" => themes::all().iter().map(|t| t.id.as_str()).collect(),
            _ => Messages::ALL.iter().map(|m| m.name()).collect(),
        };
        match value.as_str().filter(|v| allowed.contains(v)) {
            Some(v) => out.push((known, Value::from(v).to_string())),
            None => {
                let mut error = Error::new(code::INVALID_PARAMS, format!("{key} takes one of {}", allowed.join(", ")));
                error.data = Some(json!({ "param": key, "allowed": allowed }));
                return Err(error);
            }
        }
    }
    Ok(out)
}

/// The settings in effect from what jaw.db holds, and each value held that this jawd doesn't take,
/// whose setting has its default instead.
fn in_effect(held: &BTreeMap<&'static str, String>) -> (Shared, Vec<Ignored>) {
    let mut shared = Shared::default();
    let mut ignored = Vec::new();
    for (key, json) in held {
        let value: Value = serde_json::from_str(json).unwrap_or(Value::Null);
        let taken = match *key {
            "theme" => match value.as_str().filter(|id| themes::get(id).is_some()) {
                Some(id) => {
                    shared.theme = id.to_string();
                    true
                }
                None => false,
            },
            _ => match serde_json::from_value::<Messages>(value.clone()) {
                Ok(messages) => {
                    shared.messages = messages;
                    true
                }
                Err(_) => false,
            },
        };
        if !taken {
            ignored.push(Ignored { domain: TERMINAL.to_string(), key: key.to_string(), value });
        }
    }
    (shared, ignored)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jaw_proto::settings::{Messages, TERMINAL};
    use serde_json::json;

    fn opened() -> (tempfile::TempDir, Store, Settings) {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let settings = Settings::load(store.clone()).unwrap();
        (home, store, settings)
    }

    async fn refused(settings: &Settings, params: Value) -> Error {
        settings.set(TERMINAL, params).await.unwrap_err()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_change_is_checked_kept_with_a_synced_commit_and_then_published() {
        let (_home, store, settings) = opened();
        assert_eq!(settings.current(), Shared::default());
        let mut heard = settings.subscribe(TERMINAL).unwrap();
        let synced = store.storage().synced_commits;
        let now = settings.set(TERMINAL, json!({"theme": "gruvbox"})).await.unwrap();
        assert_eq!(now, Shared { theme: "gruvbox".into(), messages: Messages::Bubbles });
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
        let e = settings.set("test", json!({"theme": "gruvbox"})).await.unwrap_err();
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
        let e = refused(&settings, json!({"theme": "gruvbox", "messages": "sideways"})).await;
        assert_eq!(e.data.unwrap()["param"], "messages");
        assert_eq!(store.setting(TERMINAL, "theme").unwrap(), None, "a refused ask keeps none of its settings");
        assert_eq!(settings.current(), Shared::default());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_value_jaw_db_holds_already_is_answered_at_once_and_writes_nothing() {
        let (_home, store, settings) = opened();
        settings.set(TERMINAL, json!({"messages": "irc"})).await.unwrap();
        let synced = store.storage().synced_commits;
        let heard = settings.subscribe(TERMINAL).unwrap();
        let now = settings.set(TERMINAL, json!({"messages": "irc"})).await.unwrap();
        assert_eq!(now.messages, Messages::Irc);
        assert_eq!(store.storage().synced_commits, synced, "nothing written");
        assert!(!heard.has_changed().unwrap(), "and nothing published");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_restart_keeps_them_and_a_value_jawd_doesnt_take_is_kept_and_read_as_the_default() {
        let home = tempfile::tempdir().unwrap();
        {
            let settings = Settings::load(Store::open(home.path()).unwrap()).unwrap();
            settings.set(TERMINAL, json!({"theme": "nord", "messages": "irc"})).await.unwrap();
        }
        {
            let settings = Settings::load(Store::open(home.path()).unwrap()).unwrap();
            assert_eq!(settings.current(), Shared { theme: "nord".into(), messages: Messages::Irc });
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
        assert_eq!(settings.current(), Shared { theme: themes::DEFAULT.into(), messages: Messages::Irc });
        let ignored = Ignored { domain: TERMINAL.into(), key: "theme".into(), value: json!("rose-pine-moon") };
        assert_eq!(settings.ignored(), vec![ignored]);
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().as_deref(), Some("\"rose-pine-moon\""), "kept as it was");
        assert_eq!(store.count("settings").unwrap(), 4, "a setting or a domain jawd doesn't know is left alone");
        assert_eq!(settings.by_domain().keys().collect::<Vec<_>>(), [TERMINAL], "and not read");
        settings.set(TERMINAL, json!({"theme": "tokyo-night"})).await.unwrap();
        assert!(settings.ignored().is_empty(), "choosing the default replaces what wasn't understood");
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().as_deref(), Some("\"tokyo-night\""));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_change_that_cant_be_kept_publishes_nothing() {
        let (home, store, settings) = opened();
        let outside = rusqlite::Connection::open(home.path().join("jaw.db")).unwrap();
        outside
            .execute_batch("CREATE TRIGGER refused BEFORE INSERT ON settings BEGIN SELECT RAISE(ABORT, 'refused'); END")
            .unwrap();
        let heard = settings.subscribe(TERMINAL).unwrap();
        assert_eq!(refused(&settings, json!({"theme": "nord"})).await.code, code::INTERNAL);
        assert_eq!(settings.current(), Shared::default());
        assert!(!heard.has_changed().unwrap(), "nothing published");
        assert_eq!(store.setting(TERMINAL, "theme").unwrap(), None);
        outside.execute_batch("DROP TRIGGER refused").unwrap();
        assert_eq!(
            settings.set(TERMINAL, json!({"theme": "nord"})).await.unwrap().theme,
            "nord",
            "and it can be asked again"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn changes_from_many_interfaces_at_once_end_with_jaw_db_and_what_was_published_agreeing() {
        let (_home, store, settings) = opened();
        let settings = std::sync::Arc::new(settings);
        let mut heard = settings.subscribe(TERMINAL).unwrap();
        let asks: Vec<_> = (0..24)
            .map(|i| {
                let settings = settings.clone();
                let theme = themes::all()[i % 23].id.clone();
                let messages = if i % 2 == 0 { "irc" } else { "bubbles" };
                tokio::spawn(async move {
                    settings.set(TERMINAL, json!({"theme": theme, "messages": messages})).await.unwrap()
                })
            })
            .collect();
        for ask in asks {
            ask.await.unwrap();
        }
        let now = settings.current();
        assert_eq!(*heard.borrow_and_update(), now, "the last published is the one in effect");
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().unwrap(), serde_json::to_string(&now.theme).unwrap());
        assert_eq!(
            store.setting(TERMINAL, "messages").unwrap().unwrap(),
            serde_json::to_string(&now.messages).unwrap()
        );
    }
}
