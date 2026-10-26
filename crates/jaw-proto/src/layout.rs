//! An interface's layout of split panes, which jawd keeps for each interface by the name it gives
//! in hello, so it opens where it was left and another interface can open it: `layout.get`,
//! `layout.set` and the `layout` push.

use serde::{Deserialize, Serialize};

/// The format layouts are written in; jawd refuses one it doesn't know.
pub const FORMAT: u32 = 1;
/// The most panes a layout can have.
pub const PANES_MOST: usize = 16;
/// The most bytes a layout can take, written as jawd keeps it: compact JSON.
pub const BYTES_MOST: usize = 16 * 1024;
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

/// `layout.get`'s params: whose layout, the connection's own interface's by default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutGet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
}

/// `layout.set`'s params: the connection's interface's layout, and its version, which the
/// interface gives: more than any it has kept or been told of, such as the time in milliseconds.
/// jawd keeps it only if no layout with a newer version is kept, so asking again changes nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutSet {
    pub v: u64,
    pub layout: Layout,
}

/// The layout jawd keeps for an interface, as `layout.get` and `layout.set` answer and the
/// `layout` push says, with each pane's chat as jawd knows it now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub interface: String,
    /// The version kept; 0 when none is.
    pub v: u64,
    /// None when none is kept, when no pane's chat is left, or when it was written in a format
    /// this jawd doesn't know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
    /// `layout.set`'s answer only: whether its layout is the one kept, or a newer one is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept: Option<bool>,
    /// The panes left out, by the chat each named: a chat that's gone, or one another pane shows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped: Vec<String>,
}
