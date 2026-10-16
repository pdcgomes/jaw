use std::collections::{BTreeMap, BTreeSet};

use super::*;

#[derive(Clone, Debug, PartialEq)]
struct Line {
    id: String,
    v: u64,
    sort: String,
    body: u64,
}

impl Row for Line {
    fn id(&self) -> &str {
        &self.id
    }
    fn v(&self) -> u64 {
        self.v
    }
    fn sort(&self) -> &str {
        &self.sort
    }
}

fn line(id: &str, v: u64, sort: &str) -> Line {
    Line { id: id.into(), v, sort: sort.into(), body: 0 }
}

fn push(remove: &[&str], upsert: Vec<Line>) -> Update<Line> {
    Update { remove: remove.iter().map(|id| id.to_string()).collect(), upsert, ..Update::default() }
}

fn ids<'a>(rows: impl Iterator<Item = &'a Line>) -> Vec<&'a str> {
    rows.map(|row| row.id.as_str()).collect()
}

/// A xorshift generator, so a failing seed can be run again.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

/// A window as jawd holds it at one seq: what a fresh watch would return.
#[derive(Clone, Debug)]
struct State {
    seq: u64,
    rows: BTreeMap<String, Line>,
    total: u64,
    older: bool,
    newer: u64,
}

/// Every row's newest version, as jaw.db has it, and the window's state over time.
struct World {
    rng: Rng,
    rows: BTreeMap<String, Line>,
    /// Every version each row has had, for the stale copies the test sends again.
    versions: BTreeMap<String, Vec<Line>>,
    state: State,
}

const IDS: usize = 80;

impl World {
    fn new(seed: u64) -> World {
        let mut rng = Rng::new(seed);
        let mut rows = BTreeMap::new();
        for n in 0..IDS {
            let id = format!("m{n:02}");
            let sort = format!("{:06}~{id}", rng.below(1_000_000));
            rows.insert(id.clone(), Line { id, v: 1, sort, body: 0 });
        }
        let versions = rows.iter().map(|(id, row)| (id.clone(), vec![row.clone()])).collect();
        let state = State { seq: 1, rows: BTreeMap::new(), total: 0, older: false, newer: 0 };
        World { rng, rows, versions, state }
    }

    fn some_id(&mut self) -> String {
        format!("m{:02}", self.rng.below(IDS))
    }

    /// Moves the world on a step: rows change, some keep their `v` and take another `sort` as a
    /// held chat list's do when it reorders, some swap places, and the window's rows come and go.
    fn step(&mut self) -> State {
        let seq = self.state.seq + 1 + self.rng.below(3) as u64;
        for _ in 0..self.rng.below(6) {
            let id = self.some_id();
            let row = self.rows.get_mut(&id).expect("a row");
            row.v = seq;
            row.body += 1;
            if self.rng.one_in(2) {
                row.sort = format!("{:06}~{id}", self.rng.below(1_000_000));
            }
            self.versions.get_mut(&id).expect("its versions").push(row.clone());
        }
        for _ in 0..self.rng.below(3) {
            let id = self.some_id();
            let row = self.rows.get_mut(&id).expect("a row");
            row.sort = format!("{:06}~{id}", self.rng.below(1_000_000));
            self.versions.get_mut(&id).expect("its versions").push(row.clone());
        }
        let mut members: BTreeSet<String> = self.state.rows.keys().cloned().collect();
        if self.rng.one_in(3) && members.len() >= 2 {
            let list: Vec<String> = members.iter().cloned().collect();
            let (a, b) = (list[self.rng.below(list.len())].clone(), list[self.rng.below(list.len())].clone());
            let (sa, sb) = (self.rows[&a].sort.clone(), self.rows[&b].sort.clone());
            for (id, sort) in [(&a, sb), (&b, sa)] {
                let row = self.rows.get_mut(id).expect("a row");
                row.sort = sort;
                row.v = seq;
                self.versions.get_mut(id).expect("its versions").push(row.clone());
            }
        }
        for _ in 0..self.rng.below(5) {
            let list: Vec<String> = members.iter().cloned().collect();
            if !list.is_empty() {
                members.remove(&list[self.rng.below(list.len())]);
            }
        }
        for _ in 0..self.rng.below(6) {
            members.insert(self.some_id());
        }
        let mut state = State {
            seq,
            rows: members.iter().map(|id| (id.clone(), self.rows[id].clone())).collect(),
            ..self.state.clone()
        };
        if self.rng.one_in(3) {
            state.total = state.rows.len() as u64 + self.rng.below(50) as u64;
        }
        if self.rng.one_in(4) {
            state.older = !state.older;
        }
        if self.rng.one_in(3) {
            state.newer = self.rng.below(1_001) as u64;
        }
        self.state = state.clone();
        state
    }
}

