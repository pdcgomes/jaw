//! A book's checks, its slots read as jawd knows their chats, and a tree and a book each read as
//! the other: on their own, then through `Layouts` and jaw.db.

use std::collections::BTreeSet;
use std::path::Path;

use jaw_proto::layout::{BOOK_BYTES_MOST, Named, SLOTS_MOST, Saved};
use jaw_proto::{Error, code};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};

use super::*;
use crate::layout::tests::{
    ANA, ANA_LID, CLIMBING, GONE, RUI, RUI_LID, asked, book, both, chat, count, exists, grid, layout, merges_run,
    opened, slot, split, updated_at,
};
use crate::layout::{Formatted, Layouts, answered};
use crate::store::{Change, Chat, Lane, Person, Store};

fn named(name: &str, grid: Grid, edited: Option<Grid>, focus: Option<u32>) -> Named {
    Named { name: name.into(), grid, edited, focus }
}

/// A book of the unnamed layout alone.
fn unnamed(grid: Grid) -> Book {
    Book { format: BOOK_FORMAT, current: None, unnamed: Some(grid), unnamed_focus: None, layouts: Vec::new() }
}

fn single(chat: Option<&str>) -> Grid {
    grid(&[0.0, 1.0], &[0.0, 1.0], vec![slot(1, [0, 1, 0, 1], chat)])
}

/// `n` slots side by side, each with a group of its own.
fn row_of(n: u32) -> Grid {
    let cols: Vec<f64> = (0..=n).map(|k| f64::from(k) / f64::from(n)).collect();
    grid(
        &cols,
        &[0.0, 1.0],
        (0..n).map(|k| slot(k + 1, [k, k + 1, 0, 1], Some(format!("{k}@g.us").as_str()))).collect(),
    )
}

/// Four slots turning around one in the middle, so no line crosses the grid.
fn pinwheel() -> Grid {
    let lines = [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0];
    let slots = vec![
        slot(1, [0, 2, 0, 1], Some("1@g.us")),
        slot(2, [2, 3, 0, 2], Some("2@g.us")),
        slot(3, [1, 3, 2, 3], Some("3@g.us")),
        slot(4, [0, 1, 1, 3], Some("4@g.us")),
        slot(5, [1, 2, 1, 2], Some("5@g.us")),
    ];
    grid(&lines, &lines, slots)
}

fn refused(book: &Book) -> (bool, &'static str, String) {
    match checked(book) {
        Ok(()) => panic!("kept: {book:?}"),
        Err(Refusal::Past(why)) => (true, "", why),
        Err(Refusal::Invalid { param, why }) => (false, param, why),
    }
}

/// Each slot as the chat it shows and the lines it spans, whatever its id.
fn places(grid: &Grid) -> BTreeSet<(Option<String>, [u32; 4])> {
    grid.slots.iter().map(|s| (s.chat.clone(), [s.c0, s.c1, s.r0, s.r1])).collect()
}

fn the_book(answer: &Option<Formatted>) -> &Book {
    match answer {
        Some(Formatted::Book(book)) => book,
        other => panic!("no book: {other:?}"),
    }
}

/// `named` for a jaw.db that knows `known`, each JID as the chat it reads as.
fn knowing(known: &'static [(&'static str, &'static str)]) -> impl FnMut(&str) -> anyhow::Result<Option<String>> {
    move |jid| Ok(known.iter().find(|(asked, _)| *asked == jid).map(|(_, chat)| chat.to_string()))
}

#[test]
fn every_shape_the_app_keeps_holds_a_pinwheel_and_12_in_a_row_among_them() {
    for grid in [single(Some(ANA)), single(None), row_of(12), pinwheel()] {
        assert_eq!(holds(&grid), Ok(()), "{grid:?}");
    }
    for grid in book().grids() {
        assert_eq!(holds(grid), Ok(()), "{grid:?}");
    }
    assert_eq!(checked(&book()), Ok(()));
}

#[test]
fn a_shape_the_app_couldnt_keep_is_refused_saying_why() {
    let two = |a: [u32; 4], b: [u32; 4]| {
        grid(&[0.0, 0.5, 1.0], &[0.0, 1.0], vec![slot(1, a, Some(ANA)), slot(2, b, Some(CLIMBING))])
    };
    let lines = |cols: &[f64], rows: &[f64]| grid(cols, rows, vec![slot(1, [0, 1, 0, 1], None)]);
    let cases = [
        (lines(&[0.1, 1.0], &[0.0, 1.0]), "columns' lines go from 0 to 1"),
        (lines(&[0.0, 0.9], &[0.0, 1.0]), "columns' lines go from 0 to 1"),
        (lines(&[0.0], &[0.0, 1.0]), "columns' lines go from 0 to 1"),
        (lines(&[0.0, 1.0], &[0.0, 0.0, 1.0]), "rows' lines go from 0 to 1"),
        (two([0, 1, 0, 1], [1, 3, 0, 1]), "slot 2 spans lines the grid doesn't have"),
        (two([0, 1, 0, 1], [1, 1, 0, 1]), "slot 2 spans lines the grid doesn't have, or none"),
        (two([0, 1, 0, 1], [0, 1, 0, 1]), "no holes and no overlaps"),
        (grid(&[0.0, 0.5, 1.0], &[0.0, 1.0], vec![slot(1, [0, 1, 0, 1], None)]), "no holes and no overlaps"),
        (grid(&[0.0, 0.5, 1.0], &[0.0, 1.0], vec![slot(1, [0, 2, 0, 1], None)]), "every line inside it is"),
        (grid(&[0.0, 0.2, 0.4, 0.6, 1.0], &[0.0, 1.0], vec![slot(1, [0, 4, 0, 1], None)]), "every line inside it is"),
        (
            grid(&[0.0, 0.5, 1.0], &[0.0, 1.0], vec![slot(1, [0, 1, 0, 1], None), slot(1, [1, 2, 0, 1], None)]),
            "two slots are numbered 1",
        ),
        (two([0, 1, 0, 1], [1, 2, 0, 1]).tap(|g| g.slots[1].chat = Some("Climbing".into())), "\"Climbing\" isn't one"),
        (two([0, 1, 0, 1], [1, 2, 0, 1]).tap(|g| g.slots[1].chat = Some(ANA.into())), "is in two"),
    ];
    for (grid, why) in cases {
        let said = holds(&grid).unwrap_err();
        assert!(said.contains(why), "{grid:?}: {said}");
        assert_eq!(refused(&unnamed(grid.clone())).1, "layout.unnamed");
        let in_work = Book { layouts: vec![named("Work", single(None), Some(grid), None)], ..unnamed(single(None)) };
        let (past, param, said) = refused(&in_work);
        assert_eq!((past, param), (false, "layout.layouts"), "{said}");
        assert!(said.starts_with("\"Work\"'s edited shape: "), "{said}");
    }
}

