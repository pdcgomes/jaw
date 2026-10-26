//! Each interface's layout of split panes, kept in jaw.db as jaw's own under the name the
//! interface gives in hello, so it opens where it was left and another interface can open it:
//! `layout.get` and `layout.set`, and the `layout` push to the interface's other connections.
//!
//! A layout is checked, then kept with a synced commit before `layout.set` answers. The version
//! the interface gives decides between two layouts: only a newer one replaces what's kept, so
//! asking again, or late, changes nothing, and a stale one is answered with the newer layout.
//! Each layout written is read back once committed, and published if it's still what's kept, by
//! the set that wrote it; each interface's channel holds the newest published, never an older one
//! after it, however sets' reads and publishes interleave, so a connection, however slow, ends on
//! what's kept. Panes name chats by JID, read as jawd knows them now: the chat a JID is merged
//! into, or the same person's chat under their other JID; a pane whose chat is gone, or that shows
//! the chat of a pane before it, is left out.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use jaw_proto::layout::{
    BYTES_MOST, FORMAT, KEPT_MOST, Layout, LayoutGet, LayoutSet, NAME_MOST, Node, PANES_MOST, Saved, Split,
};
use jaw_proto::{Error, code};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::watch;

use crate::store::layout::{Keep, Keeping, Kept};
use crate::store::{Change, Store, read};
use crate::work::Priority;

/// What an interface's layout is kept under in `interface_state`.
const KEY: &str = "layout";

/// A layout just kept, as the interface's connections are pushed it, and the connection that set
/// it, which isn't.
#[derive(Clone, Debug)]
pub struct Pushed {
    pub saved: Saved,
    pub from: u64,
}

#[derive(Clone)]
pub struct Layouts {
    inner: Arc<Inner>,
}

struct Inner {
    store: Store,
    /// Each interface's channel to its connections; one no connection listens to goes the next
    /// time a connection says hello, so there's never more than one for each connection.
    channels: Mutex<HashMap<String, watch::Sender<Option<Pushed>>>>,
    /// Layouts refused since jawd started for being past a limit.
    refused: AtomicU64,
}

impl Layouts {
    pub fn new(store: Store) -> Layouts {
        Layouts { inner: Arc::new(Inner { store, channels: Mutex::default(), refused: AtomicU64::new(0) }) }
    }

    /// What a connection greeted as `interface` hears of its interface's layout from now on; none
    /// for a name no layout can be kept under.
    pub fn subscribe(&self, interface: &str) -> Option<watch::Receiver<Option<Pushed>>> {
        named(interface).ok()?;
        let mut channels = self.inner.channels.lock().expect("the layouts' channels");
        channels.retain(|_, channel| channel.receiver_count() > 0);
        Some(channels.entry(interface.to_string()).or_insert_with(|| watch::channel(None).0).subscribe())
    }

    /// `layout.get`: the layout kept for the interface asked for, or the connection's own, `own`.
    pub async fn get(&self, own: &str, params: Value) -> Result<Saved, Error> {
        let asked: LayoutGet = params_of("layout.get", params)?;
        let interface = asked.interface.unwrap_or_else(|| own.to_string());
        named(&interface)?;
        Ok(self.read(interface).await?.0)
    }

    /// `layout.set`, from the connection `from`, greeted as `interface`: answered once what it
    /// came to is on disk, with the layout kept then.
    pub async fn set(&self, interface: &str, from: u64, params: Value) -> Result<Saved, Error> {
        let LayoutSet { v, layout } = params_of("layout.set", params)?;
        named(interface)?;
        let v = i64::try_from(v).ok().filter(|&v| v > 0).ok_or_else(|| {
            invalid("layout.v", "a layout's version is a whole number from 1, such as the time in milliseconds")
        })?;
        let value = self.checked(&layout)?;
        let written = self.keep(interface, &value, v).await?;
        self.answer(interface, from, &value, v, written).await
    }

    /// Keeps `value` for `interface` with a synced commit, unless what's kept is as new; says
    /// whether it was written.
    async fn keep(&self, interface: &str, value: &str, v: i64) -> Result<bool, Error> {
        let kept = Kept::default();
        let keep = Keep {
            interface: interface.to_string(),
            key: KEY,
            value: value.to_string(),
            v,
            at: now_ms(),
            kept: kept.clone(),
        };
        let applied = self
            .inner
            .store
            .apply_durable(vec![Change::Layout(keep)])
            .await
            .map_err(|e| Error::new(code::INTERNAL, format!("jawd couldn't keep the layout: {e:#}")))?;
        if applied.quarantined > 0 {
            return Err(Error::new(code::INTERNAL, "jawd couldn't keep the layout; what failed is kept in quarantine"));
        }
        match kept.take() {
            Some(Keeping::Full { kept }) => {
                self.inner.refused.fetch_add(1, Ordering::Relaxed);
                Err(Error::new(
                    code::LIMIT,
                    format!("jawd keeps layouts for {KEPT_MOST} interfaces at most, and {kept} have one"),
                ))
            }
            Some(Keeping::Written) => Ok(true),
            Some(Keeping::Unchanged) | None => Ok(false),
        }
    }

