//! Finding anything: `find`, the palette's one search. Names come from the index in memory, matched
//! on a blocking thread while the messages' full-text query runs on the read pool, so a keystroke
//! costs the slower of the two. Names come first, best first, then messages, newest first. A newer
//! find from the same connection cancels the last: matching stops at its next check and a running
//! query stops within a thousand steps, so typing never queues searches.

pub mod bench;
mod index;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use jaw_proto::find::{COUNTED, Find, Found, Hit, Kind, Span, SpanKind};
use rusqlite::Connection;
use tokio::sync::watch;

use self::index::{Index, NameHit, Query};
use crate::store::{Change, Lane, Opened, Store, read};
use crate::work::Priority;

/// Results of each kind when a find doesn't say.
const LIMIT: u32 = 10;
/// The most results of each kind a find can ask for.
const MOST: u32 = 200;
/// How long `opened` waits for the index to take in the open.
const OPENED_WAIT: Duration = Duration::from_secs(1);
/// How long the index waits to try again when it couldn't read what changed.
const RETRY: Duration = Duration::from_secs(1);

/// A find that a newer one replaced, or whose connection closed.
#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a newer find replaced this one")
    }
}

impl std::error::Error for Cancelled {}

/// Says a find is no longer wanted.
#[derive(Default)]
pub struct Cancel {
    cancelled: AtomicBool,
}

/// How many of SQLite's steps a find's query takes between looks at whether it's still wanted.
const STEPS: i32 = 1_000;

impl Cancel {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Runs `query` on `db` unless the find is already cancelled, and stops it within a thousand
    /// steps once it is. SQLite's interrupt would miss a cancel that came just before the query
    /// started; a progress handler looks at the flag itself.
    fn run<T>(self: &Arc<Self>, db: &Connection, query: impl FnOnce() -> Result<T>) -> Result<T> {
        if self.is_cancelled() {
            return Err(Cancelled.into());
        }
        let cancel = self.clone();
        db.progress_handler(STEPS, Some(move || cancel.is_cancelled()))?;
        let answer = query();
        db.progress_handler(0, None::<fn() -> bool>)?;
        match answer {
            Err(e) if interrupted(&e) => Err(Cancelled.into()),
            answer => answer,
        }
    }
}

