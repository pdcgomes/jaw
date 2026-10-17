//! What your actions still on their way to WhatsApp show as. A reaction, an edit or a deletion for
//! everyone is done to its message's row only once WhatsApp has it (write::outbox), so the row only
//! ever holds what WhatsApp has; until then a window shows it as done, from its outbox row, by the
//! rules the row will follow once it is: an edit newer than the row's, a reaction newer than yours
//! on it. A refused action leaves the outbox, and the row shows as it always was.

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::store::Amendment;

/// An action of yours on a message, as `MESSAGE_COLUMNS` reads it: what it does, when, and when
/// your reaction on the row is from, if it has one.
#[derive(Debug, Deserialize)]
struct Pending {
    at: i64,
    effect: Amendment,
    #[serde(default)]
    mine_at: Option<i64>,
}

/// What a message row shows, which actions still on their way may change.
pub(super) struct Shown<'a> {
    pub spans: &'a mut String,
    pub summary: &'a mut String,
    pub reply: &'a mut Option<String>,
    pub detail: &'a mut Option<String>,
    pub edited_at: &'a mut Option<i64>,
    pub deleted: &'a mut bool,
    pub reactions: &'a mut String,
}

/// What a message you deleted for everyone shows.
const YOU_DELETED: &str = "You deleted this message";

fn pending(actions: Option<&str>) -> Vec<Pending> {
    match actions {
        None | Some("[]") => Vec::new(),
        Some(actions) => serde_json::from_str(actions).unwrap_or_default(),
    }
}

/// Shows `actions` (a JSON array, oldest first) over a message row.
pub(super) fn message(shown: Shown, actions: Option<&str>) {
    for action in pending(actions) {
        match action.effect {
            Amendment::Revoke => {
                *shown.deleted = true;
                *shown.spans = "[]".into();
                *shown.summary = YOU_DELETED.into();
                *shown.reply = None;
                *shown.detail = None;
            }
            Amendment::Edit { text, spans, summary }
                if !*shown.deleted && shown.edited_at.is_none_or(|edited| action.at > edited) =>
            {
                *shown.spans = spans.unwrap_or_else(|| {
                    let plain = text.map(|s| vec![serde_json::json!({ "t": "text", "s": s })]).unwrap_or_default();
                    Value::Array(plain).to_string()
                });
                *shown.summary = summary;
                *shown.edited_at = Some(action.at);
            }
            Amendment::React { emoji } if action.mine_at.is_none_or(|mine| action.at >= mine) => {
                *shown.reactions = reacted(shown.reactions, &emoji);
            }
            _ => {}
        }
    }
}

/// `reactions`, each emoji with how many and whether one's yours, with yours now `emoji`, or none
/// when it's empty; a new emoji goes last, as the first of it is newest.
fn reacted(reactions: &str, emoji: &str) -> String {
    let mut all: Vec<Map<String, Value>> = serde_json::from_str(reactions).unwrap_or_default();
    for reaction in all.iter_mut().filter(|r| r.get("mine") == Some(&Value::Bool(true))) {
        let n = reaction.get("n").and_then(Value::as_u64).unwrap_or(1);
        reaction.insert("n".into(), (n.saturating_sub(1)).into());
        reaction.insert("mine".into(), false.into());
    }
    all.retain(|r| r.get("n").and_then(Value::as_u64) != Some(0));
    if !emoji.is_empty() {
        match all.iter_mut().find(|r| r.get("emoji").and_then(Value::as_str) == Some(emoji)) {
            Some(reaction) => {
                let n = reaction.get("n").and_then(Value::as_u64).unwrap_or(0);
                reaction.insert("n".into(), (n + 1).into());
                reaction.insert("mine".into(), true.into());
            }
            None => {
                let mut mine = Map::new();
                mine.insert("emoji".into(), emoji.into());
                mine.insert("n".into(), 1.into());
                mine.insert("mine".into(), true.into());
                all.push(mine);
            }
        }
    }
    Value::Array(all.into_iter().map(Value::Object).collect()).to_string()
}

/// What a chat's newest message says in the chat list, with your actions on it still on their way.
pub(super) fn last(summary: &mut String, edited_at: Option<i64>, actions: Option<&str>) {
    let mut edited_at = edited_at;
    let mut deleted = false;
    for action in pending(actions) {
        match action.effect {
            Amendment::Revoke => {
                deleted = true;
                *summary = YOU_DELETED.into();
            }
            Amendment::Edit { summary: edited, .. } if !deleted && edited_at.is_none_or(|at| action.at > at) => {
                *summary = edited;
                edited_at = Some(action.at);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn your_reaction_on_its_way_takes_the_place_of_yours_and_an_empty_one_takes_it_away() {
        let two = r#"[{"emoji":"👍","n":2,"mine":true},{"emoji":"❤️","n":1,"mine":false}]"#;
        assert_eq!(reacted(two, "❤️"), r#"[{"emoji":"👍","mine":false,"n":1},{"emoji":"❤️","mine":true,"n":2}]"#);
        assert_eq!(reacted(two, ""), r#"[{"emoji":"👍","mine":false,"n":1},{"emoji":"❤️","mine":false,"n":1}]"#);
        assert_eq!(reacted("[]", "😂"), r#"[{"emoji":"😂","mine":true,"n":1}]"#);
        let only_mine = r#"[{"emoji":"👍","n":1,"mine":true}]"#;
        assert_eq!(reacted(only_mine, ""), "[]");
    }
}
