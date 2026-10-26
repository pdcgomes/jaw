//! The focused mode: one chat with the screen to itself, the sidebar and the chat list folded
//! away and the status line the only glimpse of the rest. Ctrl-F folds them and brings them back;
//! Alt-Up and Alt-Down still move between chats, and Ctrl-B brings the rest back with the sidebar.

use super::View;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl View {
    /// Whether one chat has the screen: folded by its key, and only while its switch is on.
    pub(super) fn focused(&self) -> bool {
        self.experiments.focus && self.focus_mode
    }

    pub(super) fn toggle_focus_mode(&mut self) {
        self.focus_mode = !self.focus_mode;
        if self.focus_mode {
            self.sidebar.keys = None;
        }
        self.dirty = true;
    }

    /// The keys the switches turned on: Ctrl-F for the focused mode, Ctrl-G for the quick look,
    /// Ctrl-P the panes' prefix; says whether it took the key. Ctrl-B, focused, brings the rest
    /// back before the sidebar.
    pub(super) fn on_experiment_key(&mut self, k: KeyEvent) -> bool {
        if !k.modifiers.contains(KeyModifiers::CONTROL) {
            return false;
        }
        match k.code {
            KeyCode::Char('f') if self.experiments.focus => {
                self.toggle_focus_mode();
                true
            }
            KeyCode::Char('g') if self.experiments.quick_look && self.library.is_none() => {
                self.open_quick_look();
                true
            }
            KeyCode::Char('p') if self.experiments.panes => {
                self.panes.prefix = super::panes::Prefix::Armed;
                true
            }
            KeyCode::Char('b') if self.focused() => {
                self.focus_mode = false;
                false
            }
            _ => false,
        }
    }

    /// The footer's keys for what the switches turned on, while typing.
    pub(super) fn experiment_hints(&self) -> Vec<(&'static str, &'static str, bool)> {
        let mut keys = vec![];
        if self.experiments.focus {
            keys.push(("ctrl-f", if self.focused() { "all chats" } else { "focus" }, true));
        }
        if self.experiments.quick_look {
            keys.push(("ctrl-g", "look", true));
        }
        if self.experiments.panes {
            keys.push(("ctrl-p", "panes", true));
        }
        keys
    }
}