/// The update from one state to the next, as jawd sends it: the rows the client lacks or holds
/// otherwise, the ids that left, and `total`, `older` and `newer` when they changed, or always for
/// a watch's or move's result.
fn between(was: &State, now: &State, moved: bool) -> Update<Line> {
    let upsert = now.rows.values().filter(|row| was.rows.get(&row.id) != Some(*row)).cloned().collect();
    let remove = was.rows.keys().filter(|id| !now.rows.contains_key(*id)).cloned().collect();
    let changed = |a: u64, b: u64| (moved || a != b).then_some(b);
    Update {
        seq: now.seq,
        remove,
        upsert,
        total: changed(was.total, now.total),
        older: (moved || was.older != now.older).then_some(now.older),
        newer: changed(was.newer, now.newer),
        moved: moved.then(|| Moved { place: Place::around(format!("p{}", now.seq), 5, 5), newer: now.newer }),
    }
}

/// What the client receives for one update: its rows shuffled, sometimes split in two, sometimes
/// sent twice, and sometimes followed by stale copies of rows the window holds newer.
fn deliver(rng: &mut Rng, world: &World, update: Update<Line>) -> Vec<Update<Line>> {
    let mut update = update;
    rng.shuffle(&mut update.upsert);
    rng.shuffle(&mut update.remove);
    let mut parts = if rng.one_in(3) && update.upsert.len() + update.remove.len() >= 2 {
        let first = Update {
            remove: update.remove.split_off(update.remove.len() / 2),
            upsert: update.upsert.split_off(update.upsert.len() / 2),
            ..update.clone()
        };
        vec![first, update]
    } else {
        vec![update]
    };
    if rng.one_in(4) {
        let again = parts[rng.below(parts.len())].clone();
        parts.push(again);
    }
    if rng.one_in(3) {
        let stale: Vec<Line> = world
            .state
            .rows
            .values()
            .filter_map(|row| world.versions[&row.id].iter().find(|old| old.v < row.v).cloned())
            .take(3)
            .collect();
        if !stale.is_empty() {
            parts.push(Update { seq: 1, upsert: stale, ..Update::default() });
        }
    }
    parts
}

/// What a model must hold: the reference's rows in the list's order, and its counts.
fn expect_rows(state: &State, descending: bool) -> Vec<Line> {
    let mut rows: Vec<Line> = state.rows.values().cloned().collect();
    rows.sort_by(|a, b| (&a.sort, &a.id).cmp(&(&b.sort, &b.id)));
    if descending {
        rows.reverse();
    }
    rows
}