/// A value changed in place, for tables of them.
trait Tap: Sized {
    fn tap(mut self, change: impl FnOnce(&mut Self)) -> Self {
        change(&mut self);
        self
    }
}
impl<T> Tap for T {}

#[test]
fn a_book_past_nine_named_layouts_or_a_shape_past_12_slots_is_refused_as_a_limit() {
    let layouts = |n: usize| (0..n).map(|k| named(&format!("L{k}"), single(None), None, None)).collect::<Vec<_>>();
    let nine = Book { layouts: layouts(LAYOUTS_MOST - 1), ..unnamed(row_of(SLOTS_MOST as u32)) };
    assert_eq!(checked(&nine), Ok(()), "the unnamed layout and nine named, of 12 slots");
    let ten = Book { layouts: layouts(LAYOUTS_MOST), ..unnamed(single(None)) };
    let (past, _, why) = refused(&ten);
    assert!(past, "{why}");
    assert!(why.contains("10 layouts at most, its unnamed one and 9 named, and this one names 10"), "{why}");
    let thirteen = row_of(SLOTS_MOST as u32 + 1);
    assert!(refused(&unnamed(thirteen.clone())).0);
    let saved = Book { layouts: vec![named("Work", thirteen.clone(), None, None)], ..unnamed(single(None)) };
    let (past, _, why) = refused(&saved);
    assert!(past && why.contains("\"Work\" has 13"), "{why}");
    let edited = Book { layouts: vec![named("Work", single(None), Some(thirteen), None)], ..unnamed(single(None)) };
    let (past, _, why) = refused(&edited);
    assert!(past && why.contains("\"Work\"'s edited shape has 13"), "{why}");
}

#[test]
fn a_book_names_each_layout_once_in_1_to_64_bytes_and_its_current_layout_is_one_of_them() {
    let with = |names: &[&str], current: Option<&str>| Book {
        current: current.map(str::to_string),
        layouts: names.iter().map(|name| named(name, single(None), None, None)).collect(),
        ..unnamed(single(None))
    };
    let sixty_four = "ã".repeat(32);
    let sixty_five = format!("{sixty_four}x");
    assert_eq!(
        checked(&with(&[sixty_four.as_str(), "Work", "work"], Some("work"))),
        Ok(()),
        "case is the app's to mind"
    );
    assert_eq!(checked(&with(&["Work"], None)), Ok(()), "the unnamed layout on screen");
    for (book, param, why) in [
        (with(&[""], None), "layout.layouts", "1 to 64 bytes"),
        (with(&[sixty_five.as_str()], None), "layout.layouts", "1 to 64 bytes"),
        (with(&["Work", "Family", "Work"], None), "layout.layouts", "\"Work\" is named twice"),
        (with(&["Work"], Some("Family")), "layout.current", "it names none \"Family\""),
    ] {
        let (past, said_param, said) = refused(&book);
        assert_eq!((past, said_param), (false, param), "{said}");
        assert!(said.contains(why), "{said}");
    }
}

#[test]
fn a_focus_on_no_slot_is_kept_as_it_was_given() {
    let mut book = book();
    book.unnamed_focus = Some(99);
    book.layouts[0].focus = Some(7);
    assert_eq!(checked(&book), Ok(()));
}

