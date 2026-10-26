//! Experimental switches: a spike's changes to the screen, each off unless turned on, so jaw looks
//! and behaves as it always has until one is. They're settled once as jaw starts, from
//! `JAW_EXPERIMENTS` for now (`JAW_EXPERIMENTS=composer,status_line,focus,quick_look,panes`);
//! whatever holds them later takes the place of `from_env` alone.

use std::sync::OnceLock;

/// What `start` settled; a view made before it, as the tests' are, has every switch off.
static AT_START: OnceLock<Experiments> = OnceLock::new();

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Experiments {
    /// The message box along the whole bottom of the screen, under the sidebar and the chat list
    /// too, naming the chat it sends to.
    pub composer: bool,
    /// A line above the box, always on screen: the chat you're in and how it is, and what needs
    /// you in the others.
    pub status_line: bool,
    /// A key folds the sidebar and the chat list away for one chat, the status line the only
    /// glimpse of the rest.
    pub focus: bool,
    /// A short catch-up of the chats that need you, over the one you're in.
    pub quick_look: bool,
    /// Split panes, as tmux has them: the conversation split across or down, a chat in each.
    pub panes: bool,
}

impl Experiments {
    /// The switches named in `list`, separated by commas; `all` turns on every one, and names jaw
    /// doesn't know turn on nothing.
    pub fn parse(list: &str) -> Experiments {
        let mut on = Experiments::default();
        for name in list.split(',').map(|n| n.trim().to_ascii_lowercase().replace('-', "_")) {
            match name.as_str() {
                "composer" => on.composer = true,
                "status_line" => on.status_line = true,
                "focus" => on.focus = true,
                "quick_look" => on.quick_look = true,
                "panes" => on.panes = true,
                "all" => {
                    on = Experiments { composer: true, status_line: true, focus: true, quick_look: true, panes: true }
                }
                _ => {}
            }
        }
        on
    }

    /// The switches as `JAW_EXPERIMENTS` names them.
    pub fn from_env() -> Experiments {
        Experiments::parse(&std::env::var("JAW_EXPERIMENTS").unwrap_or_default())
    }

    /// Settles the switches for the life of the process, as it starts.
    pub fn start(with: Experiments) {
        let _ = AT_START.set(with);
    }

    /// The switches settled as jaw started, every one off if nothing settled them.
    pub fn at_start() -> Experiments {
        AT_START.get().copied().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_names_the_switches_turned_on_and_nothing_else_is() {
        assert_eq!(Experiments::parse(""), Experiments::default());
        assert_eq!(Experiments::parse("composer"), Experiments { composer: true, ..Experiments::default() });
        let all = Experiments { composer: true, status_line: true, focus: true, quick_look: true, panes: true };
        assert_eq!(Experiments::parse("composer,status_line,focus,quick_look,panes"), all);
        assert_eq!(
            Experiments::parse(" Status-Line , focus ,nope"),
            Experiments { status_line: true, focus: true, ..Experiments::default() }
        );
        assert_eq!(Experiments::parse("all"), all);
    }
}