    /// The answer to a set of `value` at `v`, once committed: the layout kept now, published to the
    /// interface's connections if this set wrote it and it's still what's kept. A newer one
    /// committed since is published by the set that wrote it, as its own.
    async fn answer(&self, interface: &str, from: u64, value: &str, v: i64, written: bool) -> Result<Saved, Error> {
        let (mut saved, stored) = self.read(interface.to_string()).await?;
        let own = stored.is_some_and(|(kept, at)| kept == value && at == v);
        if written && own {
            self.publish(interface, Pushed { saved: saved.clone(), from });
        }
        saved.kept = Some(own);
        Ok(saved)
    }

    /// The layouts kept and the limits on them, for `status`.
    pub fn status(&self) -> jaw_proto::Layouts {
        let (kept, largest_bytes) = self.inner.store.layouts_kept().unwrap_or_default();
        jaw_proto::Layouts {
            kept,
            most: KEPT_MOST,
            largest_bytes,
            bytes_most: BYTES_MOST as u64,
            refused: self.inner.refused.load(Ordering::Relaxed),
        }
    }

    /// The layout as jawd keeps it, compact JSON, once it's known to be one jawd can keep.
    fn checked(&self, layout: &Layout) -> Result<String, Error> {
        if layout.format != FORMAT {
            let mut error = invalid(
                "layout.format",
                &format!("jawd keeps layouts in format {FORMAT}, and this one is in format {}", layout.format),
            );
            error.data = Some(json!({ "param": "layout.format", "allowed": [FORMAT] }));
            return Err(error);
        }
        let chats = layout.chats();
        if chats.len() > PANES_MOST {
            return Err(
                self.past(format!("a layout can have {PANES_MOST} panes at most, and this one has {}", chats.len()))
            );
        }
        if let Some(chat) = chats.iter().find(|chat| !jid(chat)) {
            return Err(invalid("layout.tree", &format!("a pane shows a chat by its JID, and {chat:?} isn't one")));
        }
        let mut seen = HashSet::new();
        if let Some(chat) = chats.iter().find(|chat| !seen.insert(**chat)) {
            return Err(invalid("layout.tree", &format!("a chat can be in one pane, and {chat} is in two")));
        }
        if !shares(&layout.tree) {
            return Err(invalid("layout.tree", "a split's first side takes 1 to 999 thousandths of it"));
        }
        if layout.focused as usize >= chats.len() {
            return Err(invalid(
                "layout.focused",
                &format!("the pane with the keys is one of the {} panes, counted from 0", chats.len()),
            ));
        }
        let value = serde_json::to_string(layout).expect("jaw-proto's types serialize");
        if value.len() > BYTES_MOST {
            return Err(
                self.past(format!("a layout can take {BYTES_MOST} bytes at most, and this one takes {}", value.len()))
            );
        }
        Ok(value)
    }

    /// A layout refused for being past a limit, counted for `status`.
    fn past(&self, why: String) -> Error {
        self.inner.refused.fetch_add(1, Ordering::Relaxed);
        Error::new(code::LIMIT, why)
    }

    /// What's kept for `interface`, as it's answered, and the row it came from.
    async fn read(&self, interface: String) -> Result<(Saved, Option<(String, i64)>), Error> {
        self.inner
            .store
            .reads()
            .query(Priority::Interactive, move |db| {
                let stored = read::interface_state(db, &interface, KEY)?;
                let mut saved = Saved { interface, v: 0, layout: None, kept: None, dropped: Vec::new() };
                if let Some((value, v)) = &stored {
                    saved.v = *v as u64;
                    if let Some(layout) = serde_json::from_str::<Layout>(value).ok().filter(|l| l.format == FORMAT) {
                        (saved.layout, saved.dropped) = resolved(layout, |jid| read::chat_named(db, jid))?;
                    }
                }
                Ok((saved, stored))
            })
            .await
            .map_err(|e| Error::new(code::INTERNAL, format!("jawd couldn't read the layout: {e:#}")))
    }

