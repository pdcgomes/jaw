//! The quick look's windows on the made-up account: each chat's newest few messages, beside the
//! open chat's window, kept current as messages arrive until the interface lets them go.

use super::Account;
use crate::data::{Place, Update};
use jaw_client::{Moved, Update as Window};

impl Account {
    pub(super) fn on_watch_quick_look(&mut self, chats: &[String], newest: u32) {
        let n = newest as usize;
        self.quick_look = chats.iter().filter(|c| self.index.contains_key(*c)).map(|c| (c.clone(), n)).collect();
        for (chat, _) in self.quick_look.clone() {
            let messages = &self.threads[self.index[&chat]].messages;
            let lo = messages.len().saturating_sub(n);
            let update = Window {
                upsert: messages[lo..].to_vec(),
                total: Some(messages.len() as u64),
                older: Some(lo > 0),
                newer: Some(0),
                moved: Some(Moved { place: Place::newest(newest), newer: 0 }),
                ..Window::default()
            };
            self.send(Update::QuickLook { chat, update });
        }
    }

    pub(super) fn on_unwatch_quick_look(&mut self) {
        self.quick_look.clear();
    }

    /// A message has come into chat `t`: a quick look watching it takes it, and lets go of the
    /// one that falls out of its window.
    pub(super) fn quick_look_arrived(&mut self, t: usize) {
        let thread = &self.threads[t];
        let Some(&(_, n)) = self.quick_look.iter().find(|(chat, _)| *chat == thread.chat.id) else { return };
        let Some(newest) = thread.messages.last().cloned() else { return };
        let len = thread.messages.len();
        let remove = (len > n).then(|| thread.messages[len - n - 1].id.clone()).into_iter().collect();
        let update =
            Window { upsert: vec![newest], remove, total: Some(len as u64), older: Some(len > n), ..Window::default() };
        let chat = thread.chat.id.clone();
        self.send(Update::QuickLook { chat, update });
    }
}

#[cfg(test)]
impl super::Demo {
    /// The chats the quick look watches now.
    pub fn quick_look_watched(&self) -> Vec<String> {
        self.lock().quick_look.iter().map(|(chat, _)| chat.clone()).collect()
    }

    /// Someone writes in `chat` as the live account's people do: its row, its unread count and
    /// any window on it take the message.
    pub fn writes(&self, chat: &str, from: (&str, &str, u8), text: &str) {
        let mut a = self.lock();
        let Some(&t) = a.index.get(chat) else { return };
        let sender = crate::data::Sender { id: from.0.into(), name: from.1.into(), colour: from.2 };
        let at = a.now();
        let m = a.message(at, Some(sender), super::markup(text), None);
        a.arrive(t, m);
    }
}