#[test]
fn a_tree_reads_as_a_grid_laid_out_in_the_unit_square_each_pane_a_slot_numbered_in_the_trees_order() {
    let readme = layout(2, split(How::Across, 500, chat(ANA), split(How::Down, 600, chat(CLIMBING), chat(RUI))));
    let cases = [
        (layout(0, chat(ANA)), grid(&[0.0, 1.0], &[0.0, 1.0], vec![slot(1, [0, 1, 0, 1], Some(ANA))]), 1),
        (
            both(),
            grid(
                &[0.0, 0.5, 1.0],
                &[0.0, 1.0],
                vec![slot(1, [0, 1, 0, 1], Some(ANA)), slot(2, [1, 2, 0, 1], Some(CLIMBING))],
            ),
            2,
        ),
        (
            readme,
            grid(
                &[0.0, 0.5, 1.0],
                &[0.0, 0.6, 1.0],
                vec![
                    slot(1, [0, 1, 0, 2], Some(ANA)),
                    slot(2, [1, 2, 0, 1], Some(CLIMBING)),
                    slot(3, [1, 2, 1, 2], Some(RUI)),
                ],
            ),
            3,
        ),
        // Two splits down at the same height meet in one line.
        (
            layout(
                0,
                split(
                    How::Across,
                    500,
                    split(How::Down, 500, chat("a@x"), chat("b@x")),
                    split(How::Down, 500, chat("c@x"), chat("d@x")),
                ),
            ),
            grid(
                &[0.0, 0.5, 1.0],
                &[0.0, 0.5, 1.0],
                vec![
                    slot(1, [0, 1, 0, 1], Some("a@x")),
                    slot(2, [0, 1, 1, 2], Some("b@x")),
                    slot(3, [1, 2, 0, 1], Some("c@x")),
                    slot(4, [1, 2, 1, 2], Some("d@x")),
                ],
            ),
            1,
        ),
        // At different heights, each is a line of its own, and the slots beside span both.
        (
            layout(
                3,
                split(
                    How::Across,
                    250,
                    split(How::Down, 300, chat("a@x"), chat("b@x")),
                    split(How::Down, 700, chat("c@x"), chat("d@x")),
                ),
            ),
            grid(
                &[0.0, 0.25, 1.0],
                &[0.0, 0.3, 0.7, 1.0],
                vec![
                    slot(1, [0, 1, 0, 1], Some("a@x")),
                    slot(2, [0, 1, 1, 3], Some("b@x")),
                    slot(3, [1, 2, 0, 2], Some("c@x")),
                    slot(4, [1, 2, 2, 3], Some("d@x")),
                ],
            ),
            4,
        ),
    ];
    for (tree, grid, focus) in cases {
        let (book, dropped) = of_tree(tree.clone());
        let expected = Book { unnamed_focus: Some(focus), ..unnamed(grid) };
        assert_eq!((&book, dropped.len()), (&expected, 0), "{tree:?}");
        assert_eq!(checked(&book), Ok(()), "a book jawd would keep");
    }
}

#[test]
fn a_pane_too_small_for_any_screen_is_left_out_of_the_grid_and_named_and_the_keys_go_before_it() {
    // Each split gives its second side a thousandth of what it splits, so by the sixth the panes
    // there are narrower than a double can tell apart near the grid's right edge.
    let chats: Vec<String> = (0..8).map(|k| format!("{k}@g.us")).collect();
    let tree = chats.iter().rev().skip(1).fold(chat(&chats[7]), |tree, jid| split(How::Across, 999, chat(jid), tree));
    let (book, dropped) = of_tree(layout(7, tree));
    let grid = book.unnamed.as_ref().unwrap();
    assert_eq!(holds(grid), Ok(()), "{grid:?}");
    assert!(!dropped.is_empty(), "some pane rounds to nothing: {grid:?}");
    let shown: Vec<String> = grid.slots.iter().filter_map(|s| s.chat.clone()).collect();
    let in_order = |some: &[String]| some.iter().map(|jid| chats.iter().position(|c| c == jid)).is_sorted();
    assert!(in_order(&shown) && in_order(&dropped), "each in the tree's order: {shown:?} {dropped:?}");
    let mut every = [shown, dropped].concat();
    every.sort();
    assert_eq!(every, chats, "each pane a slot or named, once");
    let last = grid.slots.last().unwrap();
    assert_eq!(book.unnamed_focus, Some(last.id), "the keys on the last pane, or the last left before it");
}

#[test]
fn a_grid_reads_as_a_tree_cut_along_the_first_line_no_slot_crosses_and_a_pinwheel_as_none() {
    // Work as edited: the group on the left, Ana over an empty slot on the right, the keys on it.
    let tree = tree_of(&book()).unwrap();
    let expected = layout(2, split(How::Across, 300, chat(CLIMBING), split(How::Down, 667, chat(ANA), chat(""))));
    assert_eq!(tree, expected);
    // Its saved shape: the group over Ana and the empty slot, so the first cut is a row's.
    let saved = Book { layouts: vec![Named { edited: None, ..book().layouts[0].clone() }], ..book() };
    let expected = layout(2, split(How::Down, 400, chat(CLIMBING), split(How::Across, 333, chat(ANA), chat(""))));
    assert_eq!(tree_of(&saved), Some(expected));
    let mut on_unnamed = book();
    on_unnamed.current = None;
    assert_eq!(tree_of(&on_unnamed), Some(layout(1, split(How::Across, 500, chat(ANA), chat(CLIMBING)))));
    on_unnamed.unnamed_focus = Some(42);
    assert_eq!(tree_of(&on_unnamed).unwrap().focused, 0, "a focus on no slot reads as the first pane");
    on_unnamed.unnamed = None;
    assert_eq!(tree_of(&on_unnamed), None, "nothing on screen");
    assert_eq!(tree_of(&unnamed(pinwheel())), None);
    let twelve = tree_of(&unnamed(row_of(12))).unwrap();
    assert_eq!(twelve.chats(), (0..12).map(|k| format!("{k}@g.us")).collect::<Vec<_>>());
}

