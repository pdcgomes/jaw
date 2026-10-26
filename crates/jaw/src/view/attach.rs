//! Attaching files: Ctrl-O, or the palette's "Attach files", opens the palette on a folder, where
//! what's typed is a path, completed with Tab, and the folder's files and folders are listed as
//! it's typed; Tab also picks a file, or several, and Enter attaches them, or the one selected.
//! Paths a terminal pastes, as dropping files on it does, are attached as they come. The box then
//! holds them, and what's typed in it is the caption: Enter sends them, Tab switches between SD and
//! HD, Ctrl-O picks more, and Esc drops them. A file of yours on its way shows how far it has got
//! under it, as jawd's `job` pushes say, and x stops it.

use std::path::{Path, PathBuf};

use super::View;
use super::palette::{Action, Does, Item, Mode, Narrow, Palette};
use crate::data::{Job, JobKind, Kind, Message, Tick};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

/// The most files one send takes, as jawd does.
pub const FILES_MOST: usize = 30;
/// The most of a folder the picker lists.
const LISTED: usize = 200;

/// Files attached to the box, to send with what's typed as their caption.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attaching {
    pub files: Vec<PathBuf>,
    pub hd: bool,
}

/// What's in the folder a path names, matching the name being typed after its last `/`: folders
/// first, then files, each by name, hidden ones only once a `.` is typed.
pub fn entries(typed: &str) -> (PathBuf, Vec<(String, bool)>) {
    let path = expand(typed);
    let (folder, part) = match typed.rfind('/') {
        Some(at) => (expand(&typed[..=at]), typed[at + 1..].to_lowercase()),
        None => (path.clone(), String::new()),
    };
    let Ok(read) = std::fs::read_dir(&folder) else { return (folder, vec![]) };
    let mut found: Vec<(String, bool)> = read
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let hidden = name.starts_with('.') && !part.starts_with('.');
            let lower = name.to_lowercase();
            let matches =
                lower.starts_with(&part) || (part.len() > 1 && !part.starts_with('.') && lower.contains(&part));
            (!hidden && matches).then(|| (name, e.path().is_dir()))
        })
        .collect();
    let starts = |name: &str| name.to_lowercase().starts_with(&part);
    found.sort_by_key(|(name, dir)| (!dir, !starts(name), name.to_lowercase()));
    found.truncate(LISTED);
    (folder, found)
}

/// A path as typed, with `~/` your home folder.
pub fn expand(typed: &str) -> PathBuf {
    match typed.strip_prefix("~/").or((typed == "~").then_some("")) {
        Some(rest) => {
            std::env::var_os("HOME").map_or_else(|| PathBuf::from(typed), |home| PathBuf::from(home).join(rest))
        }
        None => PathBuf::from(typed),
    }
}

/// A path as the picker shows it, your home folder as `~`.
pub fn shown(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~/".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// The files a terminal pasted, as dropping them on it does: whole paths, `file://` URLs, each
/// quoted or with its spaces escaped, on a line each or side by side. None if anything pasted
/// isn't a file that's there, so text that only looks like a path is typed as text.
pub fn dropped(pasted: &str) -> Option<Vec<PathBuf>> {
    let mut paths = vec![];
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut chars = pasted.trim().chars().peekable();
    let end = |word: &mut String, paths: &mut Vec<PathBuf>| {
        if !word.is_empty() {
            let w = std::mem::take(word);
            let w = w.strip_prefix("file://").map(percent_decoded).unwrap_or(w);
            paths.push(expand(&w));
        }
    };
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '\'' | '"') => quote = Some(c),
            (None, '\\') => word.extend(chars.next()),
            (None, c) if c.is_whitespace() => end(&mut word, &mut paths),
            (_, c) => word.push(c),
        }
    }
    end(&mut word, &mut paths);
    let all_files = !paths.is_empty() && paths.iter().all(|p| p.is_absolute() && p.is_file());
    all_files.then_some(paths)
}

