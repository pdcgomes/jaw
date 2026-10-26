//! Split panes on the made-up account: the windows of the panes that don't have the keys, kept
//! beside the open chat's and taking what comes into their chats as it does, until a pane closes;
//! and the panes' layout, kept as jawd keeps it for jaw, only a newer version replacing it.

use jaw_proto::layout::{Layout, Saved};

use super::Account;
use crate::data::Update;

impl Account {
    pub(super) fn on_park(&mut self) {
        self.parked.extend(self.open.take());
    }

    pub(super) fn on_unpark(&mut self, chat: &str) {
        if let Some(at) = self.parked.iter().position(|o| o.chat == chat) {
            self.on_park();
            self.open = Some(self.parked.remove(at));
        }
    }

    pub(super) fn on_unwatch_parked(&mut self, chat: &str) {
        self.parked.retain(|o| o.chat != chat);
    }

    /// Whether a window, the open one or a pane's, shows `chat`'s message at `i`.
    pub(super) fn shows(&self, chat: &str, i: usize) -> bool {
        self.open.iter().chain(&self.parked).any(|o| o.chat == chat && (o.lo..o.hi).contains(&i))
    }

    pub(super) fn on_ask_layout(&mut self) {
        self.asks += 1;
        let saved = self.saved(None);
        self.send(Update::Layout(Box::new(saved)));
    }

    pub(super) fn on_keep_layout(&mut self, layout: Layout, v: u64) {
        self.keeps += 1;
        if self.layout.as_ref().is_none_or(|(_, kept)| v > *kept) {
            self.layout = Some((layout.clone(), v));
        }
        let kept = self.layout.as_ref().is_some_and(|(l, k)| *k == v && *l == layout);
        let saved = self.saved(Some(kept));
        self.send(Update::LayoutKept(Box::new(saved)));
    }

    fn saved(&self, kept: Option<bool>) -> Saved {
        let (layout, v) = match &self.layout {
            Some((layout, v)) => (Some(layout.clone()), *v),
            None => (None, 0),
        };
        Saved { interface: "jaw".into(), v, layout, kept, dropped: vec![] }
    }
}

#[cfg(test)]
impl super::Demo {
    /// The chats whose windows are watched now: the open one's, then the panes'.
    pub fn windows_watched(&self) -> Vec<String> {
        let a = self.lock();
        a.open.iter().chain(&a.parked).map(|o| o.chat.clone()).collect()
    }

    /// The layout kept for jaw, and how often jaw has asked for it and asked to keep one.
    pub fn layout_kept(&self) -> (Option<(Layout, u64)>, u32, u32) {
        let a = self.lock();
        (a.layout.clone(), a.asks, a.keeps)
    }

    /// Keeps `layout` for jaw, as a jaw before this one did.
    pub fn keeps_layout(&self, layout: Layout, v: u64) {
        self.lock().layout = Some((layout, v));
    }
}