#[test]
fn both_ways_a_tree_reads_back_from_its_grid_as_itself_and_a_grid_from_its_tree_as_itself_to_a_thousandth() {
    let tree_trees = [
        layout(0, chat(ANA)),
        both(),
        layout(2, split(How::Across, 500, chat(ANA), split(How::Down, 600, chat(CLIMBING), chat(RUI)))),
        layout(
            3,
            split(
                How::Down,
                250,
                split(How::Across, 400, chat("a@x"), chat("b@x")),
                split(How::Across, 750, chat("c@x"), chat("d@x")),
            ),
        ),
        layout(
            3,
            split(
                How::Across,
                300,
                chat("a@x"),
                split(How::Across, 500, chat("b@x"), split(How::Down, 800, chat("c@x"), chat("d@x"))),
            ),
        ),
        layout(
            1,
            split(
                How::Across,
                500,
                split(How::Down, 500, chat("a@x"), chat("b@x")),
                split(How::Down, 500, chat("c@x"), chat("d@x")),
            ),
        ),
    ];
    for tree in tree_trees {
        let (book, _) = of_tree(tree.clone());
        assert_eq!(tree_of(&book), Some(tree.clone()), "a tree, as a grid, then a tree again");
    }
    // Split down first, then across at the same place: its grid's first cut is the column's, so it
    // reads back as another tree with the same panes in the same places, the keys on the same pane.
    let down_first = layout(
        1,
        split(
            How::Down,
            500,
            split(How::Across, 500, chat("a@x"), chat("b@x")),
            split(How::Across, 500, chat("c@x"), chat("d@x")),
        ),
    );
    let (laid_out, _) = of_tree(down_first.clone());
    let back = tree_of(&laid_out).unwrap();
    assert_eq!(
        back,
        layout(
            2,
            split(
                How::Across,
                500,
                split(How::Down, 500, chat("a@x"), chat("c@x")),
                split(How::Down, 500, chat("b@x"), chat("d@x"))
            )
        )
    );
    let (again, _) = of_tree(back);
    let [first, second] = [&laid_out, &again].map(|book| book.unnamed.clone().unwrap());
    assert_eq!((&first.cols, &first.rows, places(&first)), (&second.cols, &second.rows, places(&second)));
    // A grid that's a tree, as a tree and as a grid again: the same slots on the same lines, each
    // line where it was to within a thousandth of what its split divides, as shares are kept.
    for grid in
        [book().unnamed.unwrap(), book().layouts[0].grid.clone(), book().layouts[0].edited.clone().unwrap(), row_of(12)]
    {
        let tree = tree_of(&unnamed(grid.clone())).unwrap();
        let (back, _) = of_tree(
            super::super::resolved(tree, |jid| Ok((!jid.is_empty()).then(|| jid.to_string()))).unwrap().0.unwrap(),
        );
        let back = back.unnamed.unwrap();
        let shown = |grid: &Grid| places(grid).into_iter().filter(|(chat, _)| chat.is_some()).collect::<BTreeSet<_>>();
        if grid.slots.iter().all(|slot| slot.chat.is_some()) {
            assert_eq!(places(&back), places(&grid), "{grid:?}");
            for (now, was) in back.cols.iter().chain(&back.rows).zip(grid.cols.iter().chain(&grid.rows)) {
                assert!((now - was).abs() < 1e-3, "{now} for {was}: {grid:?}");
            }
        } else {
            // An empty slot isn't a pane, so the tree closes its split and its neighbour grows.
            assert!(shown(&grid).len() == back.slots.len() && back.slots.len() < grid.slots.len(), "{back:?}");
        }
    }
}

#[test]
fn a_book_reads_each_slot_as_jawd_knows_its_chat_emptying_one_gone_or_shown_before_it_in_reading_order() {
    let known = knowing(&[(ANA, ANA), (ANA_LID, ANA), (CLIMBING, CLIMBING), (RUI, RUI)]);
    // In Work, the slot below comes first in the list but second in reading order, and both read as
    // Ana; Family shows Ana too, which is its own business; the unnamed layout has a chat jaw.db
    // doesn't have, twice over in Family's edited shape.
    let work = grid(
        &[0.0, 1.0],
        &[0.0, 0.5, 1.0],
        vec![slot(7, [0, 1, 1, 2], Some(ANA)), slot(3, [0, 1, 0, 1], Some(ANA_LID))],
    );
    let family_edit =
        grid(&[0.0, 0.5, 1.0], &[0.0, 1.0], vec![slot(1, [0, 1, 0, 1], Some(GONE)), slot(2, [1, 2, 0, 1], Some(RUI))]);
    let book = Book {
        format: BOOK_FORMAT,
        current: Some("Family".into()),
        unnamed: Some(single(Some(GONE))),
        unnamed_focus: Some(1),
        layouts: vec![
            named("Work", work, None, Some(7)),
            named("Family", single(Some(ANA)), Some(family_edit), Some(2)),
        ],
    };
    let (read, dropped) = resolved(book.clone(), known).unwrap();
    let mut expected = book;
    expected.unnamed.as_mut().unwrap().slots[0].chat = None;
    expected.layouts[0].grid.slots[0].chat = None;
    expected.layouts[0].grid.slots[1].chat = Some(ANA.into());
    expected.layouts[1].edited.as_mut().unwrap().slots[0].chat = None;
    assert_eq!(read, expected, "the slots in their order and places, the focus and the names as they were");
    assert_eq!(dropped, [GONE, ANA], "each JID once, as the book names it");
}

