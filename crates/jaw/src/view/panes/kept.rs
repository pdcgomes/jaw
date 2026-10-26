//! The panes' layout jawd keeps for jaw, so jaw opens with the panes it was left with: asked for
//! once at start, and restored if it's a split while the conversation is still one pane; kept
//! again once it has stayed as it is for KEEP_AFTER, so moving a divider or hopping between panes
//! is kept once. Going from chat to chat in one pane keeps nothing, and going back to one pane
//! keeps that pane, so the split doesn't open again. Each layout kept has a version newer than any
//! jawd has said it keeps; what another jaw keeps only moves the version on, so two jaws never take
//! turns keeping each other's.

use jaw_proto::layout as saved;

use super::{MOST, Node, Panes, Parked, SHARE, Split};
use crate::view::View;

/// How long the layout stays as it is before it's kept, in milliseconds.
pub const KEEP_AFTER: i64 = 2_000;

/// The layout jawd keeps for jaw, as far as jaw knows, and the next to keep.
#[derive(Default)]
pub struct LayoutKept {
    /// Whether it's been asked for, and whether jawd has answered.
    asked: bool,
    answered: bool,
    /// The layout restored, or last asked to keep.
    kept: Option<saved::Layout>,
    /// The newest version jawd has said it keeps, or jaw asked it to; the next is newer.
    v: u64,
    /// A layout to keep, and when, unless it changes first.
    pending: Option<saved::Layout>,
    due: Option<i64>,
}

impl View {
    /// Asks jawd for the layout it keeps for jaw, once, with panes on.
    pub(in crate::view) fn ask_layout(&mut self) {
        if self.experiments.panes && !self.layout_kept.asked {
            self.layout_kept.asked = true;
            self.jawd.ask_layout();
        }
    }

    /// The layout jawd keeps for jaw, as asked for at start: restored if it's a split and the
    /// conversation is still one pane, and what's kept either way.
    pub(in crate::view) fn layout_came(&mut self, saved: saved::Saved) {
        self.layout_kept.v = self.layout_kept.v.max(saved.v);
        if !self.experiments.panes || self.layout_kept.answered {
            return;
        }
        self.layout_kept.answered = true;
        if let Some(layout) = saved.layout.as_ref().filter(|_| !self.split()) {
            self.restore(layout);
        }
        self.layout_kept.kept = saved.layout;
    }

    /// What jawd keeps for jaw now, as it answered a layout kept or another jaw kept one: the
    /// version the next must pass.
    pub(in crate::view) fn layout_kept_now(&mut self, saved: saved::Saved) {
        self.layout_kept.v = self.layout_kept.v.max(saved.v);
    }

    /// Keeps the layout once it has stayed as it is for KEEP_AFTER since it last changed.
    pub(in crate::view) fn keep_layout_when_due(&mut self, now: i64) {
        if !self.experiments.panes || !self.layout_kept.answered {
            return;
        }
        let layout = self.layout_to_keep();
        if layout.is_none() || layout == self.layout_kept.kept {
            self.layout_kept.pending = None;
            self.layout_kept.due = None;
        } else if layout != self.layout_kept.pending {
            self.layout_kept.pending = layout;
            self.layout_kept.due = Some(now + KEEP_AFTER);
        } else if self.layout_kept.due.is_some_and(|due| due <= now)
            && let Some(layout) = self.layout_kept.pending.take()
        {
            let v = (now.max(0) as u64).max(self.layout_kept.v + 1);
            self.layout_kept.v = v;
            self.layout_kept.due = None;
            self.jawd.keep_layout(&layout, v);
            self.layout_kept.kept = Some(layout);
        }
    }

    /// When the layout's next due to be kept.
    pub(in crate::view) fn layout_due(&self) -> Option<i64> {
        self.layout_kept.due
    }

    /// The layout to keep: the panes, while the conversation is split; once it's one pane again,
    /// that pane, if what's kept is a split; otherwise none.
    fn layout_to_keep(&self) -> Option<saved::Layout> {
        match &self.panes.tree {
            Some(tree) if self.split() => {
                let focused = self.order().iter().position(|p| *p == self.panes.focused)?;
                Some(saved::Layout { format: saved::FORMAT, focused: focused as u32, tree: self.saved_node(tree)? })
            }
            _ => {
                let split = self.layout_kept.kept.as_ref().is_some_and(|kept| kept.chats().len() > 1);
                let chat = self.open.clone().filter(|_| split)?;
                Some(saved::Layout { format: saved::FORMAT, focused: 0, tree: saved::Node::Chat(chat) })
            }
        }
    }

    /// `node` with each pane's chat and each share in thousandths; none if a pane shows none.
    fn saved_node(&self, node: &Node) -> Option<saved::Node> {
        Some(match node {
            Node::Pane(p) if *p == self.panes.focused => saved::Node::Chat(self.open.clone()?),
            Node::Pane(p) => saved::Node::Chat(self.panes.parked.get(*p)?.open.clone()?),
            Node::Split { how, first, a, b } => saved::Node::Split(Box::new(saved::Split {
                how: match how {
                    Split::Across => saved::How::Across,
                    Split::Down => saved::How::Down,
                },
                first: first * 10,
                a: self.saved_node(a)?,
                b: self.saved_node(b)?,
            })),
        })
    }

    /// Opens the panes of `layout`, a split of two to MOST panes: its first chat with the keys,
    /// each next one in a pane of its own as splitting opens one, then the keys where they were.
    fn restore(&mut self, layout: &saved::Layout) {
        let chats: Vec<String> = layout.chats().into_iter().map(str::to_string).collect();
        if !(2..=MOST).contains(&chats.len()) {
            return;
        }
        let tree = restored(&layout.tree, &mut 0);
        self.open_chat(&chats[0]);
        self.panes = Panes { parked: vec![Parked::default()], tree: Some(tree), ..Panes::default() };
        for (p, chat) in chats.iter().enumerate().skip(1) {
            self.panes.parked.push(Parked::default());
            self.park();
            self.panes.focused = p;
            self.open_chat(chat);
        }
        self.focus_pane(layout.focused as usize);
    }
}

/// A kept layout's tree as jaw lays it: its panes numbered in its order from `next`, each share
/// in percent within what jaw allows.
fn restored(node: &saved::Node, next: &mut usize) -> Node {
    match node {
        saved::Node::Chat(_) => {
            *next += 1;
            Node::Pane(*next - 1)
        }
        saved::Node::Split(split) => Node::Split {
            how: match split.how {
                saved::How::Across => Split::Across,
                saved::How::Down => Split::Down,
            },
            first: ((split.first as i16 + 5) / 10).clamp(SHARE.0, SHARE.1) as u16,
            a: Box::new(restored(&split.a, next)),
            b: Box::new(restored(&split.b, next)),
        },
    }
}
