//! The palette: Ctrl-K's one line of search for anything, over the screen. Each keystroke asks
//! jawd's `find` again and keeps only the answer to the latest; the interface's own actions are
//! matched here as jawd matches names, and merged in. Results are grouped by kind, best first;
//! Tab narrows to one kind, and Enter does the obvious thing with what's selected.

use super::panes::{PaneDo, Split};
use super::{Overlay, View};
use crate::composer::Composer;
use crate::data::{Chat, Counts, Found, FoundKind, Hit, Mark, MarkKind};
use crate::find;
use crate::fmt;
use jaw_proto::settings::{Messages, SettingsSet};
use jaw_proto::themes;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding};

/// Results of each kind asked for when all kinds show, and when one does.
const LIMIT: usize = 10;
const NARROWED_LIMIT: usize = 50;
/// Results of each kind shown when all kinds show; Tab shows the rest.
const PER_KIND: usize = 4;
/// The palette's widest, and its rows at most.
const WIDTH: u16 = 78;
const HEIGHT: u16 = 26;
/// The kinds in the order they're shown and Tab steps through them.
const ORDER: [FoundKind; 7] = [
    FoundKind::Chat,
    FoundKind::Person,
    FoundKind::Folder,
    FoundKind::Message,
    FoundKind::Link,
    FoundKind::Media,
    FoundKind::File,
];

/// What the palette shows: every kind, one of jawd's, or the interface's own actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Narrow {
    All,
    Kind(FoundKind),
    Actions,
}

/// What an action does.
#[derive(Clone, Debug, PartialEq)]
pub enum Does {
    StartChat,
    GoToUnread,
    SearchMessages,
    SearchHere(String, String),
    Mute(String, String),
    Unmute(String),
    Archive(String, bool),
    Pin(String, bool),
    MarkUnread(String, bool),
    Record,
    Keys,
    Quit,
    /// Not yet: what it says when chosen.
    Later(&'static str),
    /// How long to mute for, once Mute asks.
    MuteFor(String, i64, &'static str),
    /// A chat with a number typed into the palette.
    StartPhone(String),
    /// A key's action on the selected message, from the palette.
    OnMessage(String, char),
    React(String, String),
    /// Deletes a message for you, or for everyone.
    DeleteFor(String, bool),
    /// Asks a new folder's name, at the top or in a folder of yours.
    NewFolder(Option<String>),
    Rename(String),
    /// Asks where to move a folder.
    MoveFolder(String),
    /// Moves a folder into another, or to the top.
    MoveTo(String, Option<String>),
    DeleteFolder(String),
    /// Files the open chat, or the one picked, into folders.
    File,
    /// Ticks a folder while filing.
    Tick(String),
    /// Takes a chat out of a folder.
    Unfile(String, String),
    /// Ctrl-B's sidebar.
    Sidebar,
    /// Ctrl-L's library, across all chats.
    Library,
    /// Ctrl-,'s settings panel; on the theme list; your messages set as asked.
    Settings,
    Theme,
    Messages(Messages),
    /// The experimental quick look, the focused mode, and the split panes' actions.
    QuickLook,
    FocusMode,
    Pane(PaneDo),
}

/// What the palette is for: finding, or one of a message's actions that picks something.
#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Find,
    /// A reaction for the message: the six quick ones, or any emoji by name.
    React {
        message: String,
        summary: String,
    },
    /// For you or for everyone; `everyone` says why not when it can't be.
    Delete {
        message: String,
        summary: String,
        everyone: Option<&'static str>,
    },
    /// The chats to forward the messages to, picked with Tab.
    Forward {
        messages: Vec<String>,
        picked: Vec<(String, String)>,
    },
    /// Filing a chat or a person: the folders ticked, those it was in, and a folder being made to
    /// tick once jawd has made it.
    File {
        item: String,
        name: String,
        ticked: Vec<String>,
        was: Vec<String>,
        pending: Option<String>,
    },
    /// A folder's name, for a new one (in `parent`) or for `folder`; `then` goes back to filing.
    Name {
        folder: Option<String>,
        parent: Option<String>,
        then: Option<Box<Mode>>,
    },
    /// Where to move a folder.
    Move {
        folder: String,
    },
}

/// One of the interface's own actions, matched against what's typed.
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub name: String,
    detail: String,
    /// Its key, so the palette teaches them.
    key: Option<&'static str>,
    /// What it shows before its name, where not ▸: an emoji, for a reaction.
    lead: Option<String>,
    /// The milestone it comes with, while it's not here yet.
    later: Option<&'static str>,
    pub does: Does,
    marks: Vec<(usize, usize)>,
    score: i64,
}

impl Action {
    fn new(name: impl Into<String>, detail: impl Into<String>, does: Does) -> Action {
        Action {
            name: name.into(),
            detail: detail.into(),
            key: None,
            lead: None,
            later: None,
            does,
            marks: vec![],
            score: 0,
        }
    }

    fn key(mut self, key: &'static str) -> Action {
        self.key = Some(key);
        self
    }

    fn later(mut self, when: &'static str) -> Action {
        self.later = Some(when);
        self
    }
}

/// A row of the palette's list: a kind's heading, or something to act on.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Heading(&'static str, String),
    Found(usize),
    Action(Action),
}

impl Item {
    fn selectable(&self) -> bool {
        !matches!(self, Item::Heading(..))
    }
}

#[derive(Debug)]
pub struct Palette {
    pub query: Composer,
    pub narrow: Narrow,
    /// Messages in one chat only: its ID and name.
    pub scope: Option<(String, String)>,
    /// Mute asks how long: the chat and its name.
    pub muting: Option<(String, String)>,
    pub mode: Mode,
    /// The emoji found for what's typed while reacting, found once each time it changes.
    reactions: Vec<Action>,
    /// The newest `find` asked, the only one whose answer is kept.
    pub asked: u64,
    pub answered: bool,
    pub results: Vec<Found>,
    pub counts: Counts,
    /// The strip's counts: every kind's from the last answer for all of them, so narrowing to
    /// one keeps the others to Tab to.
    strip: Counts,
    pub failed: Option<String>,
    /// Which of the selectable rows is selected.
    pub selected: usize,
    top: usize,
}

impl Palette {
    pub fn new() -> Palette {
        Palette {
            query: Composer::default(),
            narrow: Narrow::All,
            scope: None,
            muting: None,
            mode: Mode::Find,
            reactions: vec![],
            asked: 0,
            answered: false,
            results: vec![],
            counts: Counts::new(),
            strip: Counts::new(),
            failed: None,
            selected: 0,
            top: 0,
        }
    }
}

fn kind_name(k: FoundKind) -> &'static str {
    match k {
        FoundKind::Chat => "chats",
        FoundKind::Person => "people",
        FoundKind::Folder => "folders",
        FoundKind::Message => "messages",
        FoundKind::Link => "links",
        FoundKind::Media => "media",
        FoundKind::File => "files",
    }
}

/// What's typed, if it's a phone number: digits with a `+`, spaces, dashes and brackets, enough
/// of them to be one.
fn number(query: &str) -> Option<String> {
    let q = query.trim();
    let digits = q.chars().filter(char::is_ascii_digit).count();
    (digits >= 7 && q.chars().all(|c| c.is_ascii_digit() || " +-()".contains(c))).then(|| q.to_string())
}

/// A count as the strip says it: 1,000 is at least that many.
fn count(n: u32) -> String {
    if n >= 1_000 { "1,000+".into() } else { fmt::num(n as u64) }
}

