//! Communities in chat lists: one row wherever their groups would show, summing up the groups under
//! it there and sitting where the newest of them does; one row of a window's before and after; a
//! group whose community jaw doesn't have on its own until it does, however it's learned; and each
//! window what a fresh read gives through every change, a kill at any commit too.

use std::path::Path;

use rusqlite::Connection;

use super::*;
use crate::store::folders::{Done, Item, Outcome, Step};
use crate::store::lists;

const ANA: &str = "351910000002@s.whatsapp.net";
const ANA_LID: &str = "100000000002@lid";
const RUI: &str = "351910000003@s.whatsapp.net";
const INES: &str = "351910000004@s.whatsapp.net";
const BAIRRO: &str = "120363000000000010@g.us";
const AVISOS: &str = "120363000000000011@g.us";
const FESTA: &str = "120363000000000012@g.us";
const BIBLIOTECA: &str = "120363000000000013@g.us";
const XADREZ: &str = "120363000000000014@g.us";
const VELA: &str = "120363000000000020@g.us";
const REGATA: &str = "120363000000000021@g.us";
const AT: i64 = 1_792_000_000_000;

fn community(id: &str, name: &str) -> Change {
    Change::Chat(Chat { id: id.into(), kind: Some("community"), name: Some(name.into()), ..Chat::default() })
}

fn group(id: &str, name: &str, of: &str) -> Change {
    Change::Chat(Chat {
        id: id.into(),
        kind: Some("group"),
        name: Some(name.into()),
        community: Some(of.into()),
        ..Chat::default()
    })
}

fn announcements(id: &str, name: &str, of: &str) -> Change {
    Change::Chat(Chat {
        id: id.into(),
        kind: Some("announcement"),
        name: Some(name.into()),
        community: Some(of.into()),
        ..Chat::default()
    })
}

/// A group joining community `of`, or leaving its community with `of` empty.
fn linked(id: &str, of: &str) -> Change {
    Change::Chat(Chat { id: id.into(), community: Some(of.into()), ..Chat::default() })
}

fn said(chat: &str, id: &str, at: i64) -> Change {
    Change::Message(Message { live: true, ..text(chat, id, at, Some(RUI), id) })
}

fn mentioned(chat: &str, id: &str, at: i64) -> Change {
    Change::Message(Message { live: true, mentions_me: true, ..text(chat, id, at, Some(RUI), id) })
}

fn person(jid: &str, name: &str) -> Change {
    Change::Person(Person { jids: vec![jid.into()], name: Some(name.into()), ..Person::default() })
}

fn read_up_to(chat: &str, id: &str) -> Change {
    Change::Read(Read { chat: chat.into(), ids: vec![id.into()], here: true, ..Read::default() })
}

fn archived(chat: &str, archived: bool) -> Change {
    Change::Chat(Chat { id: chat.into(), archived: Some(archived), ..Chat::default() })
}

async fn step(bed: &Bed, make: impl FnOnce(Done) -> Step) -> Outcome {
    let done = Done::default();
    bed.store.apply_durable(vec![Change::Folders(make(done.clone()))]).await.unwrap();
    done.take().unwrap()
}

async fn folder(bed: &Bed, name: &str) -> String {
    match step(bed, |done| Step::Create { name: name.into(), parent: None, rule: None, done }).await {
        Outcome::Done(Some(n)) => format!("folder:{n}"),
        other => panic!("{other:?}"),
    }
}

async fn file(bed: &Bed, folder: &str, item: Item) {
    let folder = folder.strip_prefix("folder:").unwrap().parse().unwrap();
    assert_eq!(step(bed, |done| Step::File { folder, item, done }).await, Outcome::Done(None));
}

/// The smart folder jaw starts with by that name.
async fn smart(watcher: &mut Watcher, name: &str) -> String {
    let tree = watcher.call("folders.watch", json!({})).await.unwrap();
    watcher.call("unwatch", json!({"watch": tree["watch"]})).await.unwrap();
    let rows = tree["rows"].as_array().unwrap();
    let row = rows.iter().find(|row| row["name"] == name).unwrap_or_else(|| panic!("no folder {name}"));
    row["id"].as_str().unwrap().to_string()
}