#[test]
fn window_models_match_a_reference_under_shuffled_and_repeated_updates() {
    for seed in 1..=300u64 {
        let mut world = World::new(seed);
        let mut rng = Rng::new(seed ^ 0xDEAD_BEEF);
        let mut chats: Chats<Line> = Chats::new("all", Place::top(30));
        let mut messages: Messages<Line> = Messages::new("chat", Place::newest(30));
        let mut direct: Messages<Line> = Messages::new("chat", Place::newest(30));
        let mut merged_chats: Merged<Line> = Merged::default();
        let mut merged_messages: Merged<Line> = Merged::default();
        let (mut chat_ids, mut message_ids) = (HashSet::new(), HashSet::new());
        let mut newer_then = 0;
        let mut was = world.state.clone();
        for step in 0..80 {
            let now = world.step();
            let moved = step == 0 || rng.one_in(6);
            if moved {
                newer_then = now.newer;
            }
            for part in deliver(&mut rng, &world, between(&was, &now, moved)) {
                direct.apply(part.clone());
                merged_chats.add(part.clone(), &mut chat_ids);
                merged_messages.add(part, &mut message_ids);
            }
            was = now;
            if rng.one_in(3) && step != 79 {
                continue;
            }
            let what = format!("seed {seed}, step {step}");
            assert!(
                merged_messages.remove.len() <= messages.len() && merged_messages.upsert.len() <= was.rows.len(),
                "{what}: a merge keeps no more than the window does"
            );
            chats.apply(std::mem::take(&mut merged_chats).into_update());
            messages.apply(std::mem::take(&mut merged_messages).into_update());
            let ascending = expect_rows(&was, false);
            assert_eq!(messages.rows().cloned().collect::<Vec<_>>(), ascending, "{what}: messages");
            assert_eq!(direct.rows().cloned().collect::<Vec<_>>(), ascending, "{what}: messages applied one by one");
            assert_eq!(chats.rows().cloned().collect::<Vec<_>>(), expect_rows(&was, true), "{what}: chats");
            assert_eq!(
                (messages.total, messages.older, messages.newer, chats.total),
                (was.total, was.older, was.newer, was.total),
                "{what}: total, older and newer"
            );
            assert_eq!(messages.arrived(), was.newer.saturating_sub(newer_then), "{what}: arrived since the move");
            assert_eq!(messages.seq, direct.seq, "{what}: seq");
            let mut held: Vec<String> = message_ids.iter().cloned().collect();
            held.sort();
            assert_eq!(held, was.rows.keys().cloned().collect::<Vec<_>>(), "{what}: the ids the client keeps");
        }
    }
}

#[test]
fn merging_two_updates_ends_where_applying_them_in_turn_does() {
    let cases: Vec<(Vec<Update<Line>>, Vec<&str>)> = vec![
        (vec![push(&[], vec![line("a", 5, "1")]), push(&["a"], vec![])], vec!["b@7"]),
        (vec![push(&["b"], vec![]), push(&[], vec![line("b", 3, "2")])], vec!["b@3"]),
        (
            vec![push(&[], vec![line("b", 9, "2")]), push(&["b"], vec![]), push(&[], vec![line("b", 4, "2")])],
            vec!["b@4"],
        ),
        (vec![push(&[], vec![line("b", 2, "2")])], vec!["b@7"]),
        (vec![push(&[], vec![line("b", 7, "5")])], vec!["b@7"]),
    ];
    for (n, (updates, want)) in cases.into_iter().enumerate() {
        let start = || {
            let mut m: Messages<Line> = Messages::new("c", Place::newest(5));
            m.apply(push(&[], vec![line("b", 7, "2")]));
            m
        };
        let (mut one_by_one, mut at_once) = (start(), start());
        let mut ids: HashSet<String> = ["b".to_string()].into();
        let mut merged = Merged::default();
        for update in updates {
            one_by_one.apply(update.clone());
            merged.add(update, &mut ids);
        }
        at_once.apply(merged.into_update());
        let held = |m: &Messages<Line>| m.rows().map(|r| format!("{}@{}", r.id, r.v)).collect::<Vec<_>>();
        assert_eq!(held(&one_by_one), want, "case {n}, one by one");
        assert_eq!(held(&at_once), want, "case {n}, merged");
        assert_eq!(one_by_one.rows().next().map(|r| &r.sort), at_once.rows().next().map(|r| &r.sort), "case {n}");
    }
}

