//! What each interface is looking at: whether it's in front, which terminals report as focus in
//! and out, and which chats it's reading, from its messages windows and the chat it last said it
//! opened. Notifications stay quiet for a chat an interface in front is reading, and presence
//! follows focus.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::watch;

pub struct Attention {
    connections: Mutex<HashMap<u64, Looking>>,
    next: AtomicU64,
    /// Whether any interface is in front, for what follows it as it changes.
    focus: watch::Sender<bool>,
}

#[derive(Default)]
struct Looking {
    focused: bool,
    /// Each messages window's chat, by its watch.
    windows: HashMap<String, String>,
    /// The chat the interface last said it opened.
    opened: Option<String>,
}

impl Looking {
    fn reads(&self, chat: &str) -> bool {
        self.opened.as_deref() == Some(chat) || self.windows.values().any(|watched| watched == chat)
    }
}

impl Default for Attention {
    fn default() -> Self {
        Attention { connections: Mutex::default(), next: AtomicU64::new(1), focus: watch::channel(false).0 }
    }
}

impl Attention {
    /// A connection's view, which it keeps up to date; it's forgotten when the view is dropped.
    pub fn viewer(self: &Arc<Self>) -> Viewer {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.connections.lock().expect("attention").insert(id, Looking::default());
        Viewer { attention: self.clone(), id }
    }

    /// How many interfaces are connected.
    pub fn connected(&self) -> usize {
        self.connections.lock().expect("attention").len()
    }

    /// Whether any interface is in front.
    pub fn focused(&self) -> bool {
        self.connections.lock().expect("attention").values().any(|looking| looking.focused)
    }

    /// Whether any interface is in front, as it changes.
    pub fn focus(&self) -> watch::Receiver<bool> {
        self.focus.subscribe()
    }

    /// Whether an interface in front is reading `chat`, so you're looking at it now.
    pub fn reading(&self, chat: &str) -> bool {
        self.connections.lock().expect("attention").values().any(|looking| looking.focused && looking.reads(chat))
    }

    fn change(&self, id: u64, change: impl FnOnce(&mut Looking)) {
        let mut connections = self.connections.lock().expect("attention");
        if let Some(looking) = connections.get_mut(&id) {
            change(looking);
        }
        self.refocus(&connections);
    }

    fn refocus(&self, connections: &HashMap<u64, Looking>) {
        let any = connections.values().any(|looking| looking.focused);
        self.focus.send_if_modified(|was| std::mem::replace(was, any) != any);
    }
}

/// One connection's part in `Attention`.
pub struct Viewer {
    attention: Arc<Attention>,
    id: u64,
}

impl Viewer {
    pub fn focus(&self, focused: bool) {
        self.attention.change(self.id, |looking| looking.focused = focused);
    }

    /// A messages window on `chat` is watched as `watch`.
    pub fn watched(&self, watch: String, chat: String) {
        self.attention.change(self.id, |looking| {
            looking.windows.insert(watch, chat);
        });
    }

    /// The window `watch` is no longer watched. The chat it was on stops counting as opened once
    /// no window shows it.
    pub fn unwatched(&self, watch: &str) {
        self.attention.change(self.id, |looking| {
            let Some(chat) = looking.windows.remove(watch) else { return };
            if looking.opened.as_ref() == Some(&chat) && !looking.windows.values().any(|watched| *watched == chat) {
                looking.opened = None;
            }
        });
    }

    /// The interface says you opened `chat`, which it reads from now on instead of the last one.
    pub fn opened(&self, chat: String) {
        self.attention.change(self.id, |looking| looking.opened = Some(chat));
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        let mut connections = self.attention.connections.lock().expect("attention");
        connections.remove(&self.id);
        self.attention.refocus(&connections);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANA: &str = "351912345678@s.whatsapp.net";
    const CLIMBING: &str = "120363012345678901@g.us";

    #[test]
    fn a_chat_is_read_only_while_the_interface_showing_it_is_in_front() {
        let attention = Arc::new(Attention::default());
        let terminal = attention.viewer();
        let other = attention.viewer();
        terminal.watched("w2".into(), ANA.into());
        assert!(!attention.reading(ANA), "the terminal isn't in front");
        other.focus(true);
        assert!(attention.focused());
        assert!(!attention.reading(ANA), "the interface in front isn't the one showing it");
        terminal.focus(true);
        assert!(attention.reading(ANA));
        terminal.unwatched("w2");
        assert!(!attention.reading(ANA));
    }

    #[test]
    fn an_opened_chat_is_read_until_another_is_opened_or_its_last_window_goes() {
        let attention = Arc::new(Attention::default());
        let terminal = attention.viewer();
        terminal.focus(true);
        terminal.opened(ANA.into());
        assert!(attention.reading(ANA), "opened before its window is watched");
        terminal.watched("w2".into(), ANA.into());
        terminal.watched("w3".into(), ANA.into());
        terminal.unwatched("w2");
        assert!(attention.reading(ANA), "another window still shows it");
        terminal.unwatched("w3");
        assert!(!attention.reading(ANA));
        terminal.opened(ANA.into());
        terminal.opened(CLIMBING.into());
        assert!(!attention.reading(ANA) && attention.reading(CLIMBING));
    }

    #[test]
    fn a_connection_that_ends_is_forgotten() {
        let attention = Arc::new(Attention::default());
        let terminal = attention.viewer();
        terminal.focus(true);
        terminal.opened(ANA.into());
        assert_eq!((attention.connected(), attention.focused()), (1, true));
        let focus = attention.focus();
        assert!(*focus.borrow());
        drop(terminal);
        assert!(!*focus.borrow(), "focus goes with it");
        assert_eq!((attention.connected(), attention.focused(), attention.reading(ANA)), (0, false, false));
    }
}