/// A chat list as an interface holds it, as it watched it, and its window's params.
struct Held {
    params: Value,
    model: Model,
}

impl Held {
    async fn watch(watcher: &mut Watcher, params: Value) -> Held {
        let result = watcher.call("chats.watch", params.clone()).await.unwrap();
        watcher.call("unwatch", json!({"watch": result["watch"]})).await.unwrap();
        let mut model = Model { chats: true, ..Model::default() };
        model.take(&result, "rows");
        Held { params, model }
    }

    async fn top(watcher: &mut Watcher, folder: &str) -> Held {
        Held::watch(watcher, json!({"folder": folder, "around": "top", "after": 50})).await
    }

    /// Watches again from the seq it held with the ids it holds, as an interface coming back does,
    /// and must then hold what a fresh watch gives.
    async fn come_back(&mut self, watcher: &mut Watcher, what: &str) {
        let mut params = self.params.clone();
        params["since"] = self.model.seq.into();
        params["have"] = json!(self.model.ids());
        let result = watcher.call("chats.watch", params).await.unwrap();
        watcher.call("unwatch", json!({"watch": result["watch"]})).await.unwrap();
        self.model.take(&result, "rows");
        let fresh = Held::watch(watcher, self.params.clone()).await;
        self.model.assert_matches(&fresh.model, what);
        summed(&self.model, what);
    }

    fn row(&self, id: &str) -> &Value {
        self.model.rows.get(id).unwrap_or_else(|| panic!("no row {id}: {:?}", self.model.ids()))
    }
}

/// A chat list's rows as an interface lays them out: a community's name with its counts, and under
/// it, indented, the groups naming it, its announcement group first and the rest by their sort; the
/// other chats by name.
fn shape(model: &Model) -> Vec<String> {
    let ordered = model.ordered();
    fn community<'a>(model: &Model, row: &'a Value) -> Option<&'a str> {
        let of = row["community"].as_str()?;
        (model.rows.get(of)?["kind"] == "community").then_some(of)
    }
    let community = |row: &'_ Value| community(model, row).map(str::to_string);
    let mut out = Vec::new();
    for row in ordered.iter().filter(|row| community(row).is_none()) {
        let name = row["name"].as_str().unwrap();
        if row["kind"] != "community" {
            out.push(name.to_string());
            continue;
        }
        out.push(format!("{name} {}/{}", row["children_unread"], row["children_mentions"]));
        let mut groups: Vec<&Value> =
            ordered.iter().filter(|g| community(g).as_deref() == row["id"].as_str()).collect();
        groups.sort_by_key(|g| g["kind"] != "announcement");
        out.extend(groups.iter().map(|g| format!("  {}", g["name"].as_str().unwrap())));
    }
    out
}

/// Asserts each community's row in `model` whose groups it brings sums up the groups naming it
/// there and sits just above the newest of them.
fn summed(model: &Model, what: &str) {
    for row in model.rows.values().filter(|row| row["kind"] == "community") {
        let groups: Vec<&Value> = model.rows.values().filter(|g| g["community"] == row["id"]).collect();
        if groups.is_empty() {
            continue;
        }
        let sum = |field: &str| groups.iter().map(|g| g[field].as_u64().unwrap()).sum::<u64>();
        assert_eq!(row["children_unread"].as_u64(), Some(sum("unread")), "{what}: {}'s unread", row["name"]);
        assert_eq!(row["children_mentions"].as_u64(), Some(sum("mentions")), "{what}: {}'s mentions", row["name"]);
        let sort = row["sort"].as_str().unwrap();
        for group in &groups {
            assert!(group["sort"].as_str().unwrap() < sort, "{what}: {} above {}", row["name"], group["name"]);
        }
        let newest = groups.iter().filter_map(|g| g["last"]["at"].as_i64()).max();
        if let Some(newest) = newest {
            assert_eq!(row["last"]["at"].as_i64(), Some(newest), "{what}: {}'s newest message", row["name"]);
        }
    }
}

