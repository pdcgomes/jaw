//! Saving a message's file: s, or the palette's "Save this message's file", asks where in the
//! palette, from `~/Downloads/`, where the Mac's Save puts a file: what's typed is a folder to save
//! it into or a file's path, whole or from `~/`, Tab completing a folder as the attach picker does.
//! jawd copies the file there with `media.save`, downloading it first if it isn't here yet, and
//! answers once the copy is whole, on a task of its own, so other requests don't wait for it. Where
//! it went is said as the Mac says it, "saved to Downloads as …", and why not in jawd's words.

use super::View;
use super::attach::{entries, expand, shown};
use super::palette::{Item, Mode, Narrow, Palette};
use crate::data::{Kind, Message};
use crate::fmt;
use std::path::Path;

/// Where the prompt starts: Downloads, as the Mac's Save puts a file there.
pub const DOWNLOADS: &str = "~/Downloads/";

/// Whether a message has a file of its own to save.
pub fn has_file(m: &Message) -> bool {
    !m.deleted
        && matches!(
            m.kind,
            Kind::Image
                | Kind::Video
                | Kind::Gif
                | Kind::VideoNote
                | Kind::Voice
                | Kind::Audio
                | Kind::Document
                | Kind::Sticker
        )
}

/// Where a file went, as the Mac says it: "saved to Downloads as lake.jpg", any other folder from
/// `~` where it's in your home folder.
pub fn said_saved(path: &str) -> String {
    let path = Path::new(path);
    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let folder = path.parent().unwrap_or(Path::new("/"));
    let folder = if folder == expand("~/Downloads") { "Downloads".to_string() } else { shown(folder) };
    format!("saved to {} as {name}", folder.trim_end_matches('/'))
}

impl View {
    /// s on a selected message, or the palette's "Save this message's file": the palette asking
    /// where, from Downloads.
    pub(super) fn palette_save(&mut self, m: &Message) {
        if !has_file(m) {
            self.say("there's no file to save in this one");
            return;
        }
        let mut p = Palette::new();
        p.narrow = Narrow::Actions;
        p.answered = true;
        p.query.set(DOWNLOADS);
        p.mode = Mode::Save { message: m.id.clone() };
        self.palette = Some(p);
    }

    /// The prompt's row: what's saved.
    pub(super) fn save_items(&self, message: &str) -> Vec<Item> {
        let what = self.messages.as_ref().and_then(|w| w.get(message)).map(super::actions::one_line);
        vec![Item::Heading("save", fmt::fit(&what.unwrap_or_default(), 60))]
    }

    /// Tab: what's typed completed to the first folder its last part starts.
    pub(super) fn save_tab(&mut self) {
        let Some(p) = self.palette.as_mut() else { return };
        let (folder, found) = entries(p.query.text());
        if let Some((name, _)) = found.into_iter().find(|(_, dir)| *dir) {
            p.query.set(&format!("{}/", shown(&folder.join(name)).trim_end_matches('/')));
        }
    }

    /// Enter: the file asked of jawd to where it's typed, a whole path or one from `~/`.
    pub(super) fn save_go(&mut self) {
        let Some(Palette { mode: Mode::Save { message }, query, .. }) = self.palette.as_ref() else { return };
        let (message, to) = (message.clone(), query.text().trim().to_string());
        if !(to.starts_with('/') || to.starts_with("~/")) {
            self.say("a path from / or ~/");
            return;
        }
        self.palette = None;
        let (Some(chat), Some(m)) = (self.open.clone(), self.messages.as_ref().and_then(|w| w.get(&message))) else {
            return;
        };
        let here = m.media.as_ref().is_some_and(|media| media.path.is_some());
        self.jawd.save(&chat, &message, &to);
        self.say(if here { format!("saving to {to}") } else { "it downloads first; it's saved once it's here".into() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_a_file_went_is_said_from_downloads_or_home_as_the_mac_says_it() {
        let home = expand("~/");
        let at = |rest: &str| home.join(rest).display().to_string();
        assert_eq!(said_saved(&at("Downloads/lake.jpg")), "saved to Downloads as lake.jpg");
        assert_eq!(said_saved(&at("Documents/recibo.pdf")), "saved to ~/Documents as recibo.pdf");
        assert_eq!(said_saved("/Volumes/Fotos/lake.jpg"), "saved to /Volumes/Fotos as lake.jpg");
    }
}