/// Spans cut to `w` columns, an ellipsis where they were cut.
fn fit_spans(spans: Vec<Span<'static>>, w: usize) -> Vec<Span<'static>> {
    let mut out = vec![];
    let mut used = 0;
    for s in spans {
        let sw = s.width();
        if used + sw <= w {
            used += sw;
            out.push(s);
            continue;
        }
        let room = w.saturating_sub(used);
        if room > 0 {
            out.push(Span::styled(fmt::fit(&s.content, room), s.style));
        }
        break;
    }
    out
}

impl View {
    pub fn palette_open(&mut self) {
        self.palette = Some(Palette::new());
        self.ask();
    }

    /// The palette narrowed to messages, in the open chat or everywhere, with words to look for.
    pub fn palette_search(&mut self, scope: Option<(String, String)>, words: &str) {
        let mut p = Palette::new();
        p.narrow = Narrow::Kind(FoundKind::Message);
        p.scope = scope;
        p.query.set(words);
        self.palette = Some(p);
        self.ask();
    }

    /// The palette filing `item` into folders, ticking those it's in.
    pub fn palette_file(&mut self, item: &str, name: &str, folders: Vec<String>) {
        let mut p = Palette::new();
        p.narrow = Narrow::Actions;
        p.answered = true;
        p.mode =
            Mode::File { item: item.into(), name: name.into(), ticked: folders.clone(), was: folders, pending: None };
        self.palette = Some(p);
    }

    fn palette_name(&mut self, folder: Option<String>, parent: Option<String>, then: Option<Box<Mode>>) {
        let mut p = Palette::new();
        p.narrow = Narrow::Actions;
        p.answered = true;
        if let Some(name) = folder.as_deref().map(|f| self.folder_name(f)) {
            p.query.set(&name);
        }
        p.mode = Mode::Name { folder, parent, then };
        self.palette = Some(p);
    }

    /// Your folders in the tree's order, every one open, with their depths.
    fn your_folders(&self) -> Vec<(String, usize)> {
        let mut rows: Vec<&crate::data::FolderRow> = self.folders.iter().filter(|f| f.yours()).collect();
        rows.sort_by_key(|f| (f.position, f.name.clone()));
        let mut out = vec![];
        let mut stack: Vec<(&crate::data::FolderRow, usize)> =
            rows.iter().copied().filter(|f| f.parent.is_none()).rev().map(|f| (f, 0)).collect();
        while let Some((f, depth)) = stack.pop() {
            out.push((f.id.clone(), depth));
            stack.extend(
                rows.iter().copied().filter(|c| c.parent.as_deref() == Some(&f.id)).rev().map(|c| (c, depth + 1)),
            );
        }
        out
    }

    /// Enter while filing: into the folders ticked, out of those unticked.
    fn file_go(&mut self) {
        let Some(Palette { mode: Mode::File { item, name, ticked, was, .. }, .. }) = self.palette.take() else {
            return;
        };
        let into: Vec<&String> = ticked.iter().filter(|f| !was.contains(f)).collect();
        let out: Vec<&String> = was.iter().filter(|f| !ticked.contains(f)).collect();
        for f in &into {
            self.jawd.folder_add(f, &item);
        }
        for f in &out {
            self.jawd.folder_remove(f, &item);
        }
        let names = |fs: &[&String]| fs.iter().map(|f| self.folder_name(f)).collect::<Vec<_>>().join(", ");
        let said = match (into.is_empty(), out.is_empty()) {
            (true, true) => return,
            (false, true) => format!("{name} filed in {}", names(&into)),
            (true, false) => format!("{name} taken out of {}", names(&out)),
            (false, false) => format!("{name} filed in {} and out of {}", names(&into), names(&out)),
        };
        self.say(said);
    }

    /// Enter while naming a folder.
    fn name_go(&mut self) {
        let Some(Palette { mode: Mode::Name { folder, parent, then }, query, .. }) = self.palette.take() else {
            return;
        };
        let name = query.text().trim().to_string();
        if name.is_empty() {
            return;
        }
        match folder {
            Some(f) => {
                self.jawd.folder_rename(&f, &name);
                self.say(format!("renamed {name}"));
            }
            None => {
                self.jawd.folder_create(&name, parent.as_deref());
                if let Some(mut then) = then {
                    if let Mode::File { pending, .. } = then.as_mut() {
                        *pending = Some(name.clone());
                    }
                    let mut p = Palette::new();
                    p.narrow = Narrow::Actions;
                    p.answered = true;
                    p.mode = *then;
                    self.palette = Some(p);
                } else {
                    self.say(format!("made {name}"));
                }
            }
        }
    }

    /// A folder jawd has just made: ticked if it was made while filing.
    pub fn folder_made(&mut self, row: crate::data::FolderRow) {
        if let Some(Palette { mode: Mode::File { ticked, pending, .. }, .. }) = self.palette.as_mut()
            && pending.as_deref() == Some(row.name.as_str())
        {
            ticked.push(row.id.clone());
            *pending = None;
        }
        if !self.folders.iter().any(|f| f.id == row.id) {
            self.folders.push(row);
        }
    }

    /// The palette picking a reaction for `m`.
    pub fn palette_react(&mut self, m: &crate::data::Message) {
        let mut p = Palette::new();
        p.narrow = Narrow::Actions;
        p.mode = Mode::React { message: m.id.clone(), summary: super::actions::one_line(m) };
        p.answered = true;
        self.palette = Some(p);
    }

    /// The palette asking whom to delete `m` for.
    pub fn palette_delete(&mut self, m: &crate::data::Message) {
        let mut p = Palette::new();
        p.narrow = Narrow::Actions;
        let everyone = self.not_for_everyone(m);
        p.mode = Mode::Delete { message: m.id.clone(), summary: super::actions::one_line(m), everyone };
        p.answered = true;
        self.palette = Some(p);
    }

    /// The palette's chats, to pick where to forward `m`.
    pub fn palette_forward(&mut self, m: &crate::data::Message) {
        if m.deleted {
            self.say("a deleted message can't be forwarded");
            return;
        }
        let mut p = Palette::new();
        p.narrow = Narrow::Kind(FoundKind::Chat);
        p.mode = Mode::Forward { messages: vec![m.id.clone()], picked: vec![] };
        self.palette = Some(p);
        self.ask();
    }

    /// Reactions for what's typed: before anything, the six quick ones on their numbers.
    fn react_items(&self, message: &str, summary: &str, query: &str) -> Vec<Item> {
        let mut items = vec![Item::Heading("react", fmt::fit(summary, 48))];
        let name = |e: &str| emojis::get(e).map_or_else(|| e.to_string(), |x| x.name().to_string());
        if query.trim().is_empty() {
            const KEYS: [&str; 6] = ["1", "2", "3", "4", "5", "6"];
            for (i, e) in super::actions::QUICK.iter().enumerate() {
                let mut a = Action::new(name(e), "", Does::React(message.to_string(), e.to_string())).key(KEYS[i]);
                a.lead = Some(e.to_string());
                items.push(Item::Action(a));
            }
            return items;
        }
        if let Some(p) = &self.palette {
            items.extend(p.reactions.iter().cloned().map(Item::Action));
        }
        items
    }

    /// The emoji whose names match what's typed while reacting, best first.
    fn search_emoji(&mut self) {
        let Some(p) = self.palette.as_mut() else { return };
        let Mode::React { message, .. } = &p.mode else { return };
        let query = p.query.text().to_string();
        let mut found: Vec<Action> = emojis::iter()
            .filter_map(|e| {
                let (score, marks) = find::name(&query, e.name())?;
                let mut a = Action::new(e.name(), "", Does::React(message.clone(), e.as_str().to_string()));
                a.lead = Some(e.as_str().to_string());
                a.score = score;
                a.marks = marks;
                Some(a)
            })
            .collect();
        found.sort_by_key(|a| std::cmp::Reverse(a.score));
        found.truncate(NARROWED_LIMIT);
        p.reactions = found;
    }

    /// Asks `find` for what's typed, numbered so only its answer is kept.
    fn ask(&mut self) {
        let Some(p) = self.palette.as_mut() else { return };
        p.selected = 0;
        p.top = 0;
        p.failed = None;
        let kind = match p.narrow {
            Narrow::All => None,
            Narrow::Kind(k) => Some(k),
            Narrow::Actions => return,
        };
        self.asked += 1;
        p.asked = self.asked;
        p.answered = false;
        let limit = if kind.is_some() { NARROWED_LIMIT } else { LIMIT };
        let scope = p.scope.as_ref().map(|(id, _)| id.clone());
        let query = p.query.text().to_string();
        self.jawd.find(self.asked, &query, kind, scope.as_deref(), limit);
    }

    pub fn found(&mut self, id: u64, results: Vec<Found>, counts: Counts) {
        let Some(p) = self.palette.as_mut().filter(|p| p.asked == id) else { return };
        match p.narrow {
            Narrow::Kind(k) => {
                p.strip.insert(k, counts.get(&k).copied().unwrap_or(0));
            }
            _ => p.strip = counts.clone(),
        }
        p.results = results;
        p.counts = counts;
        p.answered = true;
        p.selected = 0;
        p.top = 0;
        self.dirty = true;
    }

    pub fn find_failed(&mut self, id: u64, cancelled: bool, why: String) {
        if let Some(p) = self.palette.as_mut().filter(|p| p.asked == id && !cancelled) {
            p.failed = Some(why);
            p.answered = true;
            self.dirty = true;
        }
    }

    /// The open chat's row, from the list or, for one found outside it, as it was found.
    pub fn chat_known(&self, id: &str) -> Option<&Chat> {
        self.chats.get(id).or(self.outside.as_ref().filter(|c| c.id == id))
    }

    /// The interface's own actions, those that apply now.
    fn actions(&self) -> Vec<Action> {
        let mut all = vec![
            Action::new("Start a chat", "with anyone, or a number", Does::StartChat),
            Action::new("Go to unread", self.unread_chats(), Does::GoToUnread),
            Action::new("Search messages", "everywhere", Does::SearchMessages).key("/search"),
        ];
        if self.experiments.quick_look {
            all.push(Action::new("Quick look", "the chats that need you, staying here", Does::QuickLook).key("ctrl-g"));
        }
        if self.experiments.focus {
            let (name, detail) = if self.focused() {
                ("All chats again", "the chat list and sidebar back")
            } else {
                ("Focus on this chat", "the chat list and sidebar folded away")
            };
            all.push(Action::new(name, detail, Does::FocusMode).key("ctrl-f"));
        }
        if self.experiments.panes {
            all.push(
                Action::new(
                    "Split this pane across",
                    "another chat beside it",
                    Does::Pane(PaneDo::Split(Split::Across)),
                )
                .key("ctrl-p %"),
            );
            all.push(
                Action::new("Split this pane down", "another chat below it", Does::Pane(PaneDo::Split(Split::Down)))
                    .key("ctrl-p \""),
            );
            if self.split() {
                all.push(Action::new("Next pane", "the keys to the next", Does::Pane(PaneDo::Next)).key("ctrl-p o"));
                all.push(
                    Action::new("Close this pane", "its chat off the screen", Does::Pane(PaneDo::Close))
                        .key("ctrl-p x"),
                );
            }
        }
        if let super::Focus::Message(id) = &self.focus
            && let Some(m) = self.messages.as_ref().and_then(|w| w.get(id))
        {
            let on = |name: &str, key: &'static str, c: char| {
                Action::new(name, "the selected message", Does::OnMessage(id.clone(), c)).key(key)
            };
            if m.mine && m.status == Some(crate::data::Tick::Failed) {
                all.push(Action::new("Send it again", "it wasn't sent", Does::OnMessage(id.clone(), '\n')).key("⏎"));
            }
            all.extend([on("Reply to this message", "r", 'r'), on("React to this message", "+", '+')]);
            if self.not_editable(m).is_none() {
                all.push(on("Edit this message", "e", 'e'));
            }
            all.extend([
                on("Delete this message", "d", 'd'),
                on("Forward this message", "f", 'f'),
                on("This message's info", "i", 'i'),
                on("Copy this message", "y", 'y'),
            ]);
        }
        all.push(Action::new("New folder", "at the top of the tree", Does::NewFolder(None)));
        if let Some(f) = self.folder(&self.folder).filter(|f| f.yours()) {
            let (id, name) = (f.id.clone(), f.name.clone());
            all.extend([
                Action::new(format!("New folder in {name}"), "nested in it", Does::NewFolder(Some(id.clone()))),
                Action::new(format!("Rename {name}"), "the folder shown", Does::Rename(id.clone())),
                Action::new(format!("Move {name} into another folder"), "or to the top", Does::MoveFolder(id.clone())),
                Action::new(
                    format!("Delete the folder {name}"),
                    "its chats stay wherever else they are",
                    Does::DeleteFolder(id.clone()),
                ),
            ]);
            if let Some(chat) = self.open.as_ref().and_then(|o| self.chats.get(o)).filter(|c| c.folders.contains(&id)) {
                all.push(Action::new(
                    format!("Take this chat out of {name}"),
                    chat.name.clone(),
                    Does::Unfile(id, chat.id.clone()),
                ));
            }
        }
        let shown = if self.sidebar_shown() { "Fold the sidebar" } else { "Show the sidebar" };
        all.push(Action::new(shown, "the folders and the roster", Does::Sidebar).key("ctrl-b"));
        // "This chat" is the one open, or the one selected while the list has the screen.
        let this = if self.narrow() && self.picking { self.selected.as_deref() } else { self.open.as_deref() };
        if let Some(c) = this.and_then(|id| self.chat_known(id)) {
            let (id, name) = (c.id.clone(), c.name.clone());
            all.push(Action::new("Search this chat", name.clone(), Does::SearchHere(id.clone(), name.clone())));
            all.push(if c.muted(self.now()) {
                Action::new("Unmute this chat", name.clone(), Does::Unmute(id.clone()))
            } else {
                Action::new(
                    "Mute this chat",
                    format!("{name} · ⏎ asks for how long"),
                    Does::Mute(id.clone(), name.clone()),
                )
            });
            let (verb, archived) = if c.archived { ("Unarchive", false) } else { ("Archive", true) };
            all.push(Action::new(format!("{verb} this chat"), name.clone(), Does::Archive(id.clone(), archived)));
            let (verb, pinned) = if c.pinned { ("Unpin", false) } else { ("Pin", true) };
            all.push(Action::new(format!("{verb} this chat"), name.clone(), Does::Pin(id.clone(), pinned)));
            let unread = c.unread > 0 || c.marked_unread;
            let verb = if unread { "Mark this chat read" } else { "Mark this chat unread" };
            all.push(Action::new(verb, name, Does::MarkUnread(id, !unread)));
        }
        let shown = self.settings_shown();
        all.push(Action::new("Settings", "the theme, where your messages sit", Does::Settings).key("ctrl-,"));
        let theme = themes::or_default(&shown.theme).name.clone();
        all.push(Action::new("Change the theme", format!("{theme} now; previewed as you move"), Does::Theme));
        all.push(match shown.messages {
            Messages::Bubbles => Action::new(
                "Turn the IRC layout on",
                "your messages on the left, under \"You\"",
                Does::Messages(Messages::Irc),
            ),
            Messages::Irc => Action::new(
                "Turn the IRC layout off",
                "your messages back on the right",
                Does::Messages(Messages::Bubbles),
            ),
        });
        all.extend([
            Action::new("New poll", "in this chat", Does::Later("polls come with M2")).key("/poll").later("M2"),
            Action::new("New group", "with people you pick", Does::Later("groups come with M2")).later("M2"),
            Action::new("Attach files", "photos, videos, documents", Does::Later("attaching comes with M4"))
                .key("ctrl-o")
                .later("M4"),
            Action::new("Record a voice message", "in this chat", Does::Record).key("ctrl-r"),
            Action::new("Library", "everything shared, in every chat", Does::Library).key("ctrl-l"),
            Action::new("File this chat into folders", "", Does::File).key("m"),
            Action::new("The keys", "every key jaw has", Does::Keys).key("?"),
            Action::new("Quit", "", Does::Quit).key("ctrl-c"),
        ]);
        all
    }

    fn unread_chats(&self) -> String {
        let n = self.chats.rows().filter(|c| c.unread > 0).count();
        match n {
            0 => "nothing unread".into(),
            1 => "1 chat".into(),
            n => format!("{n} chats"),
        }
    }

    /// The actions that match what's typed, best first; before anything's typed, the common ones.
    fn matching_actions(&self, query: &str) -> Vec<Action> {
        let all = self.actions();
        if query.trim().is_empty() {
            let common =
                ["Start a chat", "Go to unread", "New poll", "Attach files", "Record a voice message", "Library"];
            return all
                .into_iter()
                .filter(|a| common.contains(&a.name.as_str()) || a.does == Does::QuickLook)
                .collect();
        }
        let mut found: Vec<Action> = all
            .into_iter()
            .filter_map(|mut a| {
                let (score, marks) = find::name(query, &a.name)?;
                a.score = score;
                a.marks = marks;
                Some(a)
            })
            .collect();
        found.sort_by_key(|a| std::cmp::Reverse(a.score));
        found
    }

    /// The palette's rows: headings and what's under them, in the order they show.
    pub fn palette_items(&self) -> Vec<Item> {
        let Some(p) = &self.palette else { return vec![] };
        match &p.mode {
            Mode::File { name, ticked, .. } => {
                let mut items = vec![Item::Heading("file into", fmt::fit(name, 48))];
                for (id, depth) in self.your_folders() {
                    let on = ticked.contains(&id);
                    let mut a =
                        Action::new(format!("{}{}", "    ".repeat(depth), self.folder_name(&id)), "", Does::Tick(id));
                    a.lead = Some(if on { "[x]" } else { "[ ]" }.into());
                    items.push(Item::Action(a));
                }
                return items;
            }
            Mode::Name { folder, parent, .. } => {
                let what = match (folder, parent) {
                    (Some(f), _) => format!("renaming {}", self.folder_name(f)),
                    (None, Some(p)) => format!("a new folder in {}", self.folder_name(p)),
                    (None, None) => "a new folder".into(),
                };
                return vec![Item::Heading("folder", what)];
            }
            Mode::Move { folder } => {
                let mut items = vec![Item::Heading("move", format!("{} into", self.folder_name(folder)))];
                items.push(Item::Action(Action::new(
                    "The top",
                    "with no folder above it",
                    Does::MoveTo(folder.clone(), None),
                )));
                let under = self.subtree(folder);
                for (id, depth) in self.your_folders().into_iter().filter(|(id, _)| !under.contains(id)) {
                    let name = format!("{}{}", "    ".repeat(depth), self.folder_name(&id));
                    items.push(Item::Action(Action::new(name, "", Does::MoveTo(folder.clone(), Some(id)))));
                }
                return items;
            }
            _ => {}
        }
        match &p.mode {
            Mode::React { message, summary } => return self.react_items(message, summary, p.query.text()),
            Mode::Delete { message, summary, everyone } => {
                let mut items = vec![Item::Heading("delete", fmt::fit(summary, 48))];
                let me =
                    Action::new("For me", "from jaw and your other devices", Does::DeleteFor(message.clone(), false));
                items.push(Item::Action(me));
                let all = match everyone {
                    None => Action::new("For everyone", "", Does::DeleteFor(message.clone(), true)),
                    Some(why) => Action::new("For everyone", "", Does::Later(why)).later(why),
                };
                items.push(Item::Action(all));
                return items;
            }
            _ => {}
        }
        if let Some((id, name)) = &p.muting {
            let now = self.now();
            let mut items = vec![Item::Heading("mute", name.clone())];
            for (label, until, said) in [
                ("For 8 hours", now + 8 * 3_600_000, "muted for 8 hours"),
                ("For a week", now + 7 * fmt::DAY, "muted for a week"),
                ("Always", i64::MAX, "muted"),
            ] {
                items.push(Item::Action(Action::new(label, "", Does::MuteFor(id.clone(), until, said))));
            }
            return items;
        }
        let query = p.query.text();
        let actions = if p.scope.is_some() { vec![] } else { self.matching_actions(query) };
        let of = |k: FoundKind| p.results.iter().enumerate().filter(move |(_, r)| r.kind == k).map(|(i, _)| i);
        let mut items = vec![];
        let group = |items: &mut Vec<Item>, label: &'static str, found: Vec<Item>, total: usize, all: bool| {
            if found.is_empty() {
                return;
            }
            let shown = if all { found.len() } else { found.len().min(PER_KIND) };
            let more =
                if total > shown { format!("{}, tab shows them all", count(total as u32)) } else { String::new() };
            items.push(Item::Heading(label, more));
            items.extend(found.into_iter().take(shown));
        };
        let whole = p.narrow != Narrow::All;
        let action_items = |actions: Vec<Action>| actions.into_iter().map(Item::Action).collect::<Vec<_>>();
        match p.narrow {
            Narrow::Actions => {
                let n = actions.len();
                group(&mut items, "actions", action_items(actions), n, true);
                return items;
            }
            Narrow::Kind(k) => {
                if let Some(number) = number(query)
                    .filter(|_| matches!(k, FoundKind::Person | FoundKind::Chat))
                    .filter(|_| !matches!(p.mode, Mode::Forward { .. }))
                {
                    let start = Action::new(
                        format!("Start a chat with {number}"),
                        "if it's on WhatsApp",
                        Does::StartPhone(number),
                    );
                    group(&mut items, "a number", vec![Item::Action(start)], 1, true);
                }
                let found: Vec<Item> = of(k).map(Item::Found).collect();
                let total = p.counts.get(&k).copied().unwrap_or(found.len() as u32) as usize;
                group(&mut items, kind_name(k), found, total, whole);
                return items;
            }
            Narrow::All => {}
        }
        if let Some(number) = number(query) {
            let start =
                Action::new(format!("Start a chat with {number}"), "if it's on WhatsApp", Does::StartPhone(number));
            group(&mut items, "a number", vec![Item::Action(start)], 1, true);
        }
        if query.trim().is_empty() {
            let recent: Vec<Item> = of(FoundKind::Chat).map(Item::Found).collect();
            let n = recent.len();
            group(&mut items, "recent", recent, n, false);
            let n = actions.len();
            group(&mut items, "actions", action_items(actions), n, true);
            return items;
        }
        // Typing what you want to do puts the actions first: three letters or more that start one.
        let letters = query.chars().filter(|c| c.is_alphanumeric()).count();
        let verb = letters >= 3 && actions.first().is_some_and(|a| a.score >= 80);
        let n_actions = actions.len();
        let mut actions = Some(action_items(actions));
        if verb {
            group(&mut items, "actions", actions.take().unwrap_or_default(), n_actions, false);
        }
        for k in ORDER {
            if k == FoundKind::Message
                && let Some(a) = actions.take()
            {
                group(&mut items, "actions", a, n_actions, false);
            }
            let found: Vec<Item> = of(k).map(Item::Found).collect();
            let total = p.counts.get(&k).copied().unwrap_or(found.len() as u32) as usize;
            group(&mut items, kind_name(k), found, total, false);
        }
        items
    }

    /// The strip's kinds: all, then each that found something, then the actions if any match.
    fn strip(&self) -> Vec<(Narrow, String)> {
        let Some(p) = &self.palette else { return vec![] };
        let mut out = vec![(Narrow::All, "all".to_string())];
        for k in ORDER {
            if k == FoundKind::Message {
                let n = if p.scope.is_some() { 0 } else { self.matching_actions(p.query.text()).len() };
                if n > 0 && !p.query.text().trim().is_empty() {
                    out.push((Narrow::Actions, format!("actions {n}")));
                }
            }
            match p.strip.get(&k) {
                Some(&n) if n > 0 => out.push((Narrow::Kind(k), format!("{} {}", kind_name(k), count(n)))),
                _ if p.narrow == Narrow::Kind(k) => out.push((Narrow::Kind(k), kind_name(k).to_string())),
                _ => {}
            }
        }
        out
    }

    fn narrow_by(&mut self, by: isize) {
        let strip = self.strip();
        let Some(p) = self.palette.as_mut() else { return };
        let at = strip.iter().position(|(n, _)| *n == p.narrow).unwrap_or(0) as isize;
        let next = (at + by).rem_euclid(strip.len() as isize) as usize;
        p.narrow = strip[next].0;
        self.ask();
    }

    pub fn on_palette_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        let Some(p) = self.palette.as_mut() else { return };
        let before = p.query.text().to_string();
        match (&mut p.mode, k.code) {
            (Mode::React { message, .. }, KeyCode::Char(c @ '1'..='6')) if p.query.is_empty() && !ctrl && !alt => {
                let (message, i) = (message.clone(), c as usize - '1' as usize);
                self.palette = None;
                if let Some(m) = self.messages.as_ref().and_then(|w| w.get(&message)).cloned() {
                    self.react_quick(&m, i);
                }
                return;
            }
            (Mode::React { .. } | Mode::Delete { .. }, KeyCode::Tab | KeyCode::BackTab) => return,
            (Mode::File { .. }, KeyCode::Char(' ')) => {
                let at = p.selected;
                if let Some((id, _)) = self.your_folders().get(at).cloned() {
                    self.act(Does::Tick(id));
                }
                return;
            }
            (Mode::File { .. }, KeyCode::Char('n')) => {
                let then = self.palette.take().map(|p| Box::new(p.mode));
                self.palette_name(None, None, then);
                return;
            }
            (
                Mode::File { .. } | Mode::Move { .. },
                KeyCode::Char(_) | KeyCode::Tab | KeyCode::BackTab | KeyCode::Backspace,
            ) => {
                return;
            }
            (Mode::Name { .. }, KeyCode::Tab | KeyCode::BackTab) => return,
            (Mode::Forward { .. }, KeyCode::Tab) => {
                self.forward_pick();
                return;
            }
            (Mode::Forward { picked, .. }, KeyCode::Backspace) if p.query.is_empty() => {
                picked.pop();
                return;
            }
            _ => {}
        }
        let Some(p) = self.palette.as_mut() else { return };
        let q = &mut p.query;
        match k.code {
            KeyCode::Esc if p.muting.is_some() => p.muting = None,
            KeyCode::Esc => self.palette = None,
            KeyCode::Char('k') if ctrl => self.palette = None,
            KeyCode::Enter => self.palette_act(),
            KeyCode::Char('g') if ctrl => {
                let item = self.palette_selected();
                match item {
                    Some(Item::Found(i))
                        if self.palette_result(i).is_some_and(|r| matches!(r.hit, Hit::Message(_))) =>
                    {
                        self.palette_act()
                    }
                    _ => self.say("ctrl-g goes to a message in its chat; links and files have theirs from M2"),
                }
            }
            KeyCode::Up | KeyCode::Char('p') if k.code == KeyCode::Up || ctrl => self.palette_move(-1),
            KeyCode::Down | KeyCode::Char('n') if k.code == KeyCode::Down || ctrl => self.palette_move(1),
            KeyCode::PageUp => self.palette_move(-8),
            KeyCode::PageDown => self.palette_move(8),
            KeyCode::Tab => self.narrow_by(1),
            KeyCode::BackTab => self.narrow_by(-1),
            KeyCode::Backspace if q.is_empty() => {
                if p.scope.take().is_none() {
                    p.narrow = Narrow::All;
                }
                self.ask();
            }
            KeyCode::Char(c) if ctrl => match c {
                'a' => q.home(),
                'e' => q.end(),
                'u' => q.kill_line_back(),
                'w' => q.kill_to_space(),
                'y' => q.yank(),
                'd' => q.delete(),
                'h' => q.backspace(),
                _ => {}
            },
            KeyCode::Char(c) if alt => match c {
                'b' => q.word_left(),
                'f' => q.word_right(),
                'd' => q.kill_word_forward(),
                _ => {}
            },
            KeyCode::Char(c) => q.insert(c.encode_utf8(&mut [0; 4])),
            KeyCode::Backspace if alt || ctrl => q.kill_word_back(),
            KeyCode::Backspace => q.backspace(),
            KeyCode::Delete => q.delete(),
            KeyCode::Left if alt || ctrl => q.word_left(),
            KeyCode::Right if alt || ctrl => q.word_right(),
            KeyCode::Left => q.left(),
            KeyCode::Right => q.right(),
            KeyCode::Home => q.home(),
            KeyCode::End => q.end(),
            _ => {}
        }
        if let Some(p) = self.palette.as_mut().filter(|p| p.query.text() != before) {
            if let Narrow::Kind(k) = p.narrow {
                p.strip.retain(|kind, _| *kind == k);
            }
            self.search_emoji();
            self.ask();
        }
    }

    pub fn palette_paste(&mut self, s: &str) {
        if let Some(p) = self.palette.as_mut() {
            p.query.insert(&fmt::one_line(s));
            self.ask();
        }
    }

    pub fn palette_move(&mut self, by: isize) {
        let n = self.palette_items().iter().filter(|i| i.selectable()).count();
        if let Some(p) = self.palette.as_mut()
            && n > 0
        {
            p.selected = (p.selected as isize + by).clamp(0, n as isize - 1) as usize;
        }
    }

    fn palette_result(&self, i: usize) -> Option<&Found> {
        self.palette.as_ref()?.results.get(i)
    }

    fn palette_selected(&self) -> Option<Item> {
        let selected = self.palette.as_ref()?.selected;
        self.palette_items().into_iter().filter(Item::selectable).nth(selected)
    }

    /// Tab while forwarding: the selected chat picked, or unpicked.
    fn forward_pick(&mut self) {
        let Some(Item::Found(i)) = self.palette_selected() else { return };
        let Some(Hit::Chat(c)) = self.palette_result(i).map(|f| f.hit.clone()) else { return };
        if let Some(Palette { mode: Mode::Forward { picked, .. }, .. }) = self.palette.as_mut() {
            match picked.iter().position(|(id, _)| *id == c.id) {
                Some(at) => {
                    picked.remove(at);
                }
                None if picked.len() >= 5 => self.say("five chats at most, as the phones allow"),
                None => picked.push((c.id.clone(), c.name.clone())),
            }
        }
    }

    /// Enter while forwarding: to the chats picked, or to the one selected if none are.
    fn forward_go(&mut self) {
        let selected = match self.palette_selected() {
            Some(Item::Found(i)) => match self.palette_result(i).map(|f| f.hit.clone()) {
                Some(Hit::Chat(c)) => Some((c.id, c.name)),
                _ => None,
            },
            _ => None,
        };
        let Some(Palette { mode: Mode::Forward { messages, picked }, .. }) = self.palette.take() else { return };
        let to: Vec<(String, String)> = if picked.is_empty() { selected.into_iter().collect() } else { picked };
        let Some(chat) = self.open.clone() else { return };
        if to.is_empty() {
            return;
        }
        let ids: Vec<String> = to.iter().map(|(id, _)| id.clone()).collect();
        self.jawd.forward(&chat, &messages, &ids);
        let said = match to.as_slice() {
            [(_, name)] => format!("forwarding to {name}"),
            more => format!("forwarding to {} chats", more.len()),
        };
        self.say(said);
    }

    /// Enter: opens a chat, goes to a message, starts a chat with someone, or does an action.
    pub fn palette_act(&mut self) {
        match self.palette.as_ref().map(|p| &p.mode) {
            Some(Mode::Forward { .. }) => return self.forward_go(),
            Some(Mode::File { .. }) => return self.file_go(),
            Some(Mode::Name { .. }) => return self.name_go(),
            _ => {}
        }
        let Some(item) = self.palette_selected() else { return };
        match item {
            Item::Heading(..) => {}
            Item::Found(i) => {
                let Some(found) = self.palette_result(i).cloned() else { return };
                match found.hit {
                    Hit::Chat(c) => {
                        self.palette = None;
                        let id = c.id.clone();
                        if self.chats.get(&id).is_none() {
                            self.outside = Some(c);
                        }
                        self.open_chat(&id);
                        self.jawd.opened(&id);
                    }
                    Hit::Message(m) => {
                        self.palette = None;
                        self.go_to_message(&m.chat, &m.id);
                    }
                    Hit::Person(person) => {
                        self.palette = None;
                        self.starting = Some(person.id.clone());
                        self.jawd.start_chat(&person.id);
                        self.say(format!("a chat with {} begins", person.name));
                    }
                    Hit::Folder(folder) => {
                        self.palette = None;
                        self.pick_folder(&folder.id);
                    }
                    Hit::Shared => {
                        let words = self.palette.take().map(|p| p.query.text().to_string()).unwrap_or_default();
                        let kind = match found.kind {
                            FoundKind::Link => crate::data::LibraryKind::Link,
                            FoundKind::File => crate::data::LibraryKind::File,
                            _ => crate::data::LibraryKind::Photo,
                        };
                        self.open_library(None, Some(kind), words.trim());
                    }
                }
            }
            Item::Action(a) => self.act(a.does),
        }
    }

    fn act(&mut self, does: Does) {
        let close = |v: &mut View| v.palette = None;
        match does {
            Does::StartChat => {
                if let Some(p) = self.palette.as_mut() {
                    p.narrow = Narrow::Kind(FoundKind::Person);
                    p.query.take();
                }
                self.ask();
            }
            Does::Record => {
                close(self);
                self.focus = super::Focus::Typing;
                match self.open.is_some() {
                    true => self.record(),
                    false => self.say("open a chat to record for it"),
                }
            }
            Does::GoToUnread => {
                close(self);
                let order: Vec<String> = self.chats.rows().map(|c| c.id.clone()).collect();
                let at = self.open.as_ref().and_then(|o| order.iter().position(|id| id == o)).map_or(0, |i| i + 1);
                let next = (0..order.len()).map(|k| &order[(at + k) % order.len()]).find(|id| {
                    self.chats
                        .get(id)
                        .is_some_and(|c| (c.unread > 0 || c.marked_unread) && self.open.as_ref() != Some(*id))
                });
                match next.cloned() {
                    Some(id) => {
                        self.open_chat(&id);
                        self.jawd.opened(&id);
                    }
                    None => self.say("nothing unread"),
                }
            }
            Does::SearchMessages => self.palette_search(None, ""),
            Does::SearchHere(id, name) => self.palette_search(Some((id, name)), ""),
            Does::Mute(id, name) => {
                if let Some(p) = self.palette.as_mut() {
                    p.muting = Some((id, name));
                    p.selected = 0;
                }
            }
            Does::MuteFor(id, until, said) => {
                close(self);
                self.jawd.mute(&id, Some(until));
                self.say(said);
            }
            Does::Unmute(id) => {
                close(self);
                self.jawd.mute(&id, None);
                self.say("unmuted");
            }
            Does::Archive(id, archived) => {
                close(self);
                self.jawd.archive(&id, archived);
                self.say(if archived { "archived; the palette still finds it" } else { "back in the chat list" });
            }
            Does::Pin(id, pinned) => {
                close(self);
                self.jawd.pin(&id, pinned);
                self.say(if pinned { "pinned" } else { "unpinned" });
            }
            Does::MarkUnread(id, unread) => {
                close(self);
                self.jawd.mark_unread(&id, unread);
                self.say(if unread { "marked unread" } else { "marked read" });
            }
            Does::Keys => {
                close(self);
                self.overlay = Some(Overlay::Keys);
            }
            Does::Quit => self.quit = true,
            Does::Later(said) => self.say(said),
            Does::StartPhone(phone) => {
                close(self);
                self.starting = Some(phone.clone());
                self.jawd.start_chat_by_phone(&phone);
                self.say(format!("a chat with {phone} begins"));
            }
            Does::NewFolder(parent) => self.palette_name(None, parent, None),
            Does::Rename(id) => self.palette_name(Some(id), None, None),
            Does::MoveFolder(folder) => {
                let mut p = Palette::new();
                p.narrow = Narrow::Actions;
                p.answered = true;
                p.mode = Mode::Move { folder };
                self.palette = Some(p);
            }
            Does::MoveTo(folder, parent) => {
                close(self);
                let name = self.folder_name(&folder);
                let into = parent.as_deref().map_or_else(|| "the top".to_string(), |p| self.folder_name(p));
                self.jawd.folder_move(&folder, parent.as_deref());
                self.say(format!("{name} moved to {into}"));
            }
            Does::DeleteFolder(folder) => {
                close(self);
                let name = self.folder_name(&folder);
                self.jawd.folder_delete(&folder);
                self.say(format!("{name} deleted; its chats stay wherever else they are"));
            }
            Does::File => {
                close(self);
                self.file_this();
            }
            Does::Tick(id) => {
                if let Some(Palette { mode: Mode::File { ticked, .. }, .. }) = self.palette.as_mut() {
                    match ticked.iter().position(|t| *t == id) {
                        Some(i) => {
                            ticked.remove(i);
                        }
                        None => ticked.push(id),
                    }
                }
            }
            Does::Unfile(folder, chat) => {
                close(self);
                self.jawd.folder_remove(&folder, &chat);
                self.say(format!("taken out of {}", self.folder_name(&folder)));
            }
            Does::Sidebar => {
                close(self);
                self.toggle_sidebar();
            }
            Does::Library => {
                close(self);
                self.open_library(None, None, "");
            }
            Does::Settings => {
                close(self);
                self.open_settings(false);
            }
            Does::Theme => {
                close(self);
                self.open_settings(true);
            }
            Does::Messages(messages) => {
                close(self);
                self.ask_settings(SettingsSet { theme: None, messages: Some(messages) });
            }
            Does::QuickLook => {
                close(self);
                self.open_quick_look();
            }
            Does::FocusMode => {
                close(self);
                self.toggle_focus_mode();
            }
            Does::Pane(does) => {
                close(self);
                self.pane_do(does);
            }
            Does::OnMessage(id, c) => {
                close(self);
                let code = if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) };
                let k = KeyEvent::new(code, KeyModifiers::NONE);
                self.on_message_key(k, &id, false, false);
            }
            Does::React(id, emoji) => {
                close(self);
                if let Some(m) = self.messages.as_ref().and_then(|w| w.get(&id)).cloned() {
                    self.react(&m, &emoji);
                }
            }
            Does::DeleteFor(id, everyone) => {
                close(self);
                if let Some(chat) = self.open.clone() {
                    self.jawd.delete(&chat, &id, everyone);
                }
                self.focus = super::Focus::Typing;
            }
        }
    }

    /// Opens a message's chat anchored on it, the message selected.
    pub(super) fn go_to_message(&mut self, chat: &str, id: &str) {
        self.jump = Some(id.to_string());
        if self.open.as_deref() == Some(chat) {
            self.asked_place = None;
            self.watch(chat, View::around(id));
        } else {
            self.open_chat_at(chat, Some(id.to_string()));
            self.jawd.opened(chat);
        }
        self.picking = false;
    }

    fn marks(&self, text: &[Mark], plain: Style) -> Vec<Span<'static>> {
        let t = &self.theme;
        let mark = Style::new().fg(t.mark).add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        text.iter()
            .map(|m| Span::styled(m.s.replace('\n', " "), if m.t == MarkKind::Mark { mark } else { plain }))
            .collect()
    }

    /// A result's glyph, its name or words with the match marked, and what Enter would do.
    fn found_row(&self, r: &Found) -> (Span<'static>, Vec<Span<'static>>, Vec<Span<'static>>) {
        let t = &self.theme;
        let dim = |s: String| vec![Span::styled(s, Style::new().fg(t.comment))];
        let name = Style::new().fg(t.fg);
        match &r.hit {
            Hit::Chat(c) => {
                let lead =
                    if c.typing.is_empty() { Span::raw(" ") } else { Span::styled("●", Style::new().fg(t.online)) };
                let mut said = vec![if c.group() { "group".to_string() } else { "chat".to_string() }];
                if c.unread > 0 {
                    said.push(format!("{} unread", c.unread));
                }
                if c.archived {
                    said.push("archived".into());
                }
                if let Some(last) = &c.last {
                    said.push(fmt::when(last.at, self.now(), self.tz));
                }
                said.extend(r.folders.iter().cloned());
                (lead, self.marks(&r.text, name), dim(said.join(" · ")))
            }
            Hit::Person(_) => (
                Span::styled("○", Style::new().fg(t.comment)),
                self.marks(&r.text, name),
                dim("no chat yet · ⏎ starts one".into()),
            ),
            Hit::Folder(_) => {
                (Span::styled("◇", Style::new().fg(t.comment)), self.marks(&r.text, name), dim("folder · M6".into()))
            }
            Hit::Message(m) => {
                let colour = m.from.as_ref().map_or(t.comment, |f| t.sender(f.colour));
                let who = m.from.as_ref().map_or_else(|| "you".to_string(), |f| f.name.clone());
                let mut main =
                    vec![Span::styled(format!("{who} "), Style::new().fg(colour).add_modifier(Modifier::BOLD))];
                let here = self.palette.as_ref().and_then(|p| p.scope.as_ref()).is_some();
                if let Some(chat) = r.chat_name.as_ref().filter(|c| !here && **c != who) {
                    main.push(Span::styled(format!("in {chat} "), Style::new().fg(t.comment)));
                }
                main.extend(self.marks(&r.text, Style::new().fg(t.text)));
                (Span::styled("┃", Style::new().fg(colour)), main, dim(fmt::when(m.at, self.now(), self.tz)))
            }
            Hit::Shared => {
                (Span::styled("↗", Style::new().fg(t.link)), self.marks(&r.text, name), dim("in the library".into()))
            }
        }
    }

    fn action_row(&self, a: &Action) -> (Span<'static>, Vec<Span<'static>>, Vec<Span<'static>>) {
        let t = &self.theme;
        let later = a.later.is_some();
        let name = Style::new().fg(if later { t.muted } else { t.fg });
        let text = find::marked(&a.name, &a.marks);
        let mut detail = vec![];
        if !a.detail.is_empty() {
            detail.push(Span::styled(a.detail.clone(), Style::new().fg(t.comment)));
        }
        if let Some(key) = a.key {
            if !detail.is_empty() {
                detail.push(Span::raw("  "));
            }
            let style =
                if later { Style::new().fg(t.comment) } else { Style::new().fg(t.focus).add_modifier(Modifier::BOLD) };
            detail.push(Span::styled(key, style));
        }
        if let Some(when) = a.later {
            detail.push(Span::styled(format!(" · {when}"), Style::new().fg(t.gutter)));
        }
        let lead = match &a.lead {
            Some(l) => Span::raw(l.clone()),
            None => Span::styled("▸", Style::new().fg(t.comment)),
        };
        (lead, self.marks(&text, name), detail)
    }

    pub fn draw_palette(&mut self, f: &mut Frame) {
        let t = self.theme;
        let screen = f.area();
        let w = WIDTH.min(screen.width.saturating_sub(4));
        let h = HEIGHT.min(screen.height.saturating_sub(3));
        if w < 30 || h < 6 {
            return;
        }
        let area = Rect::new(screen.x + (screen.width - w) / 2, screen.y + 2, w, h);
        // The screen behind it, quieter.
        for y in screen.y..screen.y + screen.height {
            for x in screen.x..screen.x + screen.width {
                if !(x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height) {
                    let cell = &mut f.buffer_mut()[(x, y)];
                    cell.set_fg(t.gutter);
                    cell.modifier = Modifier::empty();
                }
            }
        }
        f.render_widget(Clear, area);
        let mode = self.palette.as_ref().map_or(Mode::Find, |p| p.mode.clone());
        let (title, foot) = match &mode {
            Mode::File { .. } => (" file ", " space ticks · n new folder · ⏎ files · esc closes "),
            Mode::Name { .. } => (" folder ", " ⏎ names it · esc closes "),
            Mode::Move { .. } => (" move ", " ↑↓ moves · ⏎ puts it there · esc closes "),
            Mode::Find => (" palette ", " ↑↓ moves · tab narrows · ⏎ does it · ctrl-g its message · esc closes "),
            Mode::React { .. } => (" react ", " 1-6 the quick ones · type for any other · ⏎ reacts · esc closes "),
            Mode::Delete { .. } => (" delete ", " ↑↓ moves · ⏎ deletes · esc closes "),
            Mode::Forward { .. } => (" forward ", " tab picks a chat · ⏎ forwards · esc closes "),
        };
        let foot = Line::from(Span::styled(foot, Style::new().fg(t.muted)));
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(t.focus))
            .style(Style::new().bg(t.surface).fg(t.fg))
            .title(Span::styled(title, Style::new().fg(t.text)))
            .title_bottom(foot)
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let iw = inner.width as usize;
        let items = self.palette_items();
        let strip = self.strip();
        let actions = self.palette.as_ref().map_or(0, |p| {
            if p.scope.is_some() || p.narrow != Narrow::All {
                0
            } else {
                self.matching_actions(p.query.text()).len() as u32
            }
        });
        let Some(p) = self.palette.as_mut() else { return };

        let mut query = vec![Span::styled("› ", Style::new().fg(t.comment))];
        if let Some((_, name)) = &p.scope {
            query.push(Span::styled(format!(" in {name} "), Style::new().fg(t.fg).bg(t.selection)));
            query.push(Span::raw(" "));
        }
        let lead: usize = query.iter().map(|s| s.width()).sum();
        let (_, (_, col)) = p.query.view(10_000);
        let text = p.query.text().to_string();
        query.push(Span::styled(text.clone(), Style::new().fg(t.fg)));
        if text.is_empty() {
            let hint = if p.muting.is_some() {
                "for how long?"
            } else if let Mode::Name { .. } = p.mode {
                "the folder's name"
            } else if let Mode::File { .. } | Mode::Move { .. } = p.mode {
                ""
            } else if let Mode::React { .. } = p.mode {
                "an emoji's name, or 1 to 6"
            } else if let Mode::Delete { .. } = p.mode {
                "for whom?"
            } else if let Mode::Forward { .. } = p.mode {
                "a chat to forward to"
            } else if p.scope.is_some() {
                "words said in this chat"
            } else {
                match p.narrow {
                    Narrow::Kind(FoundKind::Person) => "a name or a number",
                    Narrow::Kind(FoundKind::Message) => "words said anywhere",
                    _ => "a chat, a person, something to do, or words",
                }
            };
            query.push(Span::styled(format!(" {hint}"), Style::new().fg(t.gutter)));
        }
        let found: u32 = p.counts.values().sum::<u32>() + actions;
        let right = match (&p.failed, p.answered) {
            (Some(_), _) => String::new(),
            (None, false) => "finding… ".into(),
            (None, true) if text.is_empty() => String::new(),
            (None, true) => format!("{} found ", count(found)),
        };
        let mut line = fit_spans(query, iw.saturating_sub(right.chars().count()));
        let used: usize = line.iter().map(|s| s.width()).sum();
        line.push(Span::raw(" ".repeat(iw.saturating_sub(used + right.chars().count()))));
        line.push(Span::styled(right, Style::new().fg(t.comment)));
        let buf = f.buffer_mut();
        buf.set_line(inner.x, inner.y, &Line::from(line), inner.width);
        if let Mode::Forward { picked, .. } = &p.mode {
            let mut spans = vec![Span::styled("to ", Style::new().fg(t.comment))];
            if picked.is_empty() {
                spans.push(Span::styled("the chat you pick, or several with tab", Style::new().fg(t.gutter)));
            }
            for (i, (_, name)) in picked.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::styled(format!(" {name} "), Style::new().fg(t.fg).bg(t.selection)));
            }
            buf.set_line(inner.x, inner.y + 1, &Line::from(fit_spans(spans, iw)), inner.width);
        } else if p.muting.is_none() && p.mode == Mode::Find {
            let mut spans = vec![];
            for (i, (n, label)) in strip.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw(" "));
                }
                let style = if *n == p.narrow {
                    Style::new().fg(t.fg).bg(t.selection).add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(t.comment)
                };
                spans.push(Span::styled(format!(" {label} "), style));
            }
            let used: usize = spans.iter().map(|s| s.width()).sum();
            let tab = "tab narrows";
            if used + tab.len() < iw {
                spans.push(Span::raw(" ".repeat(iw - used - tab.len())));
                spans.push(Span::styled(tab, Style::new().fg(t.gutter)));
            }
            buf.set_line(inner.x, inner.y + 1, &Line::from(fit_spans(spans, iw)), inner.width);
        }
        let list_y = inner.y + 3;
        let rows = inner.height.saturating_sub(3) as usize;
        if let Some(why) = p.failed.clone() {
            let line = Line::from(Span::styled(format!("jawd couldn't search: {why}"), Style::new().fg(t.warn)));
            buf.set_line(inner.x, list_y, &line, inner.width);
            return;
        }
        if items.is_empty() && p.answered && !text.is_empty() {
            let line = Line::from(Span::styled("Nothing matches.", Style::new().fg(t.comment)));
            buf.set_line(inner.x, list_y, &line, inner.width);
            return;
        }
        let selectable = items.iter().filter(|i| i.selectable()).count();
        p.selected = p.selected.min(selectable.saturating_sub(1));
        let sel_row = items.iter().enumerate().filter(|(_, i)| i.selectable()).nth(p.selected).map_or(0, |(r, _)| r);
        if sel_row < p.top {
            p.top = sel_row.saturating_sub(1);
        } else if sel_row >= p.top + rows {
            p.top = sel_row + 1 - rows;
        }
        let top = p.top;
        let cursor = (inner.x + (lead + col).min(iw.saturating_sub(1)) as u16, inner.y);
        let mut drawn = vec![];
        for (r, item) in items.iter().enumerate().skip(top).take(rows) {
            let y = list_y + (r - top) as u16;
            let line = match item {
                Item::Heading(label, more) => {
                    let mut spans =
                        vec![Span::styled(label.to_string(), Style::new().fg(t.muted).add_modifier(Modifier::BOLD))];
                    if !more.is_empty() {
                        spans.push(Span::styled(format!(" · {more}"), Style::new().fg(t.comment)));
                    }
                    let used: usize = spans.iter().map(|s| s.width()).sum::<usize>() + 1;
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled("─".repeat(iw.saturating_sub(used)), Style::new().fg(t.gutter)));
                    Line::from(spans)
                }
                Item::Found(i) => {
                    let Some(r) = self.palette.as_ref().and_then(|p| p.results.get(*i)) else { continue };
                    let (mut lead, main, detail) = self.found_row(r);
                    if let (Mode::Forward { picked, .. }, Hit::Chat(c)) = (&mode, &r.hit)
                        && picked.iter().any(|(id, _)| *id == c.id)
                    {
                        lead = Span::styled("✓", Style::new().fg(t.progress).add_modifier(Modifier::BOLD));
                    }
                    self.hit_line(lead, main, detail, iw)
                }
                Item::Action(a) => {
                    let (lead, main, detail) = self.action_row(a);
                    self.hit_line(lead, main, detail, iw)
                }
            };
            drawn.push((y, line, r == sel_row && item.selectable()));
        }
        let buf = f.buffer_mut();
        for (y, line, selected) in drawn {
            buf.set_line(inner.x, y, &line, inner.width);
            if selected {
                buf.set_style(
                    Rect::new(area.x + 1, y, area.width - 2, 1),
                    Style::new().bg(t.selection).add_modifier(Modifier::BOLD),
                );
            }
        }
        f.set_cursor_position(cursor);
    }

    fn hit_line(
        &self,
        lead: Span<'static>,
        main: Vec<Span<'static>>,
        detail: Vec<Span<'static>>,
        w: usize,
    ) -> Line<'static> {
        let detail_w: usize = detail.iter().map(|s| s.width()).sum();
        let room = w.saturating_sub(4 + detail_w + 2);
        let mut spans = vec![Span::raw("  "), lead, Span::raw(" ")];
        let main = fit_spans(main, room);
        let used = 4 + main.iter().map(|s| s.width()).sum::<usize>();
        spans.extend(main);
        spans.push(Span::raw(" ".repeat(w.saturating_sub(used + detail_w + 1))));
        spans.extend(detail);
        Line::from(spans)
    }

    /// Where a click in the palette lands: the selectable row under it, if any.
    pub fn palette_click(&mut self, y: u16, screen: Rect) -> bool {
        let w = WIDTH.min(screen.width.saturating_sub(4));
        let h = HEIGHT.min(screen.height.saturating_sub(3));
        let area = Rect::new(screen.x + (screen.width - w) / 2, screen.y + 2, w, h);
        if y < area.y || y >= area.y + area.height {
            self.palette = None;
            return false;
        }
        let items = self.palette_items();
        let Some(p) = self.palette.as_mut() else { return false };
        let row = (y as isize - (area.y as isize + 4)) + p.top as isize;
        if row < 0 || row as usize >= items.len() || !items[row as usize].selectable() {
            return true;
        }
        p.selected = items[..row as usize].iter().filter(|i| i.selectable()).count();
        self.palette_act();
        true
    }
}