    /// Tells `interface`'s connections of a layout just kept, unless they've been told of a newer
    /// one already, as when another set read back after this one's commit published first.
    fn publish(&self, interface: &str, pushed: Pushed) {
        let channels = self.inner.channels.lock().expect("the layouts' channels");
        if let Some(channel) = channels.get(interface) {
            channel.send_if_modified(|told| {
                let newer = told.as_ref().is_none_or(|told| told.saved.v < pushed.saved.v);
                if newer {
                    *told = Some(pushed);
                }
                newer
            });
        }
    }
}

/// `layout` with each pane's chat as `named` knows it now, without the panes whose chat it doesn't
/// know or a pane before them shows, and the chats those named. The pane with the keys stays with
/// its chat; if that's left out, the keys go to the pane before it, or the first.
pub fn resolved(
    layout: Layout,
    named: impl FnMut(&str) -> anyhow::Result<Option<String>>,
) -> anyhow::Result<(Option<Layout>, Vec<String>)> {
    struct Walk<F> {
        named: F,
        shown: HashSet<String>,
        dropped: Vec<String>,
        at: u32,
        focused: u32,
        before: u32,
        focus_kept: bool,
    }
    fn walk<F: FnMut(&str) -> anyhow::Result<Option<String>>>(
        node: Node,
        w: &mut Walk<F>,
    ) -> anyhow::Result<Option<Node>> {
        match node {
            Node::Chat(jid) => {
                let at = w.at;
                w.at += 1;
                match (w.named)(&jid)?.filter(|chat| !w.shown.contains(chat)) {
                    Some(chat) => {
                        w.before += u32::from(at < w.focused);
                        w.focus_kept |= at == w.focused;
                        w.shown.insert(chat.clone());
                        Ok(Some(Node::Chat(chat)))
                    }
                    None => {
                        w.dropped.push(jid);
                        Ok(None)
                    }
                }
            }
            Node::Split(split) => {
                let Split { how, first, a, b } = *split;
                Ok(match (walk(a, w)?, walk(b, w)?) {
                    (Some(a), Some(b)) => Some(Node::Split(Box::new(Split { how, first, a, b }))),
                    (Some(one), None) | (None, Some(one)) => Some(one),
                    (None, None) => None,
                })
            }
        }
    }
    let mut w = Walk {
        named,
        shown: HashSet::new(),
        dropped: Vec::new(),
        at: 0,
        focused: layout.focused,
        before: 0,
        focus_kept: false,
    };
    let tree = walk(layout.tree, &mut w)?;
    let focused = if w.focus_kept { w.before } else { w.before.saturating_sub(1) };
    Ok((tree.map(|tree| Layout { format: layout.format, focused, tree }), w.dropped))
}

/// Whether every split in `node` gives its first side 1 to 999 thousandths.
fn shares(node: &Node) -> bool {
    match node {
        Node::Chat(_) => true,
        Node::Split(split) => (1..=999).contains(&split.first) && shares(&split.a) && shares(&split.b),
    }
}

