//! Each frontend's settings, which jawd keeps in jaw.db in a domain of its own, named by the
//! `client` its hello gives. The terminal's, in the `jaw` domain, are the theme and where your
//! messages sit. protocol/README.md's "Settings" says how they're read, changed and pushed.

use serde::{Deserialize, Serialize};
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
/// answer and the `settings` push carry them to a connection of the `jaw` domain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// An Omarchy theme's ID, as `themes` lists them.
    pub theme: String,
    pub messages: Messages,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { theme: crate::themes::DEFAULT.to_string(), messages: Messages::default() }
    }
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
    fn the_terminals_domain_is_the_client_its_hello_gives_and_a_hello_for_another_carries_no_settings() {
        assert_eq!(TERMINAL, "jaw");
        let hello: crate::Welcome = serde_json::from_str(
            r#"{"protocol":1,"jawd":"0.1.0","root":"/r","seq":1,"account":{"state":"unlinked"},"connection":{"state":"offline"}}"#,
        )
        .unwrap();
        assert_eq!(hello.settings, None);
        let ignored = Ignored { domain: TERMINAL.into(), key: "theme".into(), value: serde_json::json!("x") };
        assert_eq!(serde_json::to_value(ignored).unwrap()["domain"], "jaw");
    }
}
