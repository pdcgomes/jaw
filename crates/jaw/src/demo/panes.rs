//! Split panes on the made-up account: the windows of the panes that don't have the keys, kept
//! beside the open chat's and taking what comes into their chats as it does, until a pane closes.

use super::Account;

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
}

#[cfg(test)]
impl super::Demo {
    /// The chats whose windows are watched now: the open one's, then the panes'.
    pub fn windows_watched(&self) -> Vec<String> {
        let a = self.lock();
        a.open.iter().chain(&a.parked).map(|o| o.chat.clone()).collect()
    }
}