fn interrupted(e: &anyhow::Error) -> bool {
    matches!(
        e.downcast_ref::<rusqlite::Error>(),
        Some(rusqlite::Error::SqliteFailure(failure, _)) if failure.code == rusqlite::ErrorCode::OperationInterrupted
    )
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

/// The palette's search, shared by every connection.
#[derive(Clone)]
pub struct Finder {
    inner: Arc<Inner>,
}

struct Inner {
    store: Store,
    index: RwLock<Index>,
    /// The feed's seq the index has taken in.
    seen: watch::Sender<i64>,
}

impl Finder {
    /// Loads the index from `store` and keeps it current from the feed.
    pub async fn start(store: Store) -> Result<Finder> {
        let committed = store.committed();
        let (index, seq) = store.reads().query(Priority::Interactive, index::load).await?;
        let finder =
            Finder { inner: Arc::new(Inner { store, index: RwLock::new(index), seen: watch::channel(seq).0 }) };
        tokio::spawn(follow(Arc::downgrade(&finder.inner), committed));
        Ok(finder)
    }

    /// Waits until the index has taken in the feed up to `seq`.
    pub async fn caught_up(&self, seq: i64) {
        let mut seen = self.inner.seen.subscribe();
        let _ = seen.wait_for(|seen| *seen >= seq).await;
    }

    /// Takes in the chats and people that changed after what the index has seen, up to `upto`.
    async fn refresh(&self, upto: i64) -> Result<()> {
        let since = *self.inner.seen.borrow();
        if upto <= since {
            return Ok(());
        }
        let changes =
            self.inner.store.reads().query(Priority::Background, move |db| index::changes(db, since, upto)).await?;
        self.inner.index.write().expect("the name index").apply(changes);
        self.inner.seen.send_replace(upto);
        Ok(())
    }

    /// Counts an open of `chat` in its frecency, once the index has it too.
    pub async fn opened(&self, chat: String) -> Result<()> {
        let applied = self.inner.store.apply(Lane::Live, vec![Change::Opened(Opened { chat, at: now_ms() })]).await?;
        let _ = tokio::time::timeout(OPENED_WAIT, self.caught_up(applied.seq)).await;
        Ok(())
    }

    /// Roughly what the index holds, in bytes.
    pub fn bytes(&self) -> usize {
        self.inner.index.read().expect("the name index").bytes()
    }

    /// Answers `find`, failing with `Cancelled` once `cancel` says it's no longer wanted.
    pub async fn find(&self, find: Find, cancel: Arc<Cancel>) -> Result<Found> {
        let limit = find.limit.unwrap_or(LIMIT).clamp(1, MOST) as usize;
        let kinds: Vec<Kind> = if find.kinds.is_empty() {
            vec![Kind::Chat, Kind::Person, Kind::Folder, Kind::Message, Kind::Link, Kind::Media, Kind::File]
        } else {
            find.kinds.clone()
        };
        let typed = find.query.trim().to_string();
        let now = now_ms();
        if typed.is_empty() {
            let recent = if kinds.contains(&Kind::Chat) {
                self.inner.index.read().expect("the name index").recent(limit, now)
            } else {
                Vec::new()
            };
            let results = self.names(recent, &cancel).await?;
            return Ok(Found { results, counts: BTreeMap::new() });
        }
        let messages = {
            let (store, chat, cancel) = (self.inner.store.clone(), find.chat.clone(), cancel.clone());
            let words = Words::new(&typed);
            let wanted = kinds.contains(&Kind::Message) && !words.0.is_empty();
            async move {
                if !wanted {
                    return anyhow::Ok(None);
                }
                store
                    .reads()
                    .query(Priority::Interactive, move |db| {
                        cancel.run(db, || {
                            let ts = read::matching(db, &words.fts(), chat.as_deref(), COUNTED as usize)?;
                            let rows = read::messages_by_t(db, &ts[..ts.len().min(limit)])?;
                            let rows: Vec<_> = rows
                                .into_iter()
                                .map(|(row, text)| (snippet(&text, &words.marks(&text)), row))
                                .collect();
                            Ok(Some((rows, ts.len())))
                        })
                    })
                    .await
            }
        };
        let names = {
            let (inner, cancel) = (self.inner.clone(), cancel.clone());
            async move {
                tokio::task::spawn_blocking(move || {
                    let query = Query::new(&typed);
                    inner.index.read().expect("the name index").matches(&query, &kinds, limit, now, &cancel)
                })
                .await
            }
        };
        let (names, messages) = tokio::join!(names, messages);
        let (names, mut counts) = names?.ok_or(Cancelled)?;
        let mut results = self.names(names, &cancel).await?;
        if let Some((rows, count)) = messages? {
            if count > 0 {
                counts.insert(Kind::Message, count as u32);
            }
            let index = self.inner.index.read().expect("the name index");
            for (text, row) in rows {
                results.push(Hit {
                    kind: Kind::Message,
                    score: 0,
                    text,
                    field: None,
                    chat_name: index.chat_name_of(&row.chat),
                    folders: Vec::new(),
                    row: serde_json::to_value(&row)?,
                });
            }
        }
        if cancel.is_cancelled() {
            return Err(Cancelled.into());
        }
        Ok(Found { results, counts })
    }

    /// Name hits as results, with each chat's row read from jaw.db as it is now; a chat removed
    /// meanwhile is left out.
    async fn names(&self, hits: Vec<NameHit>, cancel: &Arc<Cancel>) -> Result<Vec<Hit>> {
        let chats: Vec<i64> = hits.iter().filter_map(|hit| hit.chat).collect();
        let mut rows = if chats.is_empty() {
            Vec::new()
        } else {
            let cancel = cancel.clone();
            self.inner
                .store
                .reads()
                .query(Priority::Interactive, move |db| {
                    cancel.run(db, || chats.iter().map(|&n| read::chat(db, n)).collect::<Result<Vec<_>>>())
                })
                .await?
        }
        .into_iter();
        let mut results = Vec::new();
        for hit in hits {
            let row = match (hit.chat, hit.row) {
                (Some(_), _) => match rows.next().flatten() {
                    Some(chat) => serde_json::to_value(&chat)?,
                    None => continue,
                },
                (None, Some(row)) => row,
                (None, None) => continue,
            };
            results.push(Hit {
                kind: hit.kind,
                score: hit.score,
                text: spans(&hit.text, &hit.marks),
                field: hit.field,
                chat_name: None,
                folders: hit.folders,
                row,
            });
        }
        Ok(results)
    }
}

/// Keeps the index current: each commit with events wakes it, and it takes in what changed. It
/// holds the finder only while it works, so once every finder is gone the store can close, and it
/// ends when the store's writer does.
async fn follow(inner: Weak<Inner>, mut committed: watch::Receiver<i64>) {
    loop {
        let upto = *committed.borrow_and_update();
        let Some(inner) = inner.upgrade() else { return };
        if let Err(e) = (Finder { inner }).refresh(upto).await {
            eprintln!("jawd: the palette's names couldn't take in the latest changes, so it tries again: {e:#}");
            tokio::time::sleep(RETRY).await;
            continue;
        }
        if committed.changed().await.is_err() {
            return;
        }
    }
}

/// Words of this many letters or fewer find only themselves, and longer ones find words they start.
/// FTS5 gathers every word with a prefix before it answers, which for one or two letters is most of
/// the index; a whole word it reads newest first and stops at the limit.
const WHOLE_WORD: usize = 2;

/// Text folded as FTS5's tokenizer folds it: lower case, without accents.
fn fold(text: &str) -> String {
    text.chars().map(nucleo_matcher::chars::normalize).flat_map(char::to_lowercase).collect()
}

/// The words typed, folded, each with whether it finds the words it starts.
struct Words(Vec<(String, bool)>);

impl Words {
    fn new(typed: &str) -> Words {
        let words = typed
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(|word| (fold(word), word.chars().count() > WHOLE_WORD))
            .collect();
        Words(words)
    }

    /// The FTS5 query: every word, each quoted so nothing typed is FTS5's syntax.
    fn fts(&self) -> String {
        let words = self.0.iter().map(|(word, prefix)| format!("\"{word}\"{}", if *prefix { "*" } else { "" }));
        words.collect::<Vec<_>>().join(" ")
    }

    /// Where in `text` the words matched: each word of it that one finds, as byte ranges.
    fn marks(&self, text: &str) -> Vec<(usize, usize)> {
        let mut marks = Vec::new();
        let mut start = None;
        for (at, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
            if c.is_alphanumeric() {
                start.get_or_insert(at);
            } else if let Some(from) = start.take() {
                let word = fold(&text[from..at]);
                if self.0.iter().any(|(typed, prefix)| if *prefix { word.starts_with(typed) } else { word == *typed }) {
                    marks.push((from, at));
                }
            }
        }
        marks
    }
}

/// `text` as spans, with `marks` (byte ranges, in order) marked and line breaks as spaces, so a
/// result is one line.
fn spans(text: &str, marks: &[(usize, usize)]) -> Vec<Span> {
    let line = |s: &str| s.replace(['\n', '\r', '\t'], " ");
    let mut spans = Vec::new();
    let mut at = 0;
    for &(start, end) in marks {
        if start > at {
            spans.push(Span { t: SpanKind::Text, s: line(&text[at..start]) });
        }
        spans.push(Span { t: SpanKind::Mark, s: line(&text[start..end]) });
        at = end;
    }
    if at < text.len() {
        spans.push(Span { t: SpanKind::Text, s: line(&text[at..]) });
    }
    spans
}

/// How much of a message comes before its first match, in chars, when the match is further in.
const SNIPPET_BEFORE: usize = 30;
/// The most of a message a result shows, in chars.
const SNIPPET_MOST: usize = 160;

/// A stretch of a message around its first match, as spans: from the start of a word a little
/// before it, with an ellipsis where text is left out.
fn snippet(text: &str, marks: &[(usize, usize)]) -> Vec<Span> {
    let first = marks.first().map_or(0, |mark| mark.0);
    let before = text[..first].chars().count();
    let mut start = 0;
    if before > SNIPPET_BEFORE {
        start = text[..first].char_indices().nth(before - SNIPPET_BEFORE).map_or(0, |(at, _)| at);
        if let Some(space) = text[start..first].find(' ') {
            start += space + 1;
        }
    }
    let end = text[start..].char_indices().nth(SNIPPET_MOST).map_or(text.len(), |(at, _)| start + at);
    let marks: Vec<(usize, usize)> = marks
        .iter()
        .filter(|mark| mark.0 >= start && mark.0 < end)
        .map(|&(from, to)| (from - start, to.min(end) - start))
        .collect();
    let mut spans = spans(&text[start..end], &marks);
    if start > 0 {
        spans.insert(0, Span { t: SpanKind::Text, s: "…".into() });
    }
    if end < text.len() {
        spans.push(Span { t: SpanKind::Text, s: "…".into() });
    }
    spans
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::store::{Chat, Message, Person};

    async fn finder() -> (tempfile::TempDir, Store, Finder) {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let finder = Finder::start(store.clone()).await.unwrap();
        (home, store, finder)
    }

    async fn apply(store: &Store, finder: &Finder, changes: Vec<Change>) {
        let applied = store.apply(Lane::Live, changes).await.unwrap();
        finder.caught_up(applied.seq).await;
    }

    async fn find(finder: &Finder, query: &str, kinds: &[Kind]) -> Found {
        let find = Find { query: query.into(), kinds: kinds.to_vec(), ..Find::default() };
        finder.find(find, Arc::new(Cancel::default())).await.unwrap()
    }

    fn shown(hit: &Hit) -> String {
        hit.text.iter().map(|span| span.s.as_str()).collect()
    }

    fn found(found: &Found) -> Vec<(Kind, String)> {
        found.results.iter().map(|hit| (hit.kind, shown(hit))).collect()
    }

    fn group(id: &str, name: &str) -> Change {
        Change::Chat(Chat { id: id.into(), name: Some(name.into()), ..Chat::default() })
    }

    fn text(chat: &str, id: &str, at: i64, words: &str) -> Change {
        Change::Message(Message {
            chat: chat.into(),
            id: id.into(),
            at,
            sender: Some("351910000009@s.whatsapp.net".into()),
            kind: "text",
            text: Some(words.into()),
            summary: words.into(),
            ..Message::default()
        })
    }

    const ANA: &str = "351910000001@s.whatsapp.net";
    const CLIMBING: &str = "120363000000000001@g.us";

    #[tokio::test(flavor = "multi_thread")]
    async fn the_index_follows_new_people_renamed_chats_and_joined_lids() {
        let (_home, store, finder) = finder().await;
        assert!(find(&finder, "ana", &[Kind::Chat, Kind::Person]).await.results.is_empty());
        let ana = Person { jids: vec![ANA.into()], name: Some("Ana Ribeiro".into()), ..Person::default() };
        apply(&store, &finder, vec![Change::Person(ana), Change::Chat(Chat { id: ANA.into(), ..Chat::default() })])
            .await;
        let ar = find(&finder, "ar", &[Kind::Chat, Kind::Person]).await;
        assert_eq!(found(&ar), [(Kind::Chat, "Ana Ribeiro".into())]);
        assert_eq!(ar.results[0].row["id"], ANA, "the chat's row, as the chat list has it");

        apply(&store, &finder, vec![group(CLIMBING, "Climbing")]).await;
        apply(&store, &finder, vec![group(CLIMBING, "Lisbon Climbing")]).await;
        assert_eq!(found(&find(&finder, "lisb", &[Kind::Chat]).await), [(Kind::Chat, "Lisbon Climbing".into())]);

        // Someone writes in the group from their LID, known only by what they call themselves,
        // and is later joined to the contact with their number.
        let lid =
            Person { jids: vec!["100000000002@lid".into()], push_name: Some("Rui C".into()), ..Person::default() };
        let contact = Person {
            jids: vec!["351910000002@s.whatsapp.net".into()],
            name: Some("Rui Costa".into()),
            ..Person::default()
        };
        apply(&store, &finder, vec![Change::Person(lid), Change::Person(contact)]).await;
        assert_eq!(find(&finder, "rui", &[Kind::Person]).await.results.len(), 2);
        let join =
            Person { jids: vec!["100000000002@lid".into(), "351910000002@s.whatsapp.net".into()], ..Person::default() };
        apply(&store, &finder, vec![Change::Person(join)]).await;
        let rui = find(&finder, "rui", &[Kind::Person]).await;
        assert_eq!(found(&rui), [(Kind::Person, "Rui Costa".into())], "one person, by their contact's name");
        assert_eq!(rui.results[0].row["phone"], "+351910000002");
        assert_eq!(found(&find(&finder, "rui c", &[Kind::Person]).await)[0].1, "Rui Costa");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn opening_a_chat_ranks_it_first_and_is_kept_in_jaw_db() {
        let (home, store, finder) = finder().await;
        let costa = "120363000000000002@g.us";
        let santos = "120363000000000003@g.us";
        apply(&store, &finder, vec![group(costa, "Rui Costa"), group(santos, "Rui Santos")]).await;
        apply(&store, &finder, vec![text(costa, "m1", 2_000, "olá")]).await;
        assert_eq!(found(&find(&finder, "", &[]).await)[0].1, "Rui Costa", "unopened, the newest first");
        finder.opened(santos.into()).await.unwrap();
        assert_eq!(found(&find(&finder, "rui", &[Kind::Chat]).await)[0].1, "Rui Santos");
        let recent = find(&finder, "", &[]).await;
        assert_eq!(found(&recent)[0].1, "Rui Santos");
        assert_eq!(recent.results[0].score, 100, "one open, just now");
        drop((finder, store));
        let store = Store::open(home.path()).unwrap();
        let again = Finder::start(store).await.unwrap();
        assert_eq!(found(&find(&again, "", &[]).await)[0].1, "Rui Santos", "frecency is kept in jaw.db");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn messages_come_after_the_names_newest_first_with_their_chat() {
        let (_home, store, finder) = finder().await;
        let familia = "120363000000000004@g.us";
        apply(
            &store,
            &finder,
            vec![
                group(familia, "Família Gomes"),
                group(CLIMBING, "Jantar de sexta"),
                text(familia, "old", 1_000, "jantar amanhã?"),
                text(familia, "new", 5_000, "o jantar foi ótimo"),
                text(CLIMBING, "other", 3_000, "já vou"),
            ],
        )
        .await;
        let ja = find(&finder, "ja", &[Kind::Message]).await;
        assert_eq!(found(&ja), [(Kind::Message, "já vou".into())], "two letters find the word, not jantar");
        let jantar = find(&finder, "jantar", &[]).await;
        let kinds: Vec<Kind> = jantar.results.iter().map(|hit| hit.kind).collect();
        assert_eq!(kinds, [Kind::Chat, Kind::Message, Kind::Message]);
        assert_eq!(jantar.results[1].row["id"], "new");
        assert_eq!(jantar.results[1].chat_name.as_deref(), Some("Família Gomes"));
        let marked: Vec<&str> =
            jantar.results[1].text.iter().filter(|span| span.t == SpanKind::Mark).map(|span| span.s.as_str()).collect();
        assert_eq!(marked, ["jantar"]);
        assert_eq!((jantar.counts[&Kind::Chat], jantar.counts[&Kind::Message]), (1, 2));
        let amanha = find(&finder, "amanha", &[]).await;
        assert_eq!(found(&amanha), [(Kind::Message, "jantar amanhã?".into())], "accent-blind");
        let names_only = find(&finder, "jantar", &[Kind::Chat]).await;
        assert_eq!(names_only.results.len(), 1);
        let in_climbing = Find { query: "jantar".into(), chat: Some(CLIMBING.into()), ..Find::default() };
        let in_climbing = finder.find(in_climbing, Arc::new(Cancel::default())).await.unwrap();
        assert_eq!(found(&in_climbing), [(Kind::Chat, "Jantar de sexta".into())], "no message there says jantar");
        let in_familia =
            Find { query: "jantar".into(), kinds: vec![Kind::Message], chat: Some(familia.into()), ..Find::default() };
        let in_familia = finder.find(in_familia, Arc::new(Cancel::default())).await.unwrap();
        assert_eq!((in_familia.results.len(), in_familia.counts[&Kind::Message]), (2, 2));
        let links = find(&finder, "jantar", &[Kind::Link, Kind::Media, Kind::File]).await;
        assert!(links.results.is_empty() && links.counts.is_empty(), "nothing finds links, media or files yet");
    }

    #[test]
    fn short_words_find_themselves_and_longer_ones_the_words_they_start() {
        let words = Words::new("Já jant \"OR");
        assert_eq!(words.fts(), r#""ja" "jant"* "or""#);
        let text = "Já jantaste? jantar, JANTA ja jaula ou";
        let marked: Vec<&str> = words.marks(text).iter().map(|&(from, to)| &text[from..to]).collect();
        assert_eq!(marked, ["Já", "jantaste", "jantar", "JANTA", "ja"]);
    }

    #[test]
    fn a_long_message_is_cut_to_a_stretch_around_its_first_match() {
        let text = format!("{} jantar {}", "palavra ".repeat(40), "depois ".repeat(40));
        let at = text.find("jantar").unwrap();
        let spans = snippet(&text, &[(at, at + 6)]);
        assert_eq!(spans.first().unwrap().s, "…");
        assert_eq!(spans.last().unwrap().s, "…");
        assert!(spans[1].s.starts_with("palavra"), "from a word's start: {:?}", spans[1].s);
        let marks: Vec<&Span> = spans.iter().filter(|span| span.t == SpanKind::Mark).collect();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].s, "jantar");
        let shown: usize = spans.iter().map(|span| span.s.chars().count()).sum();
        assert!(shown <= SNIPPET_MOST + 2, "{shown} chars");
        let short = snippet("olá\njantar", &[(5, 11)]);
        assert_eq!(
            short,
            [Span { t: SpanKind::Text, s: "olá ".into() }, Span { t: SpanKind::Mark, s: "jantar".into() }]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_find_cancelled_before_it_runs_answers_cancelled() {
        let (_home, store, finder) = finder().await;
        apply(&store, &finder, vec![group(CLIMBING, "Lisbon Climbing")]).await;
        let cancel = Arc::new(Cancel::default());
        cancel.cancel();
        let find = Find { query: "lisbon".into(), ..Find::default() };
        let e = finder.find(find, cancel).await.unwrap_err();
        assert!(e.is::<Cancelled>(), "{e:#}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_running_query_stops_when_its_find_is_cancelled_however_soon() {
        let (_home, store, _finder) = finder().await;
        for wait in [0, 0, 1, 1, 5, 50] {
            let cancel = Arc::new(Cancel::default());
            let started = Instant::now();
            let running = {
                let (cancel, store) = (cancel.clone(), store.clone());
                let slow = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 10000000000)
                    SELECT count(*) FROM c";
                tokio::spawn(async move {
                    store
                        .reads()
                        .query(Priority::Interactive, move |db| {
                            cancel.run(db, || Ok(db.query_row(slow, [], |r| r.get::<_, i64>(0))?))
                        })
                        .await
                })
            };
            tokio::time::sleep(Duration::from_millis(wait)).await;
            cancel.cancel();
            let e = running.await.unwrap().unwrap_err();
            assert!(e.is::<Cancelled>(), "{e:#}");
            assert!(started.elapsed() < Duration::from_secs(2), "it ran on for {:?}", started.elapsed());
        }
        let after =
            store.reads().query(Priority::Interactive, |db| Ok(db.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))?));
        assert_eq!(after.await.unwrap(), 1, "the reader's next query isn't stopped");
    }
}