#[test]
fn a_reader_of_one_format_is_answered_with_a_layout_kept_in_the_other_left_out_as_a_reader_of_trees_would() {
    let known = || knowing(&[(ANA, ANA), (ANA_LID, ANA), (CLIMBING, CLIMBING)]);
    // A tree with a pane on a chat jaw.db doesn't have: left out, its split closed, then laid out.
    let tree = layout(2, split(How::Across, 400, chat(ANA_LID), split(How::Down, 600, chat(GONE), chat(CLIMBING))));
    let (book, dropped) = answered(Formatted::Tree(tree), BOOK_FORMAT, known()).unwrap();
    let expected = Book {
        unnamed_focus: Some(2),
        ..unnamed(grid(
            &[0.0, 0.4, 1.0],
            &[0.0, 1.0],
            vec![slot(1, [0, 1, 0, 1], Some(ANA)), slot(2, [1, 2, 0, 1], Some(CLIMBING))],
        ))
    };
    assert_eq!((the_book(&book), dropped), (&expected, vec![GONE.to_string()]));
    // The book's Work as edited, as a tree: the empty slot is no pane, and the keys go before it.
    let (tree, dropped) = answered(Formatted::Book(super::super::tests::book()), FORMAT, known()).unwrap();
    let expected = layout(1, split(How::Across, 300, chat(CLIMBING), chat(ANA)));
    assert_eq!((tree, dropped), (Some(Formatted::Tree(expected)), Vec::new()));
    let (none, dropped) = answered(Formatted::Book(unnamed(pinwheel())), FORMAT, known()).unwrap();
    assert_eq!((none, dropped), (None, Vec::new()), "a pinwheel isn't a tree");
    let lid = Book { unnamed_focus: Some(1), ..unnamed(single(Some(ANA_LID))) };
    let (tree, _) = answered(Formatted::Book(lid), FORMAT, known()).unwrap();
    assert_eq!(tree, Some(Formatted::Tree(layout(0, chat(ANA)))), "its slot's chat as jawd knows it");
    let gone = unnamed(single(Some(GONE)));
    assert_eq!(answered(Formatted::Book(gone), FORMAT, known()).unwrap(), (None, vec![GONE.to_string()]));
}

fn fixture(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../protocol/fixtures").join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_protocols_layout_fixtures_are_what_jawd_answers() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let rui = "351912222222@s.whatsapp.net";
    // Rui by his number and by his LID, whose chat is his number's; Ana; the group.
    let changes = vec![
        Change::Person(Person { jids: vec![rui.into(), "100000000002@lid".into()], ..Person::default() }),
        Change::Chat(Chat { id: rui.into(), kind: Some("direct"), ..Chat::default() }),
        Change::Chat(Chat { id: ANA.into(), kind: Some("direct"), ..Chat::default() }),
        Change::Chat(Chat { id: CLIMBING.into(), ..Chat::default() }),
    ];
    store.apply(Lane::Live, changes).await.unwrap();
    let layouts = Layouts::new(store);
    let answer = layouts.set("jaw-mac", 1, fixture("layout.set.request.book")["params"].clone()).await.unwrap();
    assert_eq!(serde_json::to_value(&answer).unwrap(), fixture("layout.set.result.book")["result"]);
    // The terminal's panes, the third on a chat this account doesn't have, as a tree and a grid.
    let three = layout(1, split(How::Across, 500, chat(ANA), split(How::Down, 500, chat(CLIMBING), chat(GONE))));
    layouts.set("jaw", 2, asked(1_792_000_000_123, &three)).await.unwrap();
    for name in ["layout.get", "layout.get.book"] {
        let [request, result] = ["request", "result"].map(|kind| {
            let (method, example) = name.split_once(".book").map_or((name, ""), |(method, _)| (method, ".book"));
            fixture(&format!("{method}.{kind}{example}"))
        });
        let answer = layouts.get("jaw-mac", request["params"].clone()).await.unwrap();
        assert_eq!(serde_json::to_value(&answer).unwrap(), result["result"], "{name}");
    }
    let mut told = layouts.subscribe("jaw-mac").unwrap();
    let push = fixture("layout.push.book");
    layouts.set("jaw-mac", 3, json!({ "v": push["params"]["v"], "layout": push["params"]["layout"] })).await.unwrap();
    let pushed = told.borrow_and_update().clone().unwrap();
    assert_eq!(serde_json::to_value(&pushed.saved).unwrap(), push["params"]);
}

fn kept(saved: &Saved<Formatted>) -> (u64, Option<bool>, Option<&Formatted>) {
    (saved.v, saved.kept, saved.layout.as_ref())
}