async fn drift(bed: &Bed, what: &str) {
    let drift = bed.store.reads().query(crate::work::Priority::Interactive, lists::drift).await.unwrap();
    assert!(drift.is_empty(), "{what}: {drift:?}");
}

/// Bairro, with its announcement group and three groups, one archived and one read; Regata, whose
/// community jaw doesn't have; Ana, Rui, and a folder holding Ana and Festa.
async fn bairro(bed: &Bed) -> String {
    bed.apply(vec![
        person(ANA, "Ana"),
        person(RUI, "Rui"),
        community(BAIRRO, "Bairro"),
        announcements(AVISOS, "Avisos", BAIRRO),
        group(FESTA, "Festa", BAIRRO),
        group(BIBLIOTECA, "Biblioteca", BAIRRO),
        group(XADREZ, "Xadrez", BAIRRO),
        group(REGATA, "Regata", VELA),
        said(RUI, "r1", AT + 5),
        said(XADREZ, "x1", AT + 8),
        said(AVISOS, "a1", AT + 10),
        said(REGATA, "g1", AT + 15),
        said(FESTA, "f1", AT + 20),
        mentioned(FESTA, "f2", AT + 21),
        said(BIBLIOTECA, "b1", AT + 30),
        Change::Message(Message { live: true, ..text(ANA, "n1", AT + 40, Some(ANA), "olá") }),
    ])
    .await;
    bed.apply(vec![read_up_to(XADREZ, "x1"), read_up_to(RUI, "r1"), archived(BIBLIOTECA, true)]).await;
    let family = folder(bed, "Family").await;
    file(bed, &family, Item::Chat(FESTA.into())).await;
    let ana = bed
        .store
        .reads()
        .query(crate::work::Priority::Interactive, |db| {
            Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [ANA], |r| r.get::<_, i64>(0))?)
        })
        .await
        .unwrap();
    file(bed, &family, Item::Person(ana)).await;
    family
}