/// Whether `chat` reads as a JID: a user and a server, and no spaces.
fn jid(chat: &str) -> bool {
    chat.split_once('@').is_some_and(|(user, server)| !user.is_empty() && !server.is_empty())
        && !chat.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Refuses a name no layout can be kept under.
fn named(interface: &str) -> Result<(), Error> {
    if interface.is_empty() || interface.len() > NAME_MOST {
        return Err(invalid(
            "interface",
            &format!("a layout is kept under the name an interface gives in hello, 1 to {NAME_MOST} bytes"),
        ));
    }
    Ok(())
}

fn invalid(param: &str, why: &str) -> Error {
    let mut error = Error::new(code::INVALID_PARAMS, why);
    error.data = Some(json!({ "param": param }));
    error
}

fn params_of<T: DeserializeOwned>(method: &str, params: Value) -> Result<T, Error> {
    let params = if params.is_null() { Value::Object(Default::default()) } else { params };
    serde_json::from_value(params).map_err(|e| Error::new(code::INVALID_PARAMS, format!("{method}'s params: {e}")))
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use jaw_proto::layout::How;
    use rusqlite::{Connection, OptionalExtension};

    use super::*;
    use crate::store::{Chat, Lane, Person};

    const ANA: &str = "351912345678@s.whatsapp.net";
    const ANA_LID: &str = "100000000001@lid";
    const RUI: &str = "351913333333@s.whatsapp.net";
    const RUI_LID: &str = "100000000003@lid";
    const CLIMBING: &str = "120363012345678901@g.us";
    const GONE: &str = "351919999999@s.whatsapp.net";

    fn layout(focused: u32, tree: Node) -> Layout {
        Layout { format: FORMAT, focused, tree }
    }

    fn chat(jid: &str) -> Node {
        Node::Chat(jid.into())
    }

    fn split(how: How, first: u16, a: Node, b: Node) -> Node {
        Node::Split(Box::new(Split { how, first, a, b }))
    }

    fn asked(v: u64, layout: &Layout) -> Value {
        json!({ "v": v, "layout": layout })
    }

    fn both() -> Layout {
        layout(1, split(How::Across, 500, chat(ANA), chat(CLIMBING)))
    }

    async fn opened(home: &Path, chats: &[&str]) -> (Store, Layouts) {
        let store = Store::open(home).unwrap();
        let made = chats.iter().map(|id| Change::Chat(Chat { id: id.to_string(), ..Chat::default() })).collect();
        store.apply(Lane::Live, made).await.unwrap();
        (store.clone(), Layouts::new(store))
    }

    fn updated_at(home: &Path, interface: &str) -> Option<i64> {
        let db = Connection::open(home.join("jaw.db")).unwrap();
        db.query_row("SELECT updated_at FROM interface_state WHERE interface = ?1", [interface], |r| r.get(0))
            .optional()
            .unwrap()
    }

    fn count(home: &Path, table: &str) -> i64 {
        let db = Connection::open(home.join("jaw.db")).unwrap();
        db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_layout_is_answered_only_once_a_synced_commit_has_it() {
        let home = tempfile::tempdir().unwrap();
        let (store, layouts) = opened(home.path(), &[ANA, CLIMBING]).await;
        // jaw.db's write lock, held from outside, so the layout can't be kept until it's let go.
        let holder = Connection::open(home.path().join("jaw.db")).unwrap();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();
        let synced = store.storage().synced_commits;
        let answer = {
            let layouts = layouts.clone();
            tokio::spawn(async move { layouts.set("jaw", 1, asked(5, &both())).await })
        };
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(!answer.is_finished(), "answered while jaw.db couldn't keep it");
        holder.execute_batch("COMMIT").unwrap();
        let saved = tokio::time::timeout(Duration::from_secs(10), answer).await.expect("answered once it could");
        let saved = saved.unwrap().unwrap();
        assert_eq!((saved.v, saved.kept, saved.layout), (5, Some(true), Some(both())));
        assert_eq!(store.storage().synced_commits - synced, 1, "kept with a synced commit");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn only_a_newer_layout_is_kept_and_pushed_so_asking_again_or_late_changes_nothing() {
        let home = tempfile::tempdir().unwrap();
        let (_store, layouts) = opened(home.path(), &[ANA, CLIMBING]).await;
        let mut other = layouts.subscribe("jaw").unwrap();
        let mac = layouts.subscribe("jaw-mac").unwrap();
        let one = layout(0, chat(ANA));
        let saved = layouts.set("jaw", 1, asked(10, &both())).await.unwrap();
        assert_eq!((saved.interface.as_str(), saved.v, saved.kept), ("jaw", 10, Some(true)));
        assert!(other.has_changed().unwrap(), "the terminal's other connection is told");
        let pushed = other.borrow_and_update().clone().unwrap();
        assert_eq!((pushed.from, pushed.saved.v, pushed.saved.kept), (1, 10, None));
        assert_eq!(pushed.saved.layout, Some(both()));
        assert!(!mac.has_changed().unwrap(), "another interface isn't");
        let at = updated_at(home.path(), "jaw");

        let again = layouts.set("jaw", 1, asked(10, &both())).await.unwrap();
        assert_eq!((again.v, again.kept, again.layout), (10, Some(true), Some(both())), "asked again: kept");
        let late = layouts.set("jaw", 2, asked(9, &one)).await.unwrap();
        assert_eq!((late.v, late.kept, late.layout), (10, Some(false), Some(both())), "an older one, late");
        let rival = layouts.set("jaw", 2, asked(10, &one)).await.unwrap();
        assert_eq!((rival.kept, rival.layout), (Some(false), Some(both())), "the first of a version stays");
        assert_eq!(updated_at(home.path(), "jaw"), at, "none of them changed the row");
        assert!(!other.has_changed().unwrap(), "so none was pushed");

        let newer = layouts.set("jaw", 2, asked(11, &one)).await.unwrap();
        assert_eq!((newer.v, newer.kept, newer.layout), (11, Some(true), Some(one.clone())));
        let pushed = other.borrow_and_update().clone().unwrap();
        assert_eq!((pushed.from, pushed.saved.v), (2, 11));
        let read = layouts.get("jaw-mac", json!({ "interface": "jaw" })).await.unwrap();
        assert_eq!((read.interface.as_str(), read.v, read.layout), ("jaw", 11, Some(one)), "the Mac app opens it");
        let own = layouts.get("jaw-mac", Value::Null).await.unwrap();
        assert_eq!((own.interface.as_str(), own.v, own.layout), ("jaw-mac", 0, None), "and has none of its own");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_layout_past_a_limit_is_refused_with_error_5_and_counted_and_a_wrong_one_names_its_param() {
        let home = tempfile::tempdir().unwrap();
        let (_store, layouts) = opened(home.path(), &[ANA, CLIMBING]).await;
        let refused = |e: Error| (e.code, e.data.and_then(|d| d["param"].as_str().map(str::to_string)));
        let mut later = layout(0, chat(ANA));
        later.format = 2;
        let e = layouts.set("jaw", 1, asked(1, &later)).await.unwrap_err();
        assert_eq!(e.data, Some(json!({ "param": "layout.format", "allowed": [1] })), "{}", e.message);
        assert_eq!(e.code, code::INVALID_PARAMS);
        let seventeen = (1..17).fold(chat("1@g.us"), |tree, n| split(How::Down, 500, tree, chat(&format!("{n}@g.us"))));
        let e = layouts.set("jaw", 1, asked(1, &layout(0, seventeen))).await.unwrap_err();
        assert_eq!(e.code, code::LIMIT, "{}", e.message);
        let long = format!("{}@s.whatsapp.net", "9".repeat(BYTES_MOST));
        let e = layouts.set("jaw", 1, asked(1, &layout(0, chat(&long)))).await.unwrap_err();
        assert_eq!(e.code, code::LIMIT, "{}", e.message);
        let wrong = [
            (asked(1, &layout(2, split(How::Across, 500, chat(ANA), chat(CLIMBING)))), "layout.focused"),
            (asked(1, &layout(0, split(How::Across, 0, chat(ANA), chat(CLIMBING)))), "layout.tree"),
            (asked(1, &layout(0, split(How::Across, 1000, chat(ANA), chat(CLIMBING)))), "layout.tree"),
            (asked(1, &layout(0, split(How::Across, 500, chat(ANA), chat(ANA)))), "layout.tree"),
            (asked(1, &layout(0, chat("Ana"))), "layout.tree"),
            (asked(1, &layout(0, chat("ana @s.whatsapp.net"))), "layout.tree"),
            (asked(0, &both()), "layout.v"),
            (asked(u64::MAX, &both()), "layout.v"),
        ];
        for (params, param) in wrong {
            let e = layouts.set("jaw", 1, params.clone()).await.unwrap_err();
            assert_eq!(refused(e), (code::INVALID_PARAMS, Some(param.to_string())), "{params}");
        }
        let e = layouts.set(&"x".repeat(NAME_MOST + 1), 1, asked(1, &both())).await.unwrap_err();
        assert_eq!(refused(e), (code::INVALID_PARAMS, Some("interface".into())));
        let e = layouts.get("jaw", json!({ "interface": "" })).await.unwrap_err();
        assert_eq!(refused(e), (code::INVALID_PARAMS, Some("interface".into())));
        assert_eq!((count(home.path(), "interface_state"), count(home.path(), "quarantine")), (0, 0), "nothing kept");
        let status = layouts.status();
        assert_eq!((status.kept, status.refused), (0, 2), "the panes and the bytes refused, as limits");
        assert_eq!((status.most, status.bytes_most), (KEPT_MOST, BYTES_MOST as u64));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_layout_asked_again_once_jawd_has_started_again_is_pushed_to_nobody() {
        let home = tempfile::tempdir().unwrap();
        let (store, layouts) = opened(home.path(), &[ANA, CLIMBING]).await;
        layouts.set("jaw", 1, asked(5, &both())).await.unwrap();
        // Started again, jawd has told no connection of any layout yet.
        let layouts = Layouts::new(store);
        let other = layouts.subscribe("jaw").unwrap();
        let again = layouts.set("jaw", 1, asked(5, &both())).await.unwrap();
        assert_eq!((again.v, again.kept), (5, Some(true)));
        assert!(!other.has_changed().unwrap(), "it changed nothing, so nothing is pushed");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_layout_published_after_a_newer_one_is_never_what_connections_are_told() {
        let home = tempfile::tempdir().unwrap();
        let (_store, layouts) = opened(home.path(), &[]).await;
        let mut jaw = layouts.subscribe("jaw").unwrap();
        let told = |v: u64, from: u64| Pushed {
            saved: Saved { interface: "jaw".into(), v, layout: Some(both()), kept: None, dropped: Vec::new() },
            from,
        };
        layouts.publish("jaw", told(11, 2));
        layouts.publish("jaw", told(10, 1));
        let latest = jaw.borrow_and_update().clone().unwrap();
        assert_eq!((latest.saved.v, latest.from), (11, 2), "the newer, though it was published first");
        layouts.publish("jaw", told(11, 1));
        assert!(!jaw.has_changed().unwrap(), "nor one of the same version");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_set_that_reads_back_a_newer_layout_publishes_nothing_and_the_set_that_wrote_it_publishes_it_as_its_own()
    {
        let home = tempfile::tempdir().unwrap();
        let (_store, layouts) = opened(home.path(), &[ANA, CLIMBING]).await;
        let mut told = layouts.subscribe("jaw").unwrap();
        let ten = serde_json::to_string(&both()).unwrap();
        let eleven = serde_json::to_string(&layout(0, chat(ANA))).unwrap();
        // Two sets at once, from connections 1 and 2: both commit before the first reads back.
        let first = layouts.keep("jaw", &ten, 10).await.unwrap();
        let second = layouts.keep("jaw", &eleven, 11).await.unwrap();
        assert_eq!((first, second), (true, true));
        let answered = layouts.answer("jaw", 1, &ten, 10, first).await.unwrap();
        assert_eq!((answered.v, answered.kept), (11, Some(false)), "answered with the newer one");
        assert!(!told.has_changed().unwrap(), "which it doesn't publish as connection 1's");
        layouts.answer("jaw", 2, &eleven, 11, second).await.unwrap();
        let latest = told.borrow_and_update().clone().unwrap();
        assert_eq!((latest.saved.v, latest.from), (11, 2), "connection 2's set does, so connection 1 is told");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn layouts_set_at_once_by_several_connections_end_each_on_the_newest_from_whoever_kept_it() {
        let home = tempfile::tempdir().unwrap();
        let (_store, layouts) = opened(home.path(), &[ANA, CLIMBING]).await;
        let mut told = layouts.subscribe("jaw").unwrap();
        const SETS: u64 = 48;
        // Four connections, each setting every fourth version, all at once.
        let sets: Vec<_> = (1..=SETS)
            .map(|v| {
                let layouts = layouts.clone();
                let tree = split(How::Across, 100 + v as u16, chat(ANA), chat(CLIMBING));
                tokio::spawn(async move { (v, layouts.set("jaw", v % 4 + 1, asked(v, &layout(1, tree))).await) })
            })
            .collect();
        for set in sets {
            let (v, answer) = set.await.unwrap();
            let answer = answer.unwrap();
            assert!(answer.v >= v, "answered with {v} or a newer one: {}", answer.v);
            assert_eq!(answer.kept, Some(answer.v == v), "kept says whether its own is what's kept");
        }
        let latest = told.borrow_and_update().clone().expect("the newest was published");
        assert_eq!((latest.saved.v, latest.from), (SETS, SETS % 4 + 1), "the newest, from the connection that set it");
        assert_eq!(layouts.get("jaw", Value::Null).await.unwrap().v, SETS);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_layout_jawd_couldnt_write_is_answered_with_an_error_kept_in_quarantine_and_pushed_to_nobody() {
        let home = tempfile::tempdir().unwrap();
        let (_store, layouts) = opened(home.path(), &[ANA, CLIMBING]).await;
        let mut other = layouts.subscribe("jaw").unwrap();
        let outside = Connection::open(home.path().join("jaw.db")).unwrap();
        outside
            .execute_batch(
                "CREATE TRIGGER refuse BEFORE INSERT ON interface_state BEGIN SELECT RAISE(ABORT, 'no room'); END",
            )
            .unwrap();
        let e = layouts.set("jaw", 1, asked(5, &both())).await.unwrap_err();
        assert_eq!(e.code, code::INTERNAL, "{}", e.message);
        assert_eq!((count(home.path(), "interface_state"), count(home.path(), "quarantine")), (0, 1));
        assert!(!other.has_changed().unwrap(), "nothing was kept, so nothing is pushed");
        outside.execute_batch("DROP TRIGGER refuse").unwrap();
        let again = layouts.set("jaw", 1, asked(5, &both())).await.unwrap();
        assert_eq!((again.v, again.kept), (5, Some(true)));
        assert_eq!(other.borrow_and_update().as_ref().map(|p| p.saved.v), Some(5));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn past_16_interfaces_a_new_one_is_refused_with_error_5_and_those_kept_still_change() {
        let home = tempfile::tempdir().unwrap();
        let (_store, layouts) = opened(home.path(), &[ANA, CLIMBING]).await;
        for n in 0..KEPT_MOST {
            layouts.set(&format!("app {n}"), 1, asked(1, &both())).await.unwrap();
        }
        let e = layouts.set("one more", 1, asked(1, &both())).await.unwrap_err();
        assert_eq!(e.code, code::LIMIT, "{}", e.message);
        assert_eq!(layouts.get("one more", Value::Null).await.unwrap().v, 0);
        let changed = layouts.set("app 0", 1, asked(2, &layout(0, chat(ANA)))).await.unwrap();
        assert_eq!((changed.v, changed.kept), (2, Some(true)));
        let bytes = serde_json::to_string(&both()).unwrap().len() as u64;
        let status = layouts.status();
        assert_eq!((status.kept, status.largest_bytes, status.refused), (KEPT_MOST, bytes, 1));
        assert_eq!(count(home.path(), "quarantine"), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_pane_reads_its_chat_as_jawd_knows_it_now_and_one_whose_chat_is_gone_or_shown_before_is_left_out() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let ana = Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() };
        let rui = Change::Chat(Chat { id: RUI.into(), kind: Some("direct"), ..Chat::default() });
        let rui_lid = Change::Chat(Chat { id: RUI_LID.into(), kind: Some("direct"), ..Chat::default() });
        let changes = vec![
            Change::Person(ana),
            Change::Chat(Chat { id: ANA.into(), kind: Some("direct"), ..Chat::default() }),
            Change::Chat(Chat { id: CLIMBING.into(), ..Chat::default() }),
            rui,
            rui_lid,
        ];
        store.apply(Lane::Live, changes).await.unwrap();
        let layouts = Layouts::new(store.clone());
        // Ana by her LID, the group, a chat jaw doesn't have, and the pane with the keys on it.
        let four = layout(
            2,
            split(
                How::Across,
                400,
                chat(ANA_LID),
                split(How::Down, 600, chat(CLIMBING), split(How::Across, 500, chat(GONE), chat(RUI_LID))),
            ),
        );
        let saved = layouts.set("jaw", 1, asked(1, &four)).await.unwrap();
        let three = layout(1, split(How::Across, 400, chat(ANA), split(How::Down, 600, chat(CLIMBING), chat(RUI_LID))));
        assert_eq!(
            (saved.kept, saved.layout.as_ref(), saved.dropped.as_slice()),
            (Some(true), Some(&three), &[GONE.to_string()][..])
        );
        // Rui's number and LID turn out to be one person, so their chats merge into one.
        let both_rui = layout(1, split(How::Across, 500, chat(RUI), chat(RUI_LID)));
        layouts.set("jaw", 1, asked(2, &both_rui)).await.unwrap();
        store
            .apply(
                Lane::Live,
                vec![Change::Person(Person { jids: vec![RUI.into(), RUI_LID.into()], ..Person::default() })],
            )
            .await
            .unwrap();
        let kept: String = {
            let db = Connection::open(home.path().join("jaw.db")).unwrap();
            db.query_row("SELECT k.id FROM chat_merges m JOIN chats k ON k.n = m.keep", [], |r| r.get(0))
                .optional()
                .unwrap()
                .unwrap_or_else(|| {
                    db.query_row("SELECT id FROM chats WHERE id IN (?1, ?2)", [RUI, RUI_LID], |r| r.get(0)).unwrap()
                })
        };
        let read = layouts.get("jaw", Value::Null).await.unwrap();
        assert_eq!(read.layout, Some(layout(0, chat(&kept))), "one pane, the chat they merged into, with the keys");
        assert_eq!(read.dropped, [RUI_LID.to_string()], "the second, as it named it, showed what the first does");
        assert_eq!(read.v, 2, "read as jawd knows it, and kept as it was written");
    }

    #[test]
    fn leaving_out_a_pane_closes_its_split_and_the_keys_go_to_the_pane_before_it() {
        let named =
            |gone: &'static [&'static str]| move |jid: &str| Ok((!gone.contains(&jid)).then(|| jid.to_string()));
        let four = layout(
            2,
            split(
                How::Across,
                300,
                split(How::Down, 500, chat("a@x"), chat("b@x")),
                split(How::Down, 700, chat("c@x"), chat("d@x")),
            ),
        );
        let (same, dropped) = resolved(four.clone(), named(&[])).unwrap();
        assert_eq!((same, dropped.len()), (Some(four.clone()), 0));
        let (kept, dropped) = resolved(four.clone(), named(&["c@x"])).unwrap();
        let without_c =
            layout(1, split(How::Across, 300, split(How::Down, 500, chat("a@x"), chat("b@x")), chat("d@x")));
        assert_eq!((kept, dropped), (Some(without_c), vec!["c@x".to_string()]), "the keys to b, before c");
        let (kept, _) = resolved(four.clone(), named(&["a@x", "b@x"])).unwrap();
        assert_eq!(kept, Some(layout(0, split(How::Down, 700, chat("c@x"), chat("d@x")))), "c keeps them");
        let (kept, _) =
            resolved(layout(0, split(How::Across, 500, chat("a@x"), chat("b@x"))), named(&["a@x"])).unwrap();
        assert_eq!(kept, Some(layout(0, chat("b@x"))), "the first pane gone, the next has them");
        let (kept, dropped) = resolved(four, named(&["a@x", "b@x", "c@x", "d@x"])).unwrap();
        assert_eq!((kept, dropped.len()), (None, 4));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_layout_in_a_format_this_jawd_doesnt_know_is_kept_read_as_none_and_replaced_by_a_newer_one() {
        let home = tempfile::tempdir().unwrap();
        let (_store, layouts) = opened(home.path(), &[ANA, CLIMBING]).await;
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.execute(
            "INSERT INTO interface_state (interface, key, value, v, updated_at) VALUES ('jaw', 'layout', ?1, 5, 1)",
            [r#"{"format":2,"panes":[]}"#],
        )
        .unwrap();
        let read = layouts.get("jaw", Value::Null).await.unwrap();
        assert_eq!((read.v, read.layout), (5, None));
        let stale = layouts.set("jaw", 1, asked(4, &both())).await.unwrap();
        assert_eq!((stale.kept, stale.layout), (Some(false), None), "kept as it was");
        let newer = layouts.set("jaw", 1, asked(6, &both())).await.unwrap();
        assert_eq!((newer.kept, newer.layout), (Some(true), Some(both())));
    }

    const SCENE_HOME: &str = "JAW_LAYOUT_TEST_HOME";
    const SCENE: &str = "JAW_LAYOUT_TEST_SCENE";

    /// Runs `scene` on `home` in a process of its own, this test binary again, and kills it the
    /// moment it says it has got to where the scene stops, as a crash stops jawd. Returns what it
    /// said there.
    async fn crash(home: &Path, scene: &str) -> String {
        use std::io::{BufRead, BufReader};
        let (home, scene) = (home.to_path_buf(), scene.to_string());
        tokio::task::spawn_blocking(move || {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["layout::tests::a_scene_of_jawd_to_be_killed", "--exact", "--ignored", "--nocapture"])
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

    /// The scenes `crash` runs, each in a process of its own; run on its own, it does nothing.
    /// Each asks for a layout while jaw.db's write lock is held from outside: `answered` lets it go
    /// and stops once the answer is made, before it reaches anyone; `asked` stops while the layout
    /// waits for the lock.
    #[test]
    #[ignore = "run by the crash tests, in a process of their own"]
    fn a_scene_of_jawd_to_be_killed() {
        use std::io::Write;
        let (Ok(home), Ok(scene)) = (std::env::var(SCENE_HOME), std::env::var(SCENE)) else { return };
        let at = |what: &str| {
            println!("AT {what}");
            std::io::stdout().flush().unwrap();
        };
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
        runtime.block_on(async move {
            let home = Path::new(&home);
            let (_store, layouts) = opened(home, &[ANA, CLIMBING]).await;
            let holder = Connection::open(home.join("jaw.db")).unwrap();
            holder.execute_batch("BEGIN IMMEDIATE").unwrap();
            let answer = tokio::spawn(async move { layouts.set("jaw", 1, asked(5, &both())).await });
            tokio::time::sleep(Duration::from_millis(300)).await;
            match scene.as_str() {
                "answered" => {
                    holder.execute_batch("COMMIT").unwrap();
                    let saved = answer.await.unwrap().unwrap();
                    at(&format!("{} {:?}", saved.v, saved.kept));
                }
                "asked" => at("asked"),
                other => panic!("there's no scene {other}"),
            }
            // The scene stops here, the lock still held, until it's killed.
            std::thread::sleep(Duration::from_secs(3_600));
            drop(holder);
        });
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_layout_answered_was_on_disk_first_and_after_a_crash_it_opens_and_asking_again_changes_nothing() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(crash(home.path(), "answered").await, "5 Some(true)");
        let (_store, layouts) = opened(home.path(), &[]).await;
        let read = layouts.get("jaw", Value::Null).await.unwrap();
        assert_eq!((read.v, read.layout), (5, Some(both())), "there once jawd starts again");
        let at = updated_at(home.path(), "jaw");
        let again = layouts.set("jaw", 1, asked(5, &both())).await.unwrap();
        assert_eq!(again.kept, Some(true), "the interface asks again, its answer lost");
        assert_eq!(updated_at(home.path(), "jaw"), at, "and nothing changes");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_layout_jawd_stopped_before_keeping_was_never_answered_and_asking_again_keeps_it() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(crash(home.path(), "asked").await, "asked");
        let (_store, layouts) = opened(home.path(), &[]).await;
        assert_eq!(layouts.get("jaw", Value::Null).await.unwrap().v, 0, "not kept, as it wasn't answered");
        let again = layouts.set("jaw", 1, asked(5, &both())).await.unwrap();
        assert_eq!((again.v, again.kept, again.layout), (5, Some(true), Some(both())));
    }
}