/// A `file://` URL's path with its `%20`s and the rest decoded.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (bytes[i], bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex)) {
            (b'%', Some(hi), Some(lo)) => {
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            (b, ..) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl View {
    /// Ctrl-O, or the palette's "Attach files": the picker, on the folder files were last picked
    /// from, or your home folder.
    pub(super) fn attach_open(&mut self) {
        if self.open.is_none() {
            self.say("open a chat to attach files to it");
            return;
        }
        let mut p = Palette::new();
        p.narrow = Narrow::Actions;
        p.answered = true;
        let from = self.attach_from.clone().unwrap_or_else(|| "~/".into());
        p.query.set(&from);
        p.mode = Mode::Attach { picked: vec![] };
        self.palette = Some(p);
    }

    /// The picker's rows: where it is and what's picked, then the folder's folders and files.
    pub(super) fn attach_items(&self, typed: &str, picked: &[PathBuf]) -> Vec<Item> {
        let (folder, found) = entries(typed);
        let said = match picked.len() {
            0 => shown(&folder),
            n => format!("{n} picked · {}", shown(&folder)),
        };
        let mut items = vec![Item::Heading("attach", crate::fmt::fit(&said, 60))];
        if found.is_empty() {
            items.push(Item::Heading("", "nothing here".into()));
        }
        for (name, dir) in found {
            let path = folder.join(&name);
            let mut a = if dir {
                Action::new(format!("{name}/"), "", Does::Open(path))
            } else {
                let size = std::fs::metadata(&path).map(|m| crate::fmt::bytes(m.len())).unwrap_or_default();
                Action::new(name, size, Does::Pick(path.clone()))
            };
            a.lead = Some(match &a.does {
                Does::Pick(path) if picked.contains(path) => "[x]".into(),
                Does::Pick(_) => "[ ]".into(),
                _ => " ▸ ".into(),
            });
            items.push(Item::Action(a));
        }
        items
    }

    /// Tab in the picker: a folder completed into the path, or a file picked or let go.
    pub(super) fn attach_tab(&mut self) {
        let Some(Item::Action(a)) = self.palette_selected() else { return };
        let Some(p) = self.palette.as_mut() else { return };
        match a.does {
            Does::Open(folder) => {
                p.query.set(&format!("{}/", shown(&folder).trim_end_matches('/')));
                p.selected = 0;
            }
            Does::Pick(file) => {
                let Mode::Attach { picked } = &mut p.mode else { return };
                match picked.iter().position(|f| *f == file) {
                    Some(at) => {
                        picked.remove(at);
                    }
                    None if picked.len() >= FILES_MOST => self.say(format!("{FILES_MOST} files at most at once")),
                    None => picked.push(file),
                }
            }
            _ => {}
        }
    }

    /// Enter in the picker: a folder opened, or the files picked attached, or the one selected.
    pub(super) fn attach_act(&mut self) {
        let selected = self.palette_selected();
        let Some(p) = self.palette.as_mut() else { return };
        let Mode::Attach { picked } = &p.mode else { return };
        match selected {
            Some(Item::Action(Action { does: Does::Open(folder), .. })) => {
                p.query.set(&format!("{}/", shown(&folder).trim_end_matches('/')));
                p.selected = 0;
            }
            Some(Item::Action(Action { does: Does::Pick(file), .. })) => {
                let files = if picked.is_empty() { vec![file] } else { picked.clone() };
                let typed = p.query.text().to_string();
                self.attach_from = Some(match typed.rfind('/') {
                    Some(at) => typed[..=at].to_string(),
                    None => typed,
                });
                self.palette = None;
                self.attach(files);
            }
            _ if !picked.is_empty() => {
                let files = picked.clone();
                self.palette = None;
                self.attach(files);
            }
            _ => {}
        }
    }

    /// Files into the box, beside any already there.
    pub(super) fn attach(&mut self, files: Vec<PathBuf>) {
        let a = self.attaching.get_or_insert_with(Attaching::default);
        for file in files {
            if !a.files.contains(&file) && a.files.len() < FILES_MOST {
                a.files.push(file);
            }
        }
        self.focus = super::Focus::Typing;
    }

    /// Keys while the box holds files; what isn't one of these edits the caption.
    pub(super) fn on_attaching_key(&mut self, k: KeyEvent, ctrl: bool, alt: bool) -> bool {
        match k.code {
            KeyCode::Enter if !alt && !k.modifiers.contains(ratatui::crossterm::event::KeyModifiers::SHIFT) => {
                self.send_attached();
                true
            }
            KeyCode::Esc => {
                self.attaching = None;
                true
            }
            KeyCode::Tab => {
                if let Some(a) = self.attaching.as_mut() {
                    a.hd = !a.hd;
                }
                true
            }
            KeyCode::Char('o') if ctrl => {
                self.attach_open();
                true
            }
            _ => false,
        }
    }

    /// Enter with files in the box: they go, a message each, with what's typed as the first's
    /// caption, as a reply if the box was replying.
    fn send_attached(&mut self) {
        let (Some(chat), Some(a)) = (self.open.clone(), self.attaching.take()) else { return };
        let caption = self.composer.take().trim_end().to_string();
        let files: Vec<String> = a.files.iter().map(|f| f.display().to_string()).collect();
        let reply = self.replying.take().map(|r| r.id);
        let key = self.keys.remove(&chat).unwrap_or_else(crate::data::key);
        self.jawd.send_media(&chat, &files, &caption, a.hd, &key, reply.as_deref());
        self.draft_sent.clear();
        self.draft_due = None;
        if !self.messages.as_ref().is_some_and(|w| w.following()) {
            self.watch(&chat, View::newest());
        }
        self.scroll = super::Scroll::Bottom;
    }

    /// Paths pasted into the box, as dropping files does: attached, rather than typed.
    pub(super) fn attach_dropped(&mut self, pasted: &str) -> bool {
        let Some(files) = dropped(pasted).filter(|_| self.open.is_some()) else { return false };
        self.attach(files);
        true
    }

    /// x on a file of yours still on its way: it stops, wherever it has got to.
    pub(super) fn cancel_upload(&mut self, m: &Message) -> bool {
        if !(m.mine && m.status == Some(Tick::Pending) && m.media.is_some() && m.kind != Kind::Album) {
            return false;
        }
        let Some(chat) = self.open.clone() else { return false };
        self.jawd.media_cancel(&chat, &m.id);
        self.jobs.remove(&m.id);
        self.say("stopped; ⏎ on it sends it again");
        true
    }

    pub(super) fn on_job(&mut self, job: Job) {
        if job.kind == JobKind::Download {
            return;
        }
        if job.kind == JobKind::Upload && job.progress >= 1.0 {
            self.jobs.remove(&job.message);
        } else {
            self.jobs.insert(job.message.clone(), (job.kind, job.progress));
        }
    }

    /// How far a file of yours on its way has got, for the line under it.
    pub(super) fn job_said(&self, m: &Message) -> Option<String> {
        if !(m.mine && m.status == Some(Tick::Pending)) {
            return None;
        }
        let (kind, progress) = self.jobs.get(&m.id)?;
        let what = if *kind == JobKind::Upload { "uploading" } else { "making it ready" };
        Some(format!("{what} {}%", (progress * 100.0).round() as u32))
    }

    /// The footer's keys while the box holds files.
    pub(super) fn attaching_hints(&self) -> Vec<(&'static str, &'static str, bool)> {
        let hd = self.attaching.as_ref().is_some_and(|a| a.hd);
        vec![
            ("⏎", "sends them", true),
            ("tab", if hd { "SD" } else { "HD" }, true),
            ("ctrl-o", "more", true),
            ("esc", "drops them", true),
        ]
    }

    /// What the box says it holds, for its title.
    pub(super) fn attached_title(&self) -> Option<String> {
        let a = self.attaching.as_ref()?;
        let what = match a.files.as_slice() {
            [one] => one.file_name().map_or_else(|| "1 file".into(), |n| n.to_string_lossy().into_owned()),
            more => format!("{} files", more.len()),
        };
        Some(format!(" ▣ {what} · {} ", if a.hd { "HD" } else { "SD" }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_paths_are_read_as_terminals_paste_them_and_text_isnt() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("lake.jpg"), dir.path().join("the swim.mov"));
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        let escaped = format!("{} {}", a.display(), b.display().to_string().replace(' ', "\\ "));
        assert_eq!(dropped(&escaped), Some(vec![a.clone(), b.clone()]));
        let quoted = format!("'{}'\n\"{}\"\n", a.display(), b.display());
        assert_eq!(dropped(&quoted), Some(vec![a.clone(), b.clone()]));
        let url = format!("file://{}", b.display().to_string().replace(' ', "%20"));
        assert_eq!(dropped(&url), Some(vec![b.clone()]));
        assert_eq!(dropped("see you at 8"), None, "text is typed");
        assert_eq!(dropped(&format!("{} and more", a.display())), None, "and so is text with a path in it");
        assert_eq!(dropped(&dir.path().display().to_string()), None, "a folder isn't a file to send");
    }

    #[test]
    fn a_folder_lists_its_folders_first_then_its_files_as_typed() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["b.jpg", "a.mov", ".hidden", "Notes.pdf"] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        std::fs::create_dir(dir.path().join("zoo")).unwrap();
        let root = format!("{}/", dir.path().display());
        let (folder, found) = entries(&root);
        assert_eq!(folder, dir.path());
        let names: Vec<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["zoo", "a.mov", "b.jpg", "Notes.pdf"]);
        let (_, found) = entries(&format!("{root}no"));
        assert_eq!(found, [("Notes.pdf".to_string(), false)], "as typed, whatever its case");
        let (_, found) = entries(&format!("{root}."));
        assert_eq!(found, [(".hidden".to_string(), false)], "hidden ones once a . is typed");
    }
}
