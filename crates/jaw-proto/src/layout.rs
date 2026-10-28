//! An interface's layout, which jawd keeps for each interface by the name it gives in hello, so it
//! opens where it was left and another interface can open it: `layout.get`, `layout.set` and the
//! `layout` push. A layout is in one of two formats: a tree of split panes (format 1, `Layout`, the
//! terminal's), or a book of named layouts on a grid (format 2, `Book`, the Mac app's).

use serde::{Deserialize, Serialize};

/// The format a tree of split panes is written in.
pub const FORMAT: u32 = 1;
/// The format a book of layouts on a grid is written in.
pub const BOOK_FORMAT: u32 = 2;
/// The formats jawd keeps layouts in and answers in; it refuses another.
pub const FORMATS: [u32; 2] = [FORMAT, BOOK_FORMAT];
/// The most panes a tree can have.
pub const PANES_MOST: usize = 16;
/// The most bytes a tree can take, written as jawd keeps it: compact JSON.
pub const BYTES_MOST: usize = 16 * 1024;
/// The most layouts a book holds, its unnamed one among them: the ten ⌃⌘0 to ⌃⌘9 reach, so nine
/// named ones at most.
pub const LAYOUTS_MOST: usize = 10;
/// The most slots a layout on a grid can have, in each of its shapes.
pub const SLOTS_MOST: usize = 12;
/// The longest name a book's layout can have, in bytes.
pub const LAYOUT_NAME_MOST: usize = 64;
/// The most bytes a book can take, written as jawd keeps it: compact JSON. A book at every other
/// limit, each shape edited and each slot's chat a JID of 64 bytes, takes about 40 KiB.
pub const BOOK_BYTES_MOST: usize = 64 * 1024;
/// The most interfaces jawd keeps a layout for.
pub const KEPT_MOST: u64 = 16;
/// The longest name an interface can keep a layout under, in bytes.
pub const NAME_MOST: usize = 64;

/// The panes, each showing one chat, as a tree of splits, and the pane with the keys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layout {
    /// FORMAT.
    pub format: u32,
    /// The pane with the keys, counted from 0 in the tree's order: a split's first side, then its
    /// second.
    pub focused: u32,
    pub tree: Node,
}

/// A pane, by the JID of the chat it shows, or a split of two.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Node {
    Chat(String),
    Split(Box<Split>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Split {
    pub how: How,
    /// The first side's share of the split, in thousandths: 500 is half.
    pub first: u16,
    pub a: Node,
    pub b: Node,
}

/// Which way a split runs: its sides beside each other, the first on the left, or one above the
/// other, the first on top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum How {
    Across,
    Down,
}

impl Layout {
    /// The panes' chats, in the tree's order.
    pub fn chats(&self) -> Vec<&str> {
        fn walk<'a>(node: &'a Node, out: &mut Vec<&'a str>) {
            match node {
                Node::Chat(chat) => out.push(chat),
                Node::Split(split) => {
                    walk(&split.a, out);
                    walk(&split.b, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.tree, &mut out);
        out
    }
}

/// A book of layouts on a grid, as the Mac app's `LayoutBook` keeps them, key for key, with
/// `format` where the app keeps its own version: its named layouts, each with its saved shape, the
/// shape it was edited to if it was, and its slot with the focus; the unnamed layout, what's on
/// screen before any is saved, and its slot with the focus; and which is on screen. jawd refuses a
/// key it doesn't know rather than keep the book without it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Book {
    /// BOOK_FORMAT.
    pub format: u32,
    /// The layout on screen, by its name; none for the unnamed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unnamed: Option<Grid>,
    /// The unnamed layout's slot with the focus, by its id.
    #[serde(default, rename = "unnamedFocus", skip_serializing_if = "Option::is_none")]
    pub unnamed_focus: Option<u32>,
    /// The named layouts, in the order they were made.
    #[serde(default)]
    pub layouts: Vec<Named>,
}

/// One of a book's named layouts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Named {
    pub name: String,
    /// Its saved shape, with the chat in each slot.
    pub grid: Grid,
    /// The shape it was edited to, kept apart from the saved one until it's saved or reverted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited: Option<Grid>,
    /// Its slot with the focus, by its id, in the shape on screen: the edited one if there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<u32>,
}

/// A layout on a grid, as the Mac app's `PaneGrid` keeps it: the grid's lines as fractions of its
/// width (`cols`) and its height (`rows`), from 0 to 1 in order, and slots spanning them by index.
/// The slots cover the grid with no holes and no overlaps, every line inside it is some slot's
/// edge, and a chat shows in one slot at most.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grid {
    pub cols: Vec<f64>,
    pub rows: Vec<f64>,
    pub slots: Vec<Slot>,
}

/// A slot: from line `c0` to `c1` of the grid's columns and from `r0` to `r1` of its rows, with an
/// id that stays through edits, and the chat it shows or none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub id: u32,
    pub c0: u32,
    pub c1: u32,
    pub r0: u32,
    pub r1: u32,
    /// The chat's JID; none for a slot waiting for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
}

impl Book {
    /// Every layout on a grid the book holds: the unnamed one, then each named one's saved shape
    /// and its edited one.
    pub fn grids(&self) -> impl Iterator<Item = &Grid> {
        self.unnamed.iter().chain(self.layouts.iter().flat_map(|n| std::iter::once(&n.grid).chain(&n.edited)))
    }

    /// What's on screen, as the app has it: the layout `current` names, in its edited shape if it
    /// has one, or the unnamed layout; with its slot with the focus.
    pub fn on_screen(&self) -> Option<(&Grid, Option<u32>)> {
        match self.current.as_ref().and_then(|name| self.layouts.iter().find(|n| &n.name == name)) {
            Some(named) => Some((named.edited.as_ref().unwrap_or(&named.grid), named.focus)),
            None => self.unnamed.as_ref().map(|grid| (grid, self.unnamed_focus)),
        }
    }
}

/// `layout.get`'s params: whose layout, the connection's own interface's by default, and the format
/// to answer in, the one it was kept in by default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutGet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<u32>,
}

/// `layout.set`'s params: the connection's interface's layout, a `Layout` or a `Book`, and its
/// version, which the interface gives: more than any it has kept or been told of, such as the time
/// in milliseconds. jawd keeps it only if no layout with a newer version is kept, whatever its
/// format, so asking again changes nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutSet<L = Layout> {
    pub v: u64,
    pub layout: L,
}

/// The layout jawd keeps for an interface, as `layout.get` and `layout.set` answer and the
/// `layout` push says, a `Layout` or a `Book`, with each chat as jawd knows it now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved<L = Layout> {
    pub interface: String,
    /// The version kept; 0 when none is.
    pub v: u64,
    /// None when none is kept, when no pane's chat is left, when it was written in a format this
    /// jawd doesn't know, or when a book's layout on screen, asked for as a tree, isn't one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<L>,
    /// `layout.set`'s answer only: whether its layout is the one kept, or a newer one is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept: Option<bool>,
    /// The panes left out, or the slots emptied, by the chat each named: a chat that's gone, or
    /// one another pane, or slot of the same layout, shows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped: Vec<String>,
}
