//! Folders and favourites as interfaces ask for them: `folder.create`, `folder.rename`,
//! `folder.move`, `folder.delete`, `folder.add`, `folder.remove`, `folder.smart` and
//! `favourite.set`. Each is checked here as far as its params go, then made by the writer in one
//! transaction that reaches the disk before it's answered, which checks it against the tree as it
//! is then; what the writer refuses comes back as the protocol's error, and nothing is written.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use jaw_proto::{Error, FoldersStatus, code};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::feed;
use crate::store::folders::{Done, Item, Outcome, Step, Target};
use crate::store::lists::{self, List, Rule};
use crate::store::{Change, Lane, Store};
use crate::work::Priority;

pub const METHODS: &[&str] = &[
    "folder.create",
    "folder.rename",
    "folder.move",
    "folder.delete",
    "folder.add",
    "folder.remove",
    "folder.smart",
    "favourite.set",
];

/// The longest name a folder can have, in chars.
const NAME_MOST: usize = 256;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    name: String,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    rule: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rename {
    folder: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Move {
    folder: String,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    position: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Named {
    folder: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Filing {
    folder: String,
    item: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Smart {
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    name: Option<String>,
    rule: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Favourite {
    slot: u8,
    #[serde(default)]
    target: Option<String>,
}

fn invalid(param: &str, message: impl Into<String>) -> Error {
    let mut error = Error::new(code::INVALID_PARAMS, message);
    error.data = Some(json!({ "param": param }));
    error
}

fn params_of<T: for<'de> Deserialize<'de>>(method: &str, params: Value) -> Result<T, Error> {
    let params = if params.is_null() { json!({}) } else { params };
    serde_json::from_value(params).map_err(|e| Error::new(code::INVALID_PARAMS, format!("{method}'s params: {e}")))
}

fn name_of(name: &str) -> Result<String, Error> {
    let name = name.trim();
    if name.is_empty() {
        return Err(invalid("name", "a folder needs a name"));
    }
    if name.chars().count() > NAME_MOST || name.chars().any(char::is_control) {
        return Err(invalid("name", format!("a folder's name is at most {NAME_MOST} characters, on one line")));
    }
    Ok(name.to_string())
}

fn rule_of(rule: &Value) -> Result<String, Error> {
    let rule = Rule::of(rule).map_err(|e| invalid("rule", format!("{e:#}")))?;
    Ok(serde_json::to_string(&rule).expect("a rule serializes"))
}

/// A folder of yours or a smart one, by its ID; a built-in one is refused with `built_in`.
fn folder_n(id: &str, param: &str, built_in: &str) -> Result<i64, Error> {
    match List::parse(id) {
        Some(List::Folder(n) | List::Smart(n)) => Ok(n),
        Some(_) => Err(Error::new(code::NOT_ALLOWED, built_in)),
        None => Err(invalid(param, format!("{id} isn't a folder's ID"))),
    }
}

/// Where a folder goes: the top, or a folder of yours.
fn parent_n(id: Option<&str>) -> Result<Option<i64>, Error> {
    match id.map(List::parse) {
        None => Ok(None),
        Some(Some(List::Folder(n))) => Ok(Some(n)),
        Some(Some(_)) => Err(Error::new(code::NOT_ALLOWED, "folders go only in folders of yours")),
        Some(None) => Err(invalid("parent", format!("{} isn't a folder's ID", id.unwrap_or_default()))),
    }
}

fn item_of(item: &str) -> Result<Item, Error> {
    match item.strip_prefix("person:") {
        Some(n) => n.parse().map(Item::Person).map_err(|_| invalid("item", format!("{item} isn't a person's ID"))),
        None if item.contains('@') => Ok(Item::Chat(item.to_string())),
        None => Err(invalid("item", format!("{item} is neither a chat's ID nor a person's"))),
    }
}

/// Makes `step` durably and says how it went, with the folder it made, if it made one.
async fn made(store: &Store, step: Step, done: Done) -> Result<Option<i64>, Error> {
    let applied = store
        .apply_durable(vec![Change::Folders(step)])
        .await
        .map_err(|e| Error::new(code::INTERNAL, format!("jawd couldn't change its folders: {e:#}")))?;
    match done.take() {
        Some(Outcome::Done(made)) => Ok(made),
        Some(Outcome::Refused(code, message)) => Err(Error::new(code, message)),
        None => Err(Error::new(
            code::INTERNAL,
            format!("jawd couldn't change its folders ({} kept in quarantine)", applied.quarantined),
        )),
    }
}

/// Answers one of METHODS.
pub async fn call(store: &Store, method: &str, params: Value) -> Result<Value, Error> {
    let done = Done::default();
    match method {
        "folder.create" => {
            let create: Create = params_of(method, params)?;
            let name = name_of(&create.name)?;
            let parent = parent_n(create.parent.as_deref())?;
            let rule = create.rule.as_ref().map(rule_of).transpose()?;
            let made = made(store, Step::Create { name, parent, rule, done: done.clone() }, done).await?;
            folder(store, made).await
        }
        "folder.rename" => {
            let rename: Rename = params_of(method, params)?;
            let folder = folder_n(&rename.folder, "folder", "the built-in folders keep their names")?;
            let name = name_of(&rename.name)?;
            made(store, Step::Rename { folder, name, done: done.clone() }, done).await?;
            Ok(json!({}))
        }
        "folder.move" => {
            let moving: Move = params_of(method, params)?;
            let folder = folder_n(&moving.folder, "folder", "the built-in folders stay where they are")?;
            let parent = parent_n(moving.parent.as_deref())?;
            let step = Step::Move { folder, parent, position: moving.position, done: done.clone() };
            made(store, step, done).await?;
            Ok(json!({}))
        }
        "folder.delete" => {
            let named: Named = params_of(method, params)?;
            let folder = folder_n(&named.folder, "folder", "the built-in folders stay")?;
            made(store, Step::Delete { folder, done: done.clone() }, done).await?;
            Ok(json!({}))
        }
        "folder.add" | "folder.remove" => {
            let filing: Filing = params_of(method, params)?;
            let folder = folder_n(
                &filing.folder,
                "folder",
                "the built-in folders hold what they find, so nothing is filed in them",
            )?;
            let item = item_of(&filing.item)?;
            let step = if method == "folder.add" {
                Step::File { folder, item, done: done.clone() }
            } else {
                Step::Unfile { folder, item, done: done.clone() }
            };
            made(store, step, done).await?;
            Ok(json!({}))
        }
        "folder.smart" => {
            let smart: Smart = params_of(method, params)?;
            let rule = rule_of(&smart.rule)?;
            match (smart.folder, smart.name) {
                (Some(id), _) => {
                    let folder = folder_n(&id, "folder", "the built-in folders have no rule")?;
                    made(store, Step::Rule { folder, rule, done: done.clone() }, done).await?;
                    Ok(json!({}))
                }
                (None, Some(name)) => {
                    let name = name_of(&name)?;
                    let step = Step::Create { name, parent: None, rule: Some(rule), done: done.clone() };
                    let made = made(store, step, done).await?;
                    folder(store, made).await
                }
                (None, None) => {
                    Err(invalid("folder", "folder.smart changes a smart folder's rule, or makes one with a name"))
                }
            }
        }
        "favourite.set" => {
            let favourite: Favourite = params_of(method, params)?;
            if !(1..=9).contains(&favourite.slot) {
                return Err(invalid("slot", "a favourite's slot is 1 to 9"));
            }
            let target = match favourite.target.as_deref() {
                None => None,
                Some(id) => Some(match List::parse(id) {
                    Some(List::Folder(n) | List::Smart(n)) => Target::Folder(n),
                    Some(builtin) => Target::Folder(builtin_n(store, builtin).await?),
                    None if id.contains('@') => Target::Chat(id.to_string()),
                    None => return Err(invalid("target", format!("{id} is neither a folder's ID nor a chat's"))),
                }),
            };
            made(store, Step::Favourite { slot: favourite.slot, target, done: done.clone() }, done).await?;
            Ok(json!({}))
        }
        _ => Err(Error::new(code::UNKNOWN_METHOD, format!("jawd doesn't know {method}"))),
    }
}

/// A built-in folder's n in jaw.db.
async fn builtin_n(store: &Store, list: List) -> Result<i64, Error> {
    let id = list_id(&list);
    store
        .reads()
        .query(Priority::Interactive, move |db| {
            Ok(db.query_row("SELECT n FROM folders WHERE builtin = ?1", [id], |r| r.get::<_, i64>(0))?)
        })
        .await
        .map_err(|e| Error::new(code::INTERNAL, format!("jawd couldn't find the folder: {e:#}")))
}

fn list_id(list: &List) -> &'static str {
    match list {
        List::All => "all",
        List::Archived => "archived",
        List::Unread => "unread",
        List::Groups => "groups",
        List::Communities => "communities",
        List::Folder(_) | List::Smart(_) => "",
    }
}

/// `{"folder": row}` for the folder made, as folders.watch sends it.
async fn folder(store: &Store, made: Option<i64>) -> Result<Value, Error> {
    let Some(n) = made else { return Ok(json!({})) };
    let rows =
        store.reads().query(Priority::Interactive, move |db| lists::folders(db, crate::feed::now_ms())).await.map_err(
            |e| Error::new(code::INTERNAL, format!("jawd made the folder but couldn't read it back: {e:#}")),
        )?;
    let row = rows.into_iter().find(|row| row.n == n).map(feed::folder_row);
    Ok(json!({ "folder": row }))
}

/// How long after jawd starts the folders' counts are first checked, and how often after that.
const CHECK_FIRST: Duration = Duration::from_secs(60);
const CHECK_EVERY: Duration = Duration::from_secs(3600);

/// The background check of the folders' counts, as `jawd status` reports it.
#[derive(Default)]
pub struct Checks {
    checks: AtomicU64,
    drifted: AtomicU64,
    last: Mutex<Option<String>>,
}

impl Checks {
    pub fn status(&self) -> FoldersStatus {
        FoldersStatus {
            checks: self.checks.load(Ordering::Relaxed),
            drifted: self.drifted.load(Ordering::Relaxed),
            last_drift: self.last.lock().expect("the folders' checks").clone(),
        }
    }
}

/// Checks the folders' counts against what they're derived from, a minute after jawd starts and
/// hourly after that, on a reader in the background; drifted, they're built again from it.
pub async fn check(store: Store, checks: std::sync::Arc<Checks>) {
    tokio::time::sleep(CHECK_FIRST).await;
    loop {
        if let Err(e) = check_once(&store, &checks).await {
            eprintln!("jawd: can't check the folders' counts: {e:#}");
        }
        tokio::time::sleep(CHECK_EVERY).await;
    }
}

/// One check, mending what it finds; says what it found.
pub async fn check_once(store: &Store, checks: &Checks) -> anyhow::Result<Vec<String>> {
    let drift = store.reads().query(Priority::Background, lists::drift).await?;
    checks.checks.fetch_add(1, Ordering::Relaxed);
    if drift.is_empty() {
        return Ok(drift);
    }
    let said = drift.join("; ");
    eprintln!("jawd: the folders' counts have drifted from what they count, so they're built again: {said}");
    checks.drifted.fetch_add(1, Ordering::Relaxed);
    *checks.last.lock().expect("the folders' checks") = Some(said);
    store.apply(Lane::Background, vec![Change::Folders(Step::Rebuild)]).await?;
    let again = store.reads().query(Priority::Background, lists::drift).await?;
    if !again.is_empty() {
        eprintln!("jawd: the folders' counts still differ once built again: {}", again.join("; "));
    }
    Ok(drift)
}