#[tokio::test(flavor = "multi_thread")]
async fn a_community_is_one_row_wherever_its_groups_would_show_summing_up_the_groups_there() {
    let bed = Bed::new(brisk());
    let family = bairro(&bed).await;
    let mut watcher = bed.watcher(64);
    let mentions = smart(&mut watcher, "Mentions me").await;
    let lists: [(&str, &[&str], u64); 7] = [
        ("all", &["Ana", "Bairro 3/1", "  Avisos", "  Festa", "  Xadrez", "Regata", "Rui"], 7),
        ("unread", &["Ana", "Bairro 3/1", "  Avisos", "  Festa", "Regata"], 5),
        ("groups", &["Bairro 3/1", "  Avisos", "  Festa", "  Xadrez", "Regata"], 5),
        ("communities", &["Bairro 3/1", "  Avisos", "  Festa", "  Xadrez", "Regata"], 5),
        ("archived", &["Bairro 1/0", "  Biblioteca"], 2),
        (&family, &["Ana", "Bairro 2/1", "  Festa"], 3),
        (&mentions, &["Bairro 2/1", "  Festa"], 2),
    ];
    for (folder, want, total) in lists {
        let held = Held::top(&mut watcher, folder).await;
        assert_eq!(shape(&held.model), want, "{folder}");
        assert_eq!(held.model.total, total, "{folder}: its chats, and a community it holds only for its groups");
        summed(&held.model, folder);
        if held.model.rows.contains_key(REGATA) {
            assert_eq!(held.row(REGATA)["community"], json!(VELA), "{folder}: a group of a community jaw doesn't have");
        }
    }
    let unread = Held::top(&mut watcher, "unread").await;
    assert_eq!(unread.row(BAIRRO)["last"]["summary"], json!("f2"), "its newest message, of the groups there");
    let archive = Held::top(&mut watcher, "archived").await;
    assert_eq!(archive.row(BAIRRO)["last"]["summary"], json!("b1"));
    let place = |held: &Held, id: &str| held.row(id)["sort"].as_str().unwrap().to_string();
    assert_eq!(place(&unread, BAIRRO), format!("{}~~", place(&unread, FESTA)), "just above Festa, its newest there");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_communitys_row_found_on_its_own_sums_up_every_group_of_it() {
    let bed = Bed::new(brisk());
    bairro(&bed).await;
    bed.store.indexed().await.unwrap();
    let socket = serve_all(&bed).await;
    let found =
        tokio::task::spawn_blocking(move || Line::open(&socket, None).0.call("find", json!({"query": "Bairro"})))
            .await
            .unwrap();
    let hits = found["results"].as_array().unwrap();
    let row = hits.iter().find(|hit| hit["kind"] == "chat" && hit["row"]["id"] == BAIRRO).map(|hit| &hit["row"]);
    let row = row.unwrap_or_else(|| panic!("Bairro among what's found: {found}"));
    assert_eq!(
        (row["children_unread"].as_u64(), row["children_mentions"].as_u64()),
        (Some(4), Some(1)),
        "Avisos's, Festa's with its mention and Biblioteca's, archived, as no list holds them all: {row}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_community_and_its_groups_are_one_row_of_a_window_and_a_window_around_a_group_is_around_it() {
    let bed = Bed::new(brisk());
    bed.apply(vec![
        person(ANA, "Ana"),
        person(RUI, "Rui"),
        person(INES, "Inês"),
        community(BAIRRO, "Bairro"),
        announcements(AVISOS, "Avisos", BAIRRO),
        group(FESTA, "Festa", BAIRRO),
        said(INES, "i1", AT + 10),
        said(RUI, "r1", AT + 20),
        said(FESTA, "f1", AT + 30),
        said(AVISOS, "a1", AT + 40),
        said(ANA, "n1", AT + 50),
    ])
    .await;
    let mut watcher = bed.watcher(64);
    let top = Held::watch(&mut watcher, json!({"folder": "all", "around": "top", "after": 2})).await;
    assert_eq!(shape(&top.model), ["Ana", "Bairro 2/0", "  Avisos", "  Festa"], "two rows, the community one of them");
    let around = Held::watch(&mut watcher, json!({"folder": "all", "around": FESTA, "before": 1, "after": 1})).await;
    assert_eq!(
        shape(&around.model),
        ["Ana", "Bairro 2/0", "  Avisos", "  Festa", "Rui"],
        "around Festa is around Bairro, a row above it and a row below"
    );
    let below = Held::watch(&mut watcher, json!({"folder": "all", "around": RUI, "before": 1, "after": 1})).await;
    assert_eq!(shape(&below.model), ["Bairro 2/0", "  Avisos", "  Festa", "Rui", "Inês"]);
    // A pinned group pins its community, which sits among the pinned chats.
    bed.apply(vec![Change::Chat(Chat { id: FESTA.into(), pinned: Some(AT + 60), ..Chat::default() })]).await;
    let pinned = Held::watch(&mut watcher, json!({"folder": "all", "around": "top", "after": 1})).await;
    assert_eq!(shape(&pinned.model), ["Bairro 2/0", "  Avisos", "  Festa"]);
    assert!(pinned.row(BAIRRO)["sort"].as_str().unwrap().starts_with("1~"), "{}", pinned.row(BAIRRO)["sort"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_community_a_window_hasnt_open_is_one_row_summing_up_the_same_and_opening_it_brings_its_groups() {
    let bed = Bed::new(brisk());
    bairro(&bed).await;
    let mut watcher = bed.watcher(64);
    for folder in ["all", "unread", "groups", "communities", "archived"] {
        let open = Held::top(&mut watcher, folder).await;
        let shut = json!({"folder": folder, "around": "top", "after": 50, "open": []});
        let mut closed = Held::watch(&mut watcher, shut).await;
        let groups = |held: &Held| held.model.rows.values().filter(|row| row["community"] == BAIRRO).count();
        assert!(groups(&open) > 0, "{folder}: its groups, open");
        assert_eq!(groups(&closed), 0, "{folder}: none of its groups, closed");
        for field in ["sort", "children_unread", "children_mentions", "last", "v"] {
            assert_eq!(closed.row(BAIRRO)[field], open.row(BAIRRO)[field], "{folder}: its {field}, closed as open");
        }
        assert_eq!(closed.model.total, open.model.total, "{folder}: the same list");
        closed.come_back(&mut watcher, &format!("{folder}, closed")).await;
    }
    let all = watcher.watch("chats.watch", json!({"folder": "all", "around": "top", "after": 50, "open": []})).await;
    let move_to = |open: Value| json!({"watch": all, "around": "top", "after": 50, "open": open});
    let opened = watcher.call("watch.move", move_to(json!([BAIRRO]))).await.unwrap();
    let came: Vec<&str> = opened["rows"].as_array().unwrap().iter().map(|row| row["id"].as_str().unwrap()).collect();
    assert_eq!(came.len(), 3, "its three groups come, and nothing else: {came:?}");
    assert!([AVISOS, FESTA, XADREZ].iter().all(|group| came.contains(group)), "{came:?}");
    let scrolled = watcher.call("watch.move", json!({"watch": all, "around": "top", "after": 50})).await.unwrap();
    assert!(scrolled["rows"].as_array().unwrap().is_empty(), "a move saying nothing of open keeps the window's");
    let shut = watcher.call("watch.move", move_to(json!([]))).await.unwrap();
    let mut left: Vec<&str> = shut["remove"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
    left.sort();
    assert_eq!(left, [AVISOS, FESTA, XADREZ], "closed, they leave the window");
    let scrolled = watcher.call("watch.move", json!({"watch": all, "around": "top", "after": 50})).await.unwrap();
    assert!(scrolled["rows"].as_array().unwrap().is_empty(), "and closed, a move saying nothing of open keeps it so");
    let many: Vec<String> = (0..1_001).map(|n| format!("1203630{n:011}@g.us")).collect();
    let refused = watcher.call("chats.watch", json!({"folder": "all", "around": "top", "open": many})).await;
    assert_eq!(refused.unwrap_err().code, code::INVALID_PARAMS, "a thousand communities open at most");
}

/// Each chat list's layout, as `shape` gives it.
async fn shapes(watcher: &mut Watcher) -> Vec<(String, Vec<String>, u64)> {
    let mut out = Vec::new();
    for folder in ["all", "unread", "groups", "communities", "archived"] {
        let held = Held::top(watcher, folder).await;
        summed(&held.model, folder);
        out.push((folder.to_string(), shape(&held.model), held.model.total));
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn a_group_whose_community_jaw_doesnt_have_comes_under_it_once_it_does_and_learned_twice_or_first_ends_alike() {
    // Regata first, naming Vela, which jaw learns of later, twice: the second time as it was, then
    // renamed.
    let late = Bed::new(brisk());
    late.apply(vec![group(REGATA, "Regata", VELA), said(REGATA, "g1", AT + 10), said(RUI, "r1", AT + 5)]).await;
    let mut watcher = late.watcher(64);
    let mut all = Held::top(&mut watcher, "all").await;
    assert_eq!(shape(&all.model), ["Regata", "+351910000003"], "a row of its own");
    let regata = all.row(REGATA)["v"].as_u64();
    late.apply(vec![community(VELA, "Vela")]).await;
    all.come_back(&mut watcher, "its community learned").await;
    assert_eq!(shape(&all.model), ["Vela 1/0", "  Regata", "+351910000003"]);
    assert_eq!(all.row(REGATA)["v"].as_u64(), regata, "its own row as it was: Vela's row says where it goes");
    let seq = late.feed.seq();
    late.apply(vec![community(VELA, "Vela")]).await;
    assert_eq!(late.feed.seq(), seq, "learned again as it was, nothing changes");
    late.apply(vec![community(VELA, "Vela de Cascais")]).await;
    all.come_back(&mut watcher, "its community renamed").await;
    assert_eq!(all.row(REGATA)["v"].as_u64(), regata, "renamed, its community moves none of its groups");

    // Vela first.
    let first = Bed::new(brisk());
    first.apply(vec![community(VELA, "Vela de Cascais")]).await;
    first.apply(vec![group(REGATA, "Regata", VELA), said(REGATA, "g1", AT + 10), said(RUI, "r1", AT + 5)]).await;
    let mut watching = first.watcher(64);
    assert_eq!(shapes(&mut watching).await, shapes(&mut watcher).await, "either order ends alike");

    // Vela known first as a group, from a message in its own chat, then as the community it is.
    let grown = Bed::new(brisk());
    grown.apply(vec![said(VELA, "v1", AT + 1), group(REGATA, "Regata", VELA), said(REGATA, "g1", AT + 10)]).await;
    let mut watching = grown.watcher(64);
    let mut all = Held::top(&mut watching, "all").await;
    assert_eq!(all.row(REGATA)["community"], json!(VELA));
    assert!(shape(&all.model).contains(&"Regata".to_string()), "on its own while Vela is a group to jaw");
    grown.apply(vec![community(VELA, "Vela de Cascais")]).await;
    all.come_back(&mut watching, "its community found to be one").await;
    assert_eq!(shape(&all.model)[..2], ["Vela de Cascais 1/0", "  Regata"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_communitys_counts_stay_its_groups_through_messages_reads_joins_leaves_archives_and_merges() {
    let bed = Bed::new(brisk());
    let family = bairro(&bed).await;
    bed.apply(vec![Change::Chat(Chat { id: XADREZ.into(), community: Some(String::new()), ..Chat::default() })]).await;
    bed.apply(vec![person(ANA_LID, "Ana"), said(ANA_LID, "l1", AT + 45)]).await;
    let mut watcher = bed.watcher(64);
    let mut lists = Vec::new();
    for folder in ["all", "unread", "groups", "communities", "archived", family.as_str()] {
        lists.push(Held::top(&mut watcher, folder).await);
    }
    let changes: Vec<(&str, Vec<Change>)> = vec![
        ("a message in one of its groups", vec![said(FESTA, "f3", AT + 50)]),
        ("a mention in one of its groups", vec![mentioned(AVISOS, "a2", AT + 51)]),
        ("one of its groups read", vec![read_up_to(FESTA, "f3")]),
        ("a group joining it", vec![linked(XADREZ, BAIRRO), said(XADREZ, "x2", AT + 52)]),
        ("a group leaving it", vec![linked(AVISOS, "")]),
        ("one of its groups archived", vec![archived(FESTA, true)]),
        ("one of its groups out of the archive", vec![archived(BIBLIOTECA, false)]),
        ("a group joining a community jaw doesn't have", vec![linked(FESTA, VELA)]),
        ("that community coming", vec![community(VELA, "Vela")]),
        (
            "someone's number and LID found to be one",
            vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })],
        ),
    ];
    for (what, change) in changes {
        bed.apply(change).await;
        for held in &mut lists {
            let folder = held.params["folder"].clone();
            held.come_back(&mut watcher, &format!("{what}, in {folder}")).await;
        }
        drift(&bed, what).await;
    }
    for _ in 0..50 {
        if bed.store.count("chat_merges").unwrap() == 0 {
            break;
        }
        bed.store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
    }
    for held in &mut lists {
        let folder = held.params["folder"].clone();
        held.come_back(&mut watcher, &format!("Ana's chats merged, in {folder}")).await;
    }
    drift(&bed, "Ana's chats merged").await;
    let all = &lists[0];
    assert_eq!(
        all.row(BAIRRO)["children_unread"],
        json!(2),
        "Xadrez's and Biblioteca's, as Festa went to the archive and to Vela, and Avisos left"
    );
    assert_eq!(shape(&lists[4].model)[..2], ["Vela 0/0", "  Festa"], "Festa, archived, under Vela there");
}

#[tokio::test(flavor = "multi_thread")]
async fn while_catching_up_a_community_held_only_for_its_groups_stays_in_its_window_as_they_change() {
    // A hold longer than the test, so the window keeps its order, reading only the rows it has.
    let bed = Bed::new(Timing { hold: Duration::from_secs(600), ..brisk() });
    bairro(&bed).await;
    bed.catching_up.store(true, Ordering::Relaxed);
    let mut watcher = bed.watcher(64);
    let unread = watcher.watch("chats.watch", json!({"folder": "unread", "around": "top", "after": 10})).await;
    assert_eq!(watcher.windows[&unread].rows[BAIRRO]["children_unread"], json!(3));
    bed.apply(vec![said(FESTA, "f3", AT + 50)]).await;
    while watcher.windows[&unread].rows[FESTA]["last"]["summary"] != "f3" {
        watcher.turn().await;
        watcher.drain();
    }
    let model = &watcher.windows[&unread];
    assert_eq!(model.rows[BAIRRO]["children_unread"], json!(4), "Bairro, held only for its groups, counts them still");
    summed(model, "held while catching up");
    bed.catching_up.store(false, Ordering::Relaxed);
}

/// A copy of jaw.db as a kill now would leave it, every commit so far and nothing more.
fn killed(home: &Path) -> tempfile::TempDir {
    let copy = tempfile::tempdir().unwrap();
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    db.execute("VACUUM INTO ?1", [copy.path().join("jaw.db").to_str().unwrap()]).unwrap();
    copy
}

/// What jaw.db holds of its chats, by JID, whatever the keys and versions the run gave them.
async fn chats_of(bed: &Bed) -> Vec<String> {
    bed.store
        .reads()
        .query(crate::work::Priority::Interactive, |db| {
            let mut statement = db.prepare(
                "SELECT id, kind, name, community, archived, unread, mentions, last_at, message_count FROM chats ORDER BY id",
            )?;
            let rows = statement.query_map([], |r| {
                let values: Vec<rusqlite::types::Value> = (0..9).map(|i| r.get(i)).collect::<rusqlite::Result<_>>()?;
                Ok(format!("{values:?}"))
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn communities_learned_over_many_commits_killed_after_any_end_as_a_run_never_killed() {
    // Groups first, each naming a community jaw learns of later; a group joining and one leaving;
    // the community learned again, renamed; and a group archived: a commit each.
    let script: Vec<Vec<Change>> = vec![
        vec![person(ANA, "Ana"), said(ANA, "n1", AT + 1)],
        vec![group(FESTA, "Festa", BAIRRO), said(FESTA, "f1", AT + 20), mentioned(FESTA, "f2", AT + 21)],
        vec![announcements(AVISOS, "Avisos", BAIRRO), said(AVISOS, "a1", AT + 10)],
        vec![community(BAIRRO, "Bairro")],
        vec![group(XADREZ, "Xadrez", VELA), said(XADREZ, "x1", AT + 30)],
        vec![linked(XADREZ, BAIRRO)],
        vec![linked(FESTA, "")],
        vec![community(BAIRRO, "Bairro de Alvalade"), community(VELA, "Vela")],
        vec![archived(AVISOS, true), group(REGATA, "Regata", VELA), said(REGATA, "g1", AT + 40)],
    ];
    let whole: Vec<Change> = script.iter().flatten().cloned().collect();
    let never = Bed::new(brisk());
    let mut watcher = never.watcher(64);
    let mut kills = Vec::new();
    for changes in &script {
        never.apply(changes.clone()).await;
        // What an interface held of each list as jawd was killed, to watch again once it's back.
        let mut held = Vec::new();
        for folder in ["all", "unread", "groups", "communities", "archived"] {
            held.push(Held::top(&mut watcher, folder).await);
        }
        kills.push((killed(never.home.path()), held));
    }
    // A reconnect brings it all again.
    never.apply(whole.clone()).await;
    let want = (chats_of(&never).await, shapes(&mut watcher).await);
    drift(&never, "never killed").await;
    for (k, (home, mut held)) in kills.into_iter().enumerate() {
        let store = Store::open(home.path()).unwrap();
        let catching_up = Arc::new(AtomicBool::new(false));
        let holding = catching_up.clone();
        let feed =
            Feed::keeping(store.clone(), Arc::new(move || holding.load(Ordering::Relaxed)), brisk(), CHANGES_KEPT);
        let bed = Bed { home, store, feed, catching_up };
        for changes in &script[k + 1..] {
            bed.apply(changes.clone()).await;
        }
        bed.apply(whole.clone()).await;
        let mut watcher = bed.watcher(64);
        let got = (chats_of(&bed).await, shapes(&mut watcher).await);
        assert_eq!(got, want, "killed after commit {k}");
        drift(&bed, &format!("killed after commit {k}")).await;
        for held in &mut held {
            let folder = held.params["folder"].clone();
            held.come_back(&mut watcher, &format!("killed after commit {k}, {folder} watched again")).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_community_whose_newest_group_leaves_it_sits_where_what_it_still_brings_does() {
    let bed = Bed::new(brisk());
    bed.apply(vec![
        person(RUI, "Rui"),
        community(BAIRRO, "Bairro"),
        announcements(AVISOS, "Avisos", BAIRRO),
        group(FESTA, "Festa", BAIRRO),
        said(AVISOS, "a1", AT + 10),
        said(RUI, "r1", AT + 30),
        said(FESTA, "f1", AT + 50),
    ])
    .await;
    let mut watcher = bed.watcher(64);
    let mut all = Held::top(&mut watcher, "all").await;
    assert_eq!(shape(&all.model), ["Bairro 2/0", "  Avisos", "  Festa", "Rui"]);
    bed.apply(vec![linked(FESTA, "")]).await;
    all.come_back(&mut watcher, "its newest group gone").await;
    assert_eq!(shape(&all.model), ["Festa", "Rui", "Bairro 1/0", "  Avisos"], "Bairro where Avisos is now");
}

/// How long a chat list's window takes to read with communities in it: 3,000 chats, 20
/// communities of 25 groups each, the newest chats among them, through the window's own query:
/// `cargo test --release -p jawd measure_a_chat_list_with_communities -- --ignored --nocapture`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a measurement"]
async fn measure_a_chat_list_with_communities() {
    let bed = Bed::new(brisk());
    let mut changes = Vec::new();
    for c in 0..20 {
        let id = format!("1203639{c:011}@g.us");
        changes.push(community(&id, &format!("Community {c}")));
        for g in 0..25 {
            let group = format!("1203638{:011}@g.us", c * 100 + g);
            changes.push(if g == 0 { announcements(&group, "Avisos", &id) } else { self::group(&group, "Group", &id) });
            changes.push(said(&group, &format!("g{c}-{g}"), AT + 10_000 + (c * 25 + g) as i64));
        }
    }
    for n in 0..2_500 {
        let chat = format!("3519{n:08}@s.whatsapp.net");
        changes.push(said(&chat, &format!("d{n}"), AT + n as i64 * 3));
    }
    bed.store.apply(Lane::Background, changes).await.unwrap();
    let mut watcher = bed.watcher(64);
    let communities: Vec<String> = (0..20).map(|c| format!("1203639{c:011}@g.us")).collect();
    for folder in ["all", "unread", "groups", "communities"] {
        for (open, says) in [
            (None, "every community open"),
            (Some(json!([])), "none open"),
            (Some(json!(communities[..1])), "one open"),
        ] {
            let mut params = json!({"folder": folder, "around": "top", "after": 60});
            if let Some(open) = &open {
                params["open"] = open.clone();
            }
            let mut took = Vec::new();
            for _ in 0..50 {
                let began = Instant::now();
                let held = Held::watch(&mut watcher, params.clone()).await;
                took.push(began.elapsed());
                assert!(!held.model.rows.is_empty());
            }
            took.sort();
            let rows = Held::watch(&mut watcher, params.clone()).await.model.rows.len();
            eprintln!(
                "{folder}, {says}: a window of 60 rows holding {rows}, read in {:.2} ms at p50 and {:.2} ms at p99 \
                 (a watch and an unwatch, through the client)",
                took[took.len() / 2].as_secs_f64() * 1000.0,
                took[took.len() * 99 / 100].as_secs_f64() * 1000.0,
            );
        }
    }
}