fn raw(home: &Path, interface: &str) -> Option<String> {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.query_row("SELECT value FROM interface_state WHERE interface = ?1", [interface], |r| r.get(0))
        .optional()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_book_is_kept_with_a_synced_commit_as_its_compact_json_and_read_back_key_for_key() {
    let home = tempfile::tempdir().unwrap();
    let (store, layouts) = opened(home.path(), &[ANA, CLIMBING]).await;
    let synced = store.storage().synced_commits;
    let saved = layouts.set("jaw-mac", 1, asked(5, &book())).await.unwrap();
    assert_eq!(kept(&saved), (5, Some(true), Some(&Formatted::Book(book()))));
    assert_eq!(store.storage().synced_commits - synced, 1, "kept with a synced commit");
    assert_eq!(raw(home.path(), "jaw-mac"), Some(serde_json::to_string(&book()).unwrap()));
    for params in [Value::Null, json!({ "format": 2 }), json!({ "interface": "jaw-mac", "format": 2 })] {
        let read = layouts.get("jaw-mac", params.clone()).await.unwrap();
        assert_eq!(kept(&read), (5, None, Some(&Formatted::Book(book()))), "{params}");
    }
    let json = serde_json::to_value(&layouts.get("jaw-mac", Value::Null).await.unwrap()).unwrap();
    assert_eq!(json, json!({ "interface": "jaw-mac", "v": 5, "layout": book() }), "the book as the app wrote it");
    let status = layouts.status();
    let bytes = serde_json::to_string(&book()).unwrap().len() as u64;
    assert_eq!((status.kept, status.books, status.largest_book_bytes, status.largest_bytes), (1, 1, bytes, 0));
    assert_eq!(status.book_bytes_most, BOOK_BYTES_MOST as u64);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_books_lines_come_back_as_the_numbers_they_were_written_as() {
    let home = tempfile::tempdir().unwrap();
    let (_store, layouts) = opened(home.path(), &[]).await;
    // Elevenths and sevenths, which a parser that isn't exact reads back as their neighbours.
    let sevenths: Vec<f64> = (0..=7).map(|k| f64::from(k) / 7.0).collect();
    let down = grid(&[0.0, 1.0], &sevenths, (0..7).map(|k| slot(k + 1, [0, 1, k, k + 1], None)).collect());
    let book = Book { layouts: vec![named("Lines", down, None, None)], ..unnamed(row_of(11)) };
    let line = book.unnamed.as_ref().unwrap().cols[1];
    assert_eq!(serde_json::to_string(&line).unwrap(), "0.09090909090909091");
    layouts.set("jaw-mac", 1, json!({ "v": 1, "layout": book })).await.unwrap();
    let text = serde_json::to_string(&layouts.get("jaw-mac", Value::Null).await.unwrap()).unwrap();
    let read: Saved<Book> = serde_json::from_str(&text).unwrap();
    let bits = |book: &Book| -> Vec<u64> {
        book.grids().flat_map(|g| g.cols.iter().chain(&g.rows)).map(|x| x.to_bits()).collect()
    };
    assert_eq!(bits(read.layout.as_ref().unwrap()), bits(&book), "each line the very number it was");
}

#[tokio::test(flavor = "multi_thread")]
async fn only_a_newer_layout_replaces_a_book_whatever_its_format_so_asking_again_or_late_changes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let (_store, layouts) = opened(home.path(), &[ANA, CLIMBING]).await;
    let mut other = layouts.subscribe("jaw-mac").unwrap();
    let mut first = book();
    first.current = None;
    layouts.set("jaw-mac", 1, asked(10, &book())).await.unwrap();
    let pushed = other.borrow_and_update().clone().unwrap();
    assert_eq!((pushed.from, kept(&pushed.saved)), (1, (10, None, Some(&Formatted::Book(book())))), "pushed as a book");
    let at = updated_at(home.path(), "jaw-mac");

    let again = layouts.set("jaw-mac", 1, asked(10, &book())).await.unwrap();
    assert_eq!(kept(&again), (10, Some(true), Some(&Formatted::Book(book()))), "asked again: kept");
    let late = layouts.set("jaw-mac", 2, asked(9, &first)).await.unwrap();
    assert_eq!(kept(&late), (10, Some(false), Some(&Formatted::Book(book()))), "an older one, late");
    let rival = layouts.set("jaw-mac", 2, asked(10, &first)).await.unwrap();
    assert_eq!(kept(&rival), (10, Some(false), Some(&Formatted::Book(book()))), "the first of a version stays");
    let tree = layouts.set("jaw-mac", 2, asked(10, &both())).await.unwrap();
    let as_tree = layout(1, split(How::Across, 300, chat(CLIMBING), chat(ANA)));
    assert_eq!(
        kept(&tree),
        (10, Some(false), Some(&Formatted::Tree(as_tree))),
        "a tree of that version, in its format"
    );
    assert_eq!(updated_at(home.path(), "jaw-mac"), at, "none of them changed the row");
    assert!(!other.has_changed().unwrap(), "so none was pushed");

    // A newer tree replaces the book and is pushed as a tree; a book late after it is answered with
    // the tree read as a book.
    let newer = layouts.set("jaw-mac", 3, asked(11, &both())).await.unwrap();
    assert_eq!(kept(&newer), (11, Some(true), Some(&Formatted::Tree(both()))));
    let pushed = other.borrow_and_update().clone().unwrap();
    assert_eq!((pushed.from, kept(&pushed.saved)), (3, (11, None, Some(&Formatted::Tree(both())))));
    let stale = layouts.set("jaw-mac", 1, asked(10, &book())).await.unwrap();
    let (as_book, _) = of_tree(both());
    assert_eq!(kept(&stale), (11, Some(false), Some(&Formatted::Book(as_book))), "the tree, as a book");
    let newest = layouts.set("jaw-mac", 1, asked(12, &first)).await.unwrap();
    assert_eq!(kept(&newest), (12, Some(true), Some(&Formatted::Book(first))));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_mac_opens_the_terminals_tree_as_a_grid_and_the_terminal_the_macs_layout_on_screen_as_a_tree() {
    let home = tempfile::tempdir().unwrap();
    let (_store, layouts) = opened(home.path(), &[ANA, CLIMBING, RUI]).await;
    let readme = layout(2, split(How::Across, 500, chat(ANA), split(How::Down, 600, chat(CLIMBING), chat(RUI))));
    layouts.set("jaw", 1, asked(4, &readme)).await.unwrap();
    let read = layouts.get("jaw-mac", json!({ "interface": "jaw", "format": 2 })).await.unwrap();
    let (grid, _) = of_tree(readme.clone());
    assert_eq!((read.interface.as_str(), kept(&read)), ("jaw", (4, None, Some(&Formatted::Book(grid)))));
    let same = layouts.get("jaw-mac", json!({ "interface": "jaw", "format": 1 })).await.unwrap();
    assert_eq!(kept(&same), (4, None, Some(&Formatted::Tree(readme))));
    layouts.set("jaw-mac", 1, asked(9, &book())).await.unwrap();
    let read = layouts.get("jaw", json!({ "interface": "jaw-mac", "format": 1 })).await.unwrap();
    let expected = layout(1, split(How::Across, 300, chat(CLIMBING), chat(ANA)));
    assert_eq!(kept(&read), (9, None, Some(&Formatted::Tree(expected))));
    let pinwheel = Book { unnamed_focus: Some(5), ..unnamed(pinwheel()) };
    layouts.set("jaw-mac", 1, asked(10, &pinwheel)).await.unwrap();
    let read = layouts.get("jaw", json!({ "interface": "jaw-mac", "format": 1 })).await.unwrap();
    assert_eq!((kept(&read), read.dropped.len()), ((10, None, None), 0), "a pinwheel is no tree");
    let e = layouts.get("jaw", json!({ "format": 3 })).await.unwrap_err();
    assert_eq!((e.code, e.data), (code::INVALID_PARAMS, Some(json!({ "param": "format", "allowed": [1, 2] }))));
}

#[tokio::test(flavor = "multi_thread")]
async fn exactly_nine_named_layouts_12_slots_and_64_kib_are_kept_and_one_past_each_is_error_5_with_nothing_quarantined()
{
    let home = tempfile::tempdir().unwrap();
    let (_store, layouts) = opened(home.path(), &[]).await;
    let full = |n: usize| Book {
        layouts: (0..n).map(|k| named(&format!("L{k}"), row_of(12), Some(row_of(12)), Some(1))).collect(),
        ..unnamed(row_of(12))
    };
    let at_limits = layouts.set("jaw-mac", 1, asked(1, &full(LAYOUTS_MOST - 1))).await.unwrap();
    assert_eq!(at_limits.kept, Some(true), "nine named and the unnamed, each shape of 12 slots");
    let e = layouts.set("jaw-mac", 1, asked(2, &full(LAYOUTS_MOST))).await.unwrap_err();
    assert_eq!(e.code, code::LIMIT, "{}", e.message);
    let e = layouts.set("jaw-mac", 1, asked(2, &unnamed(row_of(13)))).await.unwrap_err();
    assert_eq!(e.code, code::LIMIT, "{}", e.message);
    // One slot whose JID makes the compact JSON exactly BOOK_BYTES_MOST bytes, then one byte more.
    let sized = |bytes: usize| {
        let empty = serde_json::to_string(&unnamed(single(Some("@s.whatsapp.net")))).unwrap().len();
        unnamed(single(Some(&format!("{}@s.whatsapp.net", "9".repeat(bytes - empty)))))
    };
    assert_eq!(serde_json::to_string(&sized(BOOK_BYTES_MOST)).unwrap().len(), BOOK_BYTES_MOST);
    let at_most = layouts.set("jaw-mac", 1, asked(3, &sized(BOOK_BYTES_MOST))).await.unwrap();
    assert_eq!(at_most.kept, Some(true), "65,536 bytes");
    let e = layouts.set("jaw-mac", 1, asked(4, &sized(BOOK_BYTES_MOST + 1))).await.unwrap_err();
    assert_eq!(e.code, code::LIMIT, "{}", e.message);
    assert_eq!(count(home.path(), "quarantine"), 0, "refused before the writer, never quarantined");
    let status = layouts.status();
    assert_eq!((status.refused, status.largest_book_bytes, status.books), (3, BOOK_BYTES_MOST as u64, 1));
    // A book at every limit, each slot's chat a JID of 64 bytes, fits with room to spare.
    let mut biggest = full(LAYOUTS_MOST - 1);
    let long = |k: u32| format!("{k:0>49}@s.whatsapp.net");
    for grid in std::iter::once(biggest.unnamed.as_mut().unwrap())
        .chain(biggest.layouts.iter_mut().flat_map(|n| std::iter::once(&mut n.grid).chain(n.edited.as_mut())))
    {
        for slot in &mut grid.slots {
            slot.chat = Some(long(slot.id));
        }
    }
    assert_eq!(long(1).len(), 64);
    let bytes = serde_json::to_string(&biggest).unwrap().len();
    assert!(bytes < BOOK_BYTES_MOST * 2 / 3, "{bytes} bytes");
    assert_eq!(layouts.set("jaw-mac", 1, asked(5, &biggest)).await.unwrap().kept, Some(true));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_book_is_refused_naming_its_param_and_nothing_is_kept() {
    let home = tempfile::tempdir().unwrap();
    let (_store, layouts) = opened(home.path(), &[ANA]).await;
    let param = |e: Error| (e.code, e.data.and_then(|d| d["param"].as_str().map(str::to_string)));
    let mut unknown = serde_json::to_value(book()).unwrap();
    unknown["layouts"][0]["colour"] = json!("red");
    let mut no_format = serde_json::to_value(book()).unwrap();
    no_format.as_object_mut().unwrap().remove("format");
    let mut not_current = book();
    not_current.current = Some("Family".into());
    let mut v1 = serde_json::to_value(book()).unwrap();
    v1["v"] = json!(1);
    let cases = [
        (json!({ "v": 1, "layout": unknown }), "layout"),
        (json!({ "v": 1, "layout": v1 }), "layout"),
        (json!({ "v": 1, "layout": no_format }), "layout.format"),
        (json!({ "v": 1, "layout": not_current }), "layout.current"),
        (
            json!({ "v": 1, "layout": unnamed(pinwheel()).tap(|b: &mut Book| b.unnamed.as_mut().unwrap().slots[4].c1 = 3) }),
            "layout.unnamed",
        ),
    ];
    for (params, expected) in cases {
        let e = layouts.set("jaw-mac", 1, params.clone()).await.unwrap_err();
        assert_eq!(param(e), (code::INVALID_PARAMS, Some(expected.to_string())), "{params}");
    }
    assert_eq!((count(home.path(), "interface_state"), count(home.path(), "quarantine")), (0, 0));
    assert_eq!(layouts.status().refused, 0, "none of them past a limit");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_slot_reads_its_chat_as_jawd_knows_it_now_through_a_merge_run_to_its_end_never_a_chat_being_merged_away() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    // Rui's LID chat first, so it has the lower n, then his number's chat, the group and Ana's,
    // known by her number and her LID.
    let changes = vec![
        Change::Chat(Chat { id: RUI_LID.into(), kind: Some("direct"), ..Chat::default() }),
        Change::Chat(Chat { id: RUI.into(), kind: Some("direct"), ..Chat::default() }),
        Change::Chat(Chat { id: CLIMBING.into(), ..Chat::default() }),
        Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() }),
        Change::Chat(Chat { id: ANA.into(), kind: Some("direct"), ..Chat::default() }),
    ];
    store.apply(Lane::Live, changes).await.unwrap();
    let layouts = Layouts::new(store.clone());
    // The unnamed layout: Rui's LID over the group and his number; Work: Ana by her LID beside a
    // chat jaw.db doesn't have.
    let three = grid(
        &[0.0, 0.5, 1.0],
        &[0.0, 0.5, 1.0],
        vec![
            slot(1, [0, 2, 0, 1], Some(RUI_LID)),
            slot(2, [0, 1, 1, 2], Some(CLIMBING)),
            slot(3, [1, 2, 1, 2], Some(RUI)),
        ],
    );
    let work = grid(
        &[0.0, 0.5, 1.0],
        &[0.0, 1.0],
        vec![slot(1, [0, 1, 0, 1], Some(ANA_LID)), slot(2, [1, 2, 0, 1], Some(GONE))],
    );
    let book = Book { layouts: vec![named("Work", work, None, Some(2))], unnamed_focus: Some(3), ..unnamed(three) };
    let saved = layouts.set("jaw-mac", 1, asked(1, &book)).await.unwrap();
    let chats = |saved: &Saved<Formatted>| -> Vec<Vec<Option<String>>> {
        the_book(&saved.layout).grids().map(|g| g.slots.iter().map(|s| s.chat.clone()).collect()).collect()
    };
    let some = |jid: &str| Some(jid.to_string());
    assert_eq!(chats(&saved), [vec![some(RUI_LID), some(CLIMBING), some(RUI)], vec![some(ANA), None]]);
    assert_eq!(saved.dropped, [GONE], "Ana by her LID is her chat, and a chat jaw.db doesn't have is emptied");
    store
        .apply(Lane::Live, vec![Change::Person(Person { jids: vec![RUI.into(), RUI_LID.into()], ..Person::default() })])
        .await
        .unwrap();
    assert_eq!(count(home.path(), "chat_merges"), 1, "the merge has begun");
    let during = layouts.get("jaw-mac", Value::Null).await.unwrap();
    assert_eq!(chats(&during)[0], [some(RUI), some(CLIMBING), None], "during the merge: {during:?}");
    assert_eq!(during.dropped, [RUI, GONE], "the slot that showed Rui again, as it named him, is emptied");
    assert_eq!(the_book(&during.layout).unnamed_focus, Some(3), "the focus stays on its slot, empty now");
    // A slot naming Rui by a device JID while his LID chat, the older, is being merged away.
    let device = Book { unnamed_focus: None, ..unnamed(single(Some("351913333333:7@s.whatsapp.net"))) };
    let by_device = layouts.set("jaw", 1, asked(1, &device)).await.unwrap();
    assert_eq!(chats(&by_device), [vec![some(RUI)]], "never the chat being merged away: {by_device:?}");
    merges_run(&store, home.path()).await;
    assert!(!exists(home.path(), RUI_LID), "the merged-away chat's row is gone");
    for interface in ["jaw", "jaw-mac"] {
        let after = layouts.get(interface, Value::Null).await.unwrap();
        for jid in chats(&after).concat().into_iter().flatten() {
            assert!(exists(home.path(), &jid), "{interface}'s slot on {jid}, which has no row: {after:?}");
        }
    }
    let after = layouts.get("jaw-mac", Value::Null).await.unwrap();
    assert_eq!(chats(&after)[0], [some(RUI), some(CLIMBING), None]);
    let kept_as_written: Book = serde_json::from_str(&raw(home.path(), "jaw-mac").unwrap()).unwrap();
    assert_eq!(kept_as_written, book, "jaw.db keeps the book as it was written");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_book_jawd_couldnt_write_is_answered_with_an_error_and_kept_in_quarantine_whole() {
    let home = tempfile::tempdir().unwrap();
    let (_store, layouts) = opened(home.path(), &[ANA, CLIMBING]).await;
    let outside = Connection::open(home.path().join("jaw.db")).unwrap();
    outside
        .execute_batch(
            "CREATE TRIGGER refuse BEFORE INSERT ON interface_state BEGIN SELECT RAISE(ABORT, 'no room'); END",
        )
        .unwrap();
    let mut big = unnamed(row_of(12));
    big.unnamed.as_mut().unwrap().slots[0].chat = Some(format!("{}@s.whatsapp.net", "9".repeat(30_000)));
    let e = layouts.set("jaw-mac", 1, asked(5, &big)).await.unwrap_err();
    assert_eq!(e.code, code::INTERNAL, "{}", e.message);
    let quarantined: Vec<u8> = outside.query_row("SELECT raw FROM quarantine", [], |r| r.get(0)).unwrap();
    assert_eq!(quarantined, serde_json::to_vec(&big).unwrap(), "the whole book, past what a tree may take");
    assert!(quarantined.len() > 16 * 1024);
    outside.execute_batch("DROP TRIGGER refuse").unwrap();
    assert_eq!(layouts.set("jaw-mac", 1, asked(5, &big)).await.unwrap().kept, Some(true), "asked again, it's kept");
}