#[test]
fn an_older_version_never_undoes_a_newer_one_and_the_same_version_resorts() {
    let mut list: Chats<Line> = Chats::new("all", Place::top(10));
    list.apply(push(&[], vec![line("ana", 1, "0~2"), line("rui", 1, "0~1")]));
    assert_eq!(ids(list.rows()), ["ana", "rui"]);
    list.apply(push(&[], vec![line("rui", 2, "0~3"), line("rui", 1, "0~1")]));
    assert_eq!(ids(list.rows()), ["rui", "ana"]);
    list.apply(push(&[], vec![line("rui", 2, "0~0")]));
    assert_eq!(ids(list.rows()), ["ana", "rui"], "the same v with a new sort, as a catch-up's reorder sends");
}

#[test]
fn a_held_chat_list_keeps_its_order_with_new_chats_first_until_released() {
    let mut list: Chats<Line> = Chats::new("all", Place::top(10));
    list.apply(push(&[], vec![line("ana", 1, "0~2"), line("rui", 1, "0~1"), line("mum", 1, "0~0")]));
    list.hold();
    list.apply(push(&["mum"], vec![line("rui", 2, "0~4"), line("sofia", 1, "0~5")]));
    assert_eq!(ids(list.rows()), ["sofia", "ana", "rui"], "held, with what came since first");
    assert_eq!(list.position("rui"), Some(2));
    assert!(list.release());
    assert!(!list.release());
    assert_eq!(ids(list.rows()), ["sofia", "rui", "ana"]);
    assert_eq!(list.position("rui"), Some(1));
}

#[test]
fn messages_read_oldest_first_and_say_what_left() {
    let mut w: Messages<Line> = Messages::new("ana", Place::newest(5));
    assert!(!w.loaded);
    w.apply(push(&[], vec![line("b", 1, "2"), line("a", 1, "1"), line("c", 1, "3")]));
    assert!(w.loaded);
    assert_eq!((w.oldest().unwrap().id.as_str(), w.newest().unwrap().id.as_str()), ("a", "c"));
    assert_eq!(w.after("1").unwrap().id, "b");
    assert_eq!(w.before("2").unwrap().id, "a");
    assert!(w.before("1").is_none());
    assert_eq!((w.count_after("1"), w.count_before("3", 1), w.count_before("3", 9)), (2, 1, 2));
    let gone = w.apply(push(&["a", "x"], vec![line("d", 1, "4")]));
    assert_eq!(gone, ["a"]);
    assert_eq!(w.nearest("1").unwrap().id, "b", "an anchor that went falls to the next");
    assert_eq!(w.nearest("9").unwrap().id, "d", "or to the newest");
    assert_eq!(ids(w.rows().rev()), ["d", "c", "b"]);
}

#[test]
fn what_arrived_counts_from_the_last_move_and_the_newest_is_followed_from_the_bottom() {
    let mut w: Messages<Line> = Messages::new("ana", Place::newest(5));
    let moved = |newer| Moved { place: Place::around("b", 1, 1), newer };
    w.apply(Update { newer: Some(40), moved: Some(moved(40)), ..push(&[], vec![line("b", 1, "2")]) });
    assert_eq!((w.arrived(), w.following(), w.holds_newest()), (0, false, false));
    w.apply(Update { newer: Some(42), ..Update::default() });
    assert_eq!(w.arrived(), 2, "two came since the window moved");
    w.apply(Update { newer: Some(3), moved: Some(moved(3)), ..Update::default() });
    assert_eq!(w.arrived(), 0);
    w.apply(Update { newer: Some(0), moved: Some(Moved { place: Place::newest(5), newer: 0 }), ..Update::default() });
    assert!(w.following() && w.holds_newest());
}
