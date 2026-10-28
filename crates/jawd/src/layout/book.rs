//! Books of layouts on a grid (format 2): what jawd checks before it keeps one, and how it reads one
//! for a connection, each slot's chat as jawd knows it now; and how a tree of panes (format 1) reads
//! as a book and a book as a tree, so an interface of either format opens the other's.
//!
//! **A tree as a grid,** as the Mac app's `PaneGrid.from` reads one: the tree is laid out in the
//! unit square, a split across at `first` thousandths putting a column line at that share of its
//! width (`a` on the left) and a split down a row line at that share of its height (`a` on top);
//! the grid's columns and rows are the panes' edges, each once, in order; and each pane is a slot
//! spanning the lines at its edges, numbered by its place in the tree's order from 1, with its
//! chat. A pane whose share of the square rounds to nothing, which only splits nested past any
//! screen's pixels can make, is left out. The book has that grid as its unnamed layout, with the
//! focused pane's slot as its focus, and nothing else.
//!
//! **A grid as a tree:** the book's layout on screen is cut in two along a line no slot crosses,
//! the first such column line from the left, or else the first row line from the top, and each
//! part again, until each part is one slot: each cut a split, across at a column line or down at a
//! row line, its share the thousandths its line falls at, rounded, from 1 to 999. The slots are the
//! panes, in the order the cuts give them, and the focused slot is the pane with the keys, or the
//! first pane if the focus is on no slot. A grid no line cuts, such as a pinwheel, isn't a tree.

use std::collections::HashSet;

use anyhow::Result;
use jaw_proto::layout::{
    BOOK_FORMAT, Book, FORMAT, Grid, How, LAYOUT_NAME_MOST, LAYOUTS_MOST, Layout, Node, SLOTS_MOST, Slot, Split,
};

use super::jid;

/// Why a book can't be kept: past a limit, error 5; or not a book jawd can keep, -32602 with the
/// param that's wrong.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Refusal {
    Past(String),
    Invalid { param: &'static str, why: String },
}

/// Checks a book jawd is asked to keep: nine named layouts at most besides the unnamed one, and 12
/// slots at most in each shape; each name 1 to 64 bytes and the book's only one by it; `current`
/// one of them; and every shape a grid as the app keeps one (`holds`). The slots with the focus
/// are kept as they're given: the app goes to the first slot when its focus is on none.
pub(super) fn checked(book: &Book) -> Result<(), Refusal> {
    if book.layouts.len() >= LAYOUTS_MOST {
        return Err(Refusal::Past(format!(
            "a book holds {LAYOUTS_MOST} layouts at most, its unnamed one and {} named, and this one names {}",
            LAYOUTS_MOST - 1,
            book.layouts.len()
        )));
    }
    if let Some((_, place, grid)) = shapes(book).find(|(_, _, grid)| grid.slots.len() > SLOTS_MOST) {
        return Err(Refusal::Past(format!(
            "a layout has {SLOTS_MOST} slots at most, and {place} has {}",
            grid.slots.len()
        )));
    }
    let mut names = HashSet::new();
    for named in &book.layouts {
        if named.name.is_empty() || named.name.len() > LAYOUT_NAME_MOST {
            let why = format!("a layout's name is 1 to {LAYOUT_NAME_MOST} bytes, and {:?} isn't", named.name);
            return Err(Refusal::Invalid { param: "layout.layouts", why });
        }
        if !names.insert(named.name.as_str()) {
            let why = format!("a book names each of its layouts once, and {:?} is named twice", named.name);
            return Err(Refusal::Invalid { param: "layout.layouts", why });
        }
    }
    if let Some(current) = book.current.as_ref().filter(|current| !names.contains(current.as_str())) {
        let why = format!("the layout on screen is one the book names, and it names none {current:?}");
        return Err(Refusal::Invalid { param: "layout.current", why });
    }
    for (param, place, grid) in shapes(book) {
        holds(grid).map_err(|why| Refusal::Invalid { param, why: format!("{place}: {why}") })?;
    }
    Ok(())
}

/// Each shape the book holds, with the param that names it and how a refusal names it.
fn shapes(book: &Book) -> impl Iterator<Item = (&'static str, String, &Grid)> {
    let unnamed = book.unnamed.iter().map(|grid| ("layout.unnamed", "the unnamed layout".to_string(), grid));
    let named = book.layouts.iter().flat_map(|named| {
        let saved = ("layout.layouts", format!("{:?}", named.name), &named.grid);
        let edited =
            named.edited.iter().map(|grid| ("layout.layouts", format!("{:?}'s edited shape", named.name), grid));
        std::iter::once(saved).chain(edited)
    });
    unnamed.chain(named)
}

