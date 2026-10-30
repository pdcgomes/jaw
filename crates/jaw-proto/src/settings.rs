//! Each frontend's settings, which jawd keeps in jaw.db in a domain of its own, named by the
//! `client` its hello gives. The terminal's, in the `jaw` domain, are the theme and where your
//! messages sit. protocol/README.md's "Settings" says how they're read, changed and pushed.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// The terminal's domain: the `client` its hello gives, which `jaw settings` and `jaw status` give
/// too.
pub const TERMINAL: &str = "jaw";

/// Where your own messages sit in a conversation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Messages {
    /// In a column from a third of the way across.
    #[default]
    Bubbles,
    /// On the left under "You", as everyone's are, as IRC clients put them.
    Irc,
}

impl Messages {
    pub const ALL: [Messages; 2] = [Messages::Bubbles, Messages::Irc];

    /// Its name on the wire.
    pub fn name(self) -> &'static str {
        match self {
            Messages::Bubbles => "bubbles",
            Messages::Irc => "irc",
        }
    }
}

/// The terminal's settings, each with the value in effect, as hello's answer, `settings.set`'s
/// answer and the `settings` push carry them to a connection of the `jaw` domain. A setting an
/// older jawd leaves out reads as its default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// An Omarchy theme's ID, as `themes` lists them.
    pub theme: String,
    #[serde(deserialize_with = "messages_or_default")]
    pub messages: Messages,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { theme: crate::themes::DEFAULT.to_string(), messages: Messages::default() }
    }
}

/// `messages` as jaw reads it from jawd: a value it doesn't know, as a newer jawd may send, is the
/// default. `Messages` alone refuses one, so jawd can tell a value in jaw.db it doesn't take.
fn messages_or_default<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Messages, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(deserializer)?).unwrap_or_default())
}

/// `settings.set`'s params: the settings to change, at least one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsSet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messages: Option<Messages>,
}

/// `settings.set`'s result: every setting as it now is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsKept {
    pub settings: Settings,
}

/// A setting whose value in jaw.db this jawd doesn't take, so its default is in effect: its domain,
/// its key and that value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ignored {
    pub domain: String,
    pub key: String,
    pub value: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_tokyo_night_with_yours_on_the_right_and_a_change_names_only_what_it_changes() {
        assert_eq!(Settings::default(), Settings { theme: "tokyo-night".into(), messages: Messages::Bubbles });
        assert_eq!(Messages::ALL.map(Messages::name), ["bubbles", "irc"]);
        let change = SettingsSet { theme: Some("gruvbox".into()), messages: None };
        assert_eq!(serde_json::to_value(change).unwrap(), serde_json::json!({"theme": "gruvbox"}));
    }

    #[test]
    fn messages_writes_each_value_by_its_name_and_refuses_any_other() {
        for value in Messages::ALL {
            assert_eq!(serde_json::to_value(value).unwrap(), value.name());
        }
        assert!(serde_json::from_value::<Messages>(serde_json::json!("compact")).is_err());
    }

    #[test]
    fn settings_read_a_messages_value_jaw_doesnt_know_as_the_default() {
        let settings: Settings = serde_json::from_str(r#"{"theme":"x","messages":"compact"}"#).unwrap();
        assert_eq!(settings, Settings { theme: "x".into(), messages: Messages::Bubbles });
    }

    #[test]
    fn a_setting_an_older_jawd_leaves_out_reads_as_its_default() {
        assert_eq!(serde_json::from_str::<Settings>("{}").unwrap(), Settings::default());
    }

    #[test]
    fn the_terminals_domain_is_jaw_and_an_ignored_setting_says_its_domain() {
        assert_eq!(TERMINAL, "jaw");
        let ignored = Ignored { domain: TERMINAL.into(), key: "theme".into(), value: serde_json::json!("x") };
        assert_eq!(serde_json::to_value(ignored).unwrap()["domain"], "jaw");
    }

    #[test]
    fn hello_and_status_with_no_settings_leave_them_out_rather_than_sending_them_empty() {
        let hello: crate::Welcome = serde_json::from_value(serde_json::json!({
            "protocol": crate::PROTOCOL, "jawd": "0.1.0", "root": "/r", "seq": 1,
            "account": {"state": "unlinked"}, "connection": {"state": "offline"}
        }))
        .unwrap();
        assert_eq!(hello.settings, None);
        assert_eq!(serde_json::to_value(&hello).unwrap().get("settings"), None);
        let status: crate::Status = serde_json::from_value(serde_json::json!({
            "jawd": "0.1.0", "protocol": crate::PROTOCOL, "root": "/r", "account": {"state": "unlinked"},
            "connection": {"state": "offline"}, "engine": {"kind": "go", "state": "running"}
        }))
        .unwrap();
        assert!(status.settings.is_empty() && status.settings_ignored.is_empty());
        let status = serde_json::to_value(&status).unwrap();
        assert_eq!((status.get("settings"), status.get("settings_ignored")), (None, None));
    }
}