/// Whether `grid` is a layout as the app keeps one, or why not: its lines from 0 to 1, each past
/// the one before; each slot spanning lines the grid has, from one to a later one; the slots
/// covering every cell between the lines once; every line inside the grid some slot's edge; each
/// slot's id its own; and each slot's chat a JID, in no other slot.
pub(super) fn holds(grid: &Grid) -> Result<(), String> {
    for (axis, lines) in [("columns", &grid.cols), ("rows", &grid.rows)] {
        let in_order = lines.windows(2).all(|pair| pair[0] < pair[1]);
        if lines.len() < 2 || lines[0] != 0.0 || lines[lines.len() - 1] != 1.0 || !in_order {
            return Err(format!("its {axis}' lines go from 0 to 1, each past the one before"));
        }
        // Each line inside the grid is the edge of a slot, so there are at most two a slot; checked
        // before the cells are counted, so a grid with many lines costs nothing to refuse.
        if lines.len() - 2 > 2 * grid.slots.len() {
            return Err(format!("it has more {axis}' lines than its slots have edges"));
        }
    }
    let (columns, rows) = (grid.cols.len() - 1, grid.rows.len() - 1);
    let mut cells = vec![0usize; columns * rows];
    let (mut ids, mut chats) = (HashSet::new(), HashSet::new());
    for slot in &grid.slots {
        let [c0, c1, r0, r1] = [slot.c0, slot.c1, slot.r0, slot.r1].map(|line| line as usize);
        if !(c0 < c1 && c1 <= columns && r0 < r1 && r1 <= rows) {
            return Err(format!("slot {} spans lines the grid doesn't have, or none", slot.id));
        }
        if !ids.insert(slot.id) {
            return Err(format!("two slots are numbered {}", slot.id));
        }
        if let Some(chat) = &slot.chat {
            if !jid(chat) {
                return Err(format!("a slot shows a chat by its JID, and {chat:?} isn't one"));
            }
            if !chats.insert(chat.as_str()) {
                return Err(format!("a chat can be in one slot, and {chat} is in two"));
            }
        }
        for r in r0..r1 {
            for cell in &mut cells[r * columns + c0..r * columns + c1] {
                *cell += 1;
            }
        }
    }
    if cells.iter().any(|&slots| slots != 1) {
        return Err("its slots cover it with no holes and no overlaps".into());
    }
    let edge = |lines: usize, ends: fn(&Slot) -> [u32; 2]| {
        (1..lines).all(|k| grid.slots.iter().any(|slot| ends(slot).contains(&(k as u32))))
    };
    if !edge(columns, |s| [s.c0, s.c1]) || !edge(rows, |s| [s.r0, s.r1]) {
        return Err("every line inside it is some slot's edge".into());
    }
    Ok(())
}

/// `book` with each slot's chat as `named` knows it now: a slot whose chat it doesn't know, or that
/// shows a chat a slot before it in the same shape shows, in reading order (top to bottom, then
/// left to right), is emptied, keeping its place; and the chats those named, each once, in the
/// order the book has them. The slots keep their order, and the focus its slot.
pub(super) fn resolved(
    mut book: Book,
    mut named: impl FnMut(&str) -> Result<Option<String>>,
) -> Result<(Book, Vec<String>)> {
    let mut dropped: Vec<String> = Vec::new();
    let shapes = book
        .unnamed
        .iter_mut()
        .chain(book.layouts.iter_mut().flat_map(|named| std::iter::once(&mut named.grid).chain(named.edited.as_mut())));
    for grid in shapes {
        let mut shown = HashSet::new();
        for at in reading(grid) {
            let slot = &mut grid.slots[at];
            let Some(jid) = slot.chat.take() else { continue };
            match named(&jid)?.filter(|chat| !shown.contains(chat)) {
                Some(chat) => {
                    shown.insert(chat.clone());
                    slot.chat = Some(chat);
                }
                None if !dropped.contains(&jid) => dropped.push(jid),
                None => {}
            }
        }
    }
    Ok((book, dropped))
}

/// The places of `grid`'s slots in reading order: top to bottom, then left to right, as the app
/// counts them.
fn reading(grid: &Grid) -> Vec<usize> {
    let mut order: Vec<usize> = (0..grid.slots.len()).collect();
    order.sort_by_key(|&at| (grid.slots[at].r0, grid.slots[at].c0));
    order
}

/// `tree`, as a connection reading format 1 is answered with it, read as a book whose unnamed
/// layout is the tree on a grid; and the chats of the panes too small to be on one.
pub(super) fn of_tree(tree: Layout) -> (Book, Vec<String>) {
    let mut panes = Vec::new();
    lay_out(tree.tree, [0.0, 1.0, 0.0, 1.0], &mut panes);
    let mut dropped = Vec::new();
    let mut kept = Vec::new();
    for (at, (chat, [x0, x1, y0, y1])) in panes.into_iter().enumerate() {
        if x0 < x1 && y0 < y1 {
            kept.push((at as u32 + 1, chat, [x0, x1, y0, y1]));
        } else {
            dropped.push(chat);
        }
    }
    let lines = |low: usize, high: usize| {
        let mut lines: Vec<f64> = kept.iter().flat_map(|(_, _, edges)| [edges[low], edges[high]]).collect();
        lines.sort_by(f64::total_cmp);
        lines.dedup();
        lines
    };
    let (cols, rows) = (lines(0, 1), lines(2, 3));
    let line = |lines: &[f64], at: f64| lines.partition_point(|&line| line < at) as u32;
    // The keys stay with their pane, or go to the pane before it, or the first.
    let focused = tree.focused + 1;
    let focus = kept.iter().map(|(id, _, _)| *id).take_while(|&id| id <= focused).last().or(kept.first().map(|k| k.0));
    let slots = kept
        .into_iter()
        .map(|(id, chat, [x0, x1, y0, y1])| Slot {
            id,
            c0: line(&cols, x0),
            c1: line(&cols, x1),
            r0: line(&rows, y0),
            r1: line(&rows, y1),
            chat: Some(chat),
        })
        .collect();
    let grid = Grid { cols, rows, slots };
    (
        Book { format: BOOK_FORMAT, current: None, unnamed: Some(grid), unnamed_focus: focus, layouts: Vec::new() },
        dropped,
    )
}

/// Each pane of `node`, in the tree's order, with its edges in `rect`: its left, right, top and
/// bottom, as fractions of the square.
fn lay_out(node: Node, rect: [f64; 4], panes: &mut Vec<(String, [f64; 4])>) {
    match node {
        Node::Chat(chat) => panes.push((chat, rect)),
        Node::Split(split) => {
            let Split { how, first, a, b } = *split;
            let [x0, x1, y0, y1] = rect;
            let share = f64::from(first) / 1000.0;
            let (rect_a, rect_b) = match how {
                How::Across => {
                    let x = x0 + (x1 - x0) * share;
                    ([x0, x, y0, y1], [x, x1, y0, y1])
                }
                How::Down => {
                    let y = y0 + (y1 - y0) * share;
                    ([x0, x1, y0, y], [x0, x1, y, y1])
                }
            };
            lay_out(a, rect_a, panes);
            lay_out(b, rect_b, panes);
        }
    }
}

/// A book's layout on screen read as a tree, when it can be one, with each slot's chat as the book
/// has it; an empty slot is a pane named "", which no chat is, so reading the tree as format 1
/// leaves it out as it does a pane whose chat is gone. None with nothing on screen, or with a grid
/// no line cuts.
pub(super) fn tree_of(book: &Book) -> Option<Layout> {
    let (grid, focus) = book.on_screen()?;
    holds(grid).ok()?;
    let slots: Vec<&Slot> = grid.slots.iter().collect();
    let mut order = Vec::new();
    let whole = (0, grid.cols.len() as u32 - 1, 0, grid.rows.len() as u32 - 1);
    let tree = cut(grid, &slots, whole, &mut order)?;
    let focused = focus.and_then(|id| order.iter().position(|&slot| slot == id)).unwrap_or(0);
    Some(Layout { format: FORMAT, focused: focused as u32, tree })
}

/// The slots `slots`, which cover the part of `grid` from column line `c0` to `c1` and row line
/// `r0` to `r1`, as a tree, their ids put in `order` in the tree's order; none if no line cuts it.
fn cut(grid: &Grid, slots: &[&Slot], (c0, c1, r0, r1): (u32, u32, u32, u32), order: &mut Vec<u32>) -> Option<Node> {
    if let [slot] = slots {
        order.push(slot.id);
        return Some(Node::Chat(slot.chat.clone().unwrap_or_default()));
    }
    let across = (c0 + 1..c1).find(|&k| slots.iter().all(|s| s.c1 <= k || s.c0 >= k)).map(|k| (How::Across, k));
    let down = || (r0 + 1..r1).find(|&k| slots.iter().all(|s| s.r1 <= k || s.r0 >= k)).map(|k| (How::Down, k));
    let (how, k) = across.or_else(down)?;
    let (before, after): (Vec<&Slot>, Vec<&Slot>) =
        slots.iter().copied().partition(|s| if how == How::Across { s.c1 <= k } else { s.r1 <= k });
    let (first, part_a, part_b) = match how {
        How::Across => (share(&grid.cols, c0, k, c1), (c0, k, r0, r1), (k, c1, r0, r1)),
        How::Down => (share(&grid.rows, r0, k, r1), (c0, c1, r0, k), (c0, c1, k, r1)),
    };
    let a = cut(grid, &before, part_a, order)?;
    let b = cut(grid, &after, part_b, order)?;
    Some(Node::Split(Box::new(Split { how, first, a, b })))
}

/// The thousandths line `k` falls at between lines `low` and `high`, from 1 to 999.
fn share(lines: &[f64], low: u32, k: u32, high: u32) -> u16 {
    let [low, k, high] = [low, k, high].map(|at| lines[at as usize]);
    ((k - low) / (high - low) * 1000.0).round().clamp(1.0, 999.0) as u16
}

#[cfg(test)]
mod tests;
