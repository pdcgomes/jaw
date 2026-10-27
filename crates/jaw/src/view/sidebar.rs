//! The sidebar: the folder tree with each folder's unread total, then the roster of the folder
//! picked, as the wireframes have them. It sits left of the chat list from 120 columns, and
//! Ctrl-B gives it the keys, folds it away, or below 120 draws it over the list while it has them.
//! Picking a folder makes the chat list that folder's window; Tab moves to the roster, whose
//! Enter opens or starts someone's chat. Alt-Shift-Up and Alt-Shift-Down move between folders
//! from anywhere, and a click does what Enter does on the folder or person under it, or opens or
//! closes a folder on its mark, leaving the keys where they were.

use super::{Focus, View};
use crate::data::Place;
use crate::data::{FolderRow, PresenceState, RosterRow};
use crate::fmt;
use jaw_client::Chats;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Clear;
use std::collections::HashSet;

/// The sidebar's width, and the narrowest screen it sits beside the list on.
pub const SIDEBAR: u16 = 26;
pub const SIDEBAR_FROM: u16 = 120;

/// Which half of the sidebar has the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Folders,
    Roster,
}

#[derive(Debug, Default)]
pub struct Sidebar {
    /// Folded away by Ctrl-B, on a screen wide enough to show it.
    pub folded: bool,
    /// Which half has the keys, when the sidebar does.
    pub keys: Option<Part>,
    /// The tree's row and the roster's row the keys are on.
    pub at: usize,
    pub roster_at: usize,
    /// Your folders opened to show the folders in them.
    pub open: HashSet<String>,
}

/// A line of the tree: a folder at a depth, or the gap between the built-in, yours and smart ones.
#[derive(Clone, Debug, PartialEq)]
pub enum Tree {
    Folder { id: String, depth: usize, children: bool },
    Gap,
}

/// A line of the roster: a folder's heading under All, someone, or how many more are offline.
#[derive(Clone, Debug, PartialEq)]
pub enum Roster {
    Heading { name: String, online: usize, of: usize },
    Person(Box<RosterRow>),
    Offline(usize),
}

/// What a line of the sidebar is to the mouse: a folder, with its mark's column if it has folders
/// in it, or someone in the roster.
enum Hit {
    Folder(String, Option<u16>),
    Person(String),
}

/// The built-in folders, before jawd has said its counts.
pub fn built_in() -> Vec<FolderRow> {
    [
        ("all", "All"),
        ("unread", "Unread"),
        ("groups", "Groups"),
        ("communities", "Communities"),
        ("archived", "Archived"),
    ]
    .iter()
    .enumerate()
    .map(|(i, (id, name))| FolderRow {
        id: id.to_string(),
        name: name.to_string(),
        position: i as i64,
        ..FolderRow::default()
    })
    .collect()
}

impl View {
    /// Whether the screen is wide enough for the sidebar beside the list.
    pub(super) fn wide(&self) -> bool {
        self.size().is_some_and(|(w, _)| w >= SIDEBAR_FROM)
    }

    /// Whether the sidebar is drawn: beside the list, or over it while it has the keys.
    pub(super) fn sidebar_shown(&self) -> bool {
        (self.wide() && !self.sidebar.folded) || self.sidebar.keys.is_some()
    }

    pub(super) fn folder_name(&self, id: &str) -> String {
        self.folders.iter().find(|f| f.id == id).map_or_else(|| id.to_string(), |f| f.name.clone())
    }

    pub(super) fn folder(&self, id: &str) -> Option<&FolderRow> {
        self.folders.iter().find(|f| f.id == id)
    }

    /// Ctrl-B: the keys into the sidebar, or folded away with them.
    pub(super) fn toggle_sidebar(&mut self) {
        if self.sidebar.keys.take().is_some() {
            if self.wide() {
                self.sidebar.folded = true;
            }
            return;
        }
        self.sidebar.folded = false;
        self.sidebar.keys = Some(Part::Folders);
        self.sidebar.at = self.tree_row(&self.folder).unwrap_or(0);
    }

    /// The row of the tree a folder is on, if it shows.
    fn tree_row(&self, folder: &str) -> Option<usize> {
        self.tree().iter().position(|t| matches!(t, Tree::Folder { id, .. } if id == folder))
    }

    /// The tree as drawn: built-in folders, yours nested under the ones opened, then smart ones.
    pub(super) fn tree(&self) -> Vec<Tree> {
        self.tree_open(|id| self.sidebar.open.contains(id))
    }

    /// The tree with yours opened where `open` says.
    fn tree_open(&self, open: impl Fn(&str) -> bool) -> Vec<Tree> {
        fn by_position(mut rows: Vec<&FolderRow>) -> Vec<&FolderRow> {
            rows.sort_by_key(|r| (r.position, r.name.clone()));
            rows
        }
        let mut out: Vec<Tree> = by_position(self.folders.iter().filter(|f| f.built_in()).collect())
            .into_iter()
            .map(|f| Tree::Folder { id: f.id.clone(), depth: 0, children: false })
            .collect();
        let yours: Vec<&FolderRow> = self.folders.iter().filter(|f| f.yours()).collect();
        let children = |parent: Option<&str>| -> Vec<&FolderRow> {
            by_position(yours.iter().copied().filter(|f| f.parent.as_deref() == parent).collect())
        };
        let mut stack: Vec<(&FolderRow, usize)> = children(None).into_iter().rev().map(|f| (f, 0)).collect();
        if !stack.is_empty() {
            out.push(Tree::Gap);
        }
        while let Some((f, depth)) = stack.pop() {
            let under = children(Some(&f.id));
            out.push(Tree::Folder { id: f.id.clone(), depth, children: !under.is_empty() });
            if open(&f.id) {
                stack.extend(under.into_iter().rev().map(|c| (c, depth + 1)));
            }
        }
        let smart = by_position(self.folders.iter().filter(|f| f.smart.is_some()).collect());
        if !smart.is_empty() {
            out.push(Tree::Gap);
        }
        out.extend(smart.into_iter().map(|f| Tree::Folder { id: f.id.clone(), depth: 0, children: false }));
        out
    }

    /// The roster's lines: under All every top folder of yours as a group with who's around and
    /// the rest folded into one line; under any other folder its people, around first.
    pub(super) fn roster_lines(&self) -> Vec<Roster> {
        let mut people: Vec<&RosterRow> = self.roster.iter().collect();
        people.sort_by(|a, b| a.sort.cmp(&b.sort));
        if self.folder != "all" {
            return people.into_iter().map(|p| Roster::Person(Box::new(p.clone()))).collect();
        }
        let mut out = vec![];
        let mut tops: Vec<&FolderRow> = self.folders.iter().filter(|f| f.yours() && f.parent.is_none()).collect();
        tops.sort_by_key(|f| f.position);
        let mut placed: HashSet<&str> = HashSet::new();
        let group = |out: &mut Vec<Roster>, name: String, members: Vec<&RosterRow>| {
            if members.is_empty() {
                return;
            }
            let around: Vec<&RosterRow> = members.iter().copied().filter(|p| p.presence.around()).collect();
            out.push(Roster::Heading { name, online: around.len(), of: members.len() });
            out.extend(around.iter().map(|p| Roster::Person(Box::new((*p).clone()))));
            if members.len() > around.len() {
                out.push(Roster::Offline(members.len() - around.len()));
            }
        };
        for top in tops {
            let under = self.subtree(&top.id);
            let members: Vec<&RosterRow> =
                people.iter().copied().filter(|p| p.folders.iter().any(|f| under.contains(f))).collect();
            placed.extend(members.iter().map(|p| p.id.as_str()));
            group(&mut out, top.name.clone(), members);
        }
        let rest: Vec<&RosterRow> = people.iter().copied().filter(|p| !placed.contains(p.id.as_str())).collect();
        group(&mut out, "Others".into(), rest);
        out
    }

    /// A folder and the folders under it.
    pub(super) fn subtree(&self, folder: &str) -> Vec<String> {
        let mut out = vec![folder.to_string()];
        let mut i = 0;
        while i < out.len() {
            let parent = out[i].clone();
            out.extend(self.folders.iter().filter(|f| f.parent.as_deref() == Some(&parent)).map(|f| f.id.clone()));
            i += 1;
        }
        out
    }

    /// Makes the chat list `folder`'s window, and the roster its people.
    pub(super) fn pick_folder(&mut self, folder: &str) {
        if self.folder == folder {
            return;
        }
        self.folder = folder.to_string();
        if let Some(open) = self.open.as_ref().and_then(|o| self.chats.get(o)).cloned() {
            self.outside = Some(open);
        }
        self.chats = Chats::new(folder, Place::top(super::LIST_ROWS));
        self.rows_laid.clear();
        self.list_top = 0;
        self.jawd.watch_chats(folder, Place::top(super::LIST_ROWS));
        self.roster.clear();
        self.jawd.watch_roster(folder);
        self.dirty = true;
    }

    /// Alt-Shift-Up and Alt-Shift-Down: the folder before or after, in the tree's order with every
    /// folder open, picked as Enter picks it, the folders it's in opened so the sidebar shows it.
    /// At either end it stays, as Alt-Up and Alt-Down do.
    pub(super) fn step_folder(&mut self, by: isize) {
        let order: Vec<String> = self
            .tree_open(|_| true)
            .into_iter()
            .filter_map(|t| match t {
                Tree::Folder { id, .. } => Some(id),
                Tree::Gap => None,
            })
            .collect();
        let Some(last) = order.len().checked_sub(1) else { return };
        let at = order.iter().position(|id| *id == self.folder).unwrap_or(0) as isize;
        let id = order[(at + by).clamp(0, last as isize) as usize].clone();
        let mut parent = self.folder(&id).and_then(|f| f.parent.clone());
        while let Some(p) = parent {
            parent = self.folder(&p).and_then(|f| f.parent.clone());
            self.sidebar.open.insert(p);
        }
        self.pick_folder(&id);
        self.sidebar.at = self.tree_row(&id).unwrap_or(self.sidebar.at);
    }

    pub(super) fn on_folders(&mut self, upsert: Vec<FolderRow>, remove: Vec<String>, whole: bool) {
        if whole {
            let mut rows = upsert;
            for b in built_in() {
                if !rows.iter().any(|r| r.id == b.id) {
                    rows.push(b);
                }
            }
            self.folders = rows;
        } else {
            for row in upsert {
                match self.folders.iter_mut().find(|f| f.id == row.id) {
                    Some(f) => *f = row,
                    None => self.folders.push(row),
                }
            }
            self.folders.retain(|f| !remove.contains(&f.id));
        }
        if self.folder(&self.folder.clone()).is_none() {
            self.pick_folder("all");
        }
    }

    pub(super) fn on_roster(&mut self, folder: String, upsert: Vec<RosterRow>, remove: Vec<String>, whole: bool) {
        if folder != self.folder {
            return;
        }
        if whole {
            self.roster = upsert;
            return;
        }
        for row in upsert {
            match self.roster.iter_mut().find(|r| r.id == row.id) {
                Some(r) => *r = row,
                None => self.roster.push(row),
            }
        }
        self.roster.retain(|r| !remove.contains(&r.id));
    }

    /// Keys while the sidebar has them.
    pub(super) fn on_sidebar_key(&mut self, k: KeyEvent) {
        let Some(part) = self.sidebar.keys else { return };
        match (part, k.code) {
            (_, KeyCode::Esc) => self.sidebar.keys = None,
            (_, KeyCode::Tab | KeyCode::BackTab) => {
                self.sidebar.keys = Some(if part == Part::Folders { Part::Roster } else { Part::Folders });
            }
            (Part::Folders, KeyCode::Up | KeyCode::Char('k')) => self.tree_move(-1),
            (Part::Folders, KeyCode::Down | KeyCode::Char('j')) => self.tree_move(1),
            (Part::Folders, KeyCode::Right | KeyCode::Left) => {
                if let Some(Tree::Folder { id, children: true, .. }) = self.tree().get(self.sidebar.at).cloned() {
                    if k.code == KeyCode::Right {
                        self.sidebar.open.insert(id);
                    } else {
                        self.sidebar.open.remove(&id);
                    }
                }
            }
            (Part::Folders, KeyCode::Enter) => {
                if let Some(Tree::Folder { id, .. }) = self.tree().get(self.sidebar.at).cloned() {
                    self.show_folder(&id);
                }
            }
            (Part::Roster, KeyCode::Up | KeyCode::Char('k')) => self.roster_move(-1),
            (Part::Roster, KeyCode::Down | KeyCode::Char('j')) => self.roster_move(1),
            (Part::Roster, KeyCode::Enter) => {
                if let Some(p) = self.roster_selected() {
                    self.open_person(&p);
                }
            }
            (Part::Roster, KeyCode::Char('m')) => {
                if let Some(p) = self.roster_selected() {
                    self.palette_file(&p.id, &p.name, p.folders.clone());
                }
            }
            _ => {}
        }
    }

    /// Enter on a folder, or a click on it: its chats, and below 120 columns the sidebar closes.
    fn show_folder(&mut self, id: &str) {
        self.pick_folder(id);
        if !self.wide() {
            self.sidebar.keys = None;
        }
    }

    /// Enter on someone in the roster, or a click on them: their chat opens, or one with them
    /// begins, and the keys leave the sidebar.
    fn open_person(&mut self, p: &RosterRow) {
        self.sidebar.keys = None;
        match &p.chat {
            Some(chat) => {
                self.open_chat(chat);
                self.jawd.opened(chat);
            }
            None => {
                self.starting = Some(p.id.clone());
                self.jawd.start_chat(&p.id);
                self.say(format!("a chat with {} begins", p.name));
            }
        }
    }

    /// The mouse over the sidebar, wherever it's drawn: a click on a folder shows its chats, on its
    /// mark opens or closes it, and on someone opens their chat, each as Enter does there, without
    /// giving the sidebar the keys. Neither the tree nor the roster scrolls, so the wheel moves
    /// nothing, not even the list under the sidebar drawn over it.
    pub(super) fn on_sidebar_mouse(&mut self, m: MouseEvent) {
        if m.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        self.dirty = true;
        self.overlay = None;
        let (at, y) = (Position::new(m.column, m.row), m.row);
        let on_row = |rows: &[(u16, String)]| rows.iter().find(|(row, _)| *row == y).map(|(_, id)| id.clone());
        if let Some(id) = self.areas.folder_marks.iter().find(|(mark, _)| mark.contains(at)).map(|(_, id)| id.clone()) {
            self.sidebar.at = self.tree_row(&id).unwrap_or(self.sidebar.at);
            if !self.sidebar.open.remove(&id) {
                self.sidebar.open.insert(id);
            }
        } else if let Some(id) = on_row(&self.areas.folder_rows) {
            self.sidebar.at = self.tree_row(&id).unwrap_or(self.sidebar.at);
            self.show_folder(&id);
        } else if let Some(id) = on_row(&self.areas.people_rows) {
            let people = self.roster_people();
            if let Some(i) = people.iter().position(|p| p.id == id) {
                self.sidebar.roster_at = i;
                self.open_person(&people[i]);
            }
        }
    }

    fn tree_move(&mut self, by: isize) {
        let tree = self.tree();
        let mut at = self.sidebar.at as isize;
        loop {
            at += by;
            match tree.get(at.max(0) as usize) {
                _ if at < 0 => return,
                None => return,
                Some(Tree::Gap) => continue,
                Some(Tree::Folder { .. }) => break,
            }
        }
        self.sidebar.at = at as usize;
    }

    /// The people the roster's keys can land on.
    fn roster_people(&self) -> Vec<RosterRow> {
        self.roster_lines()
            .into_iter()
            .filter_map(|l| match l {
                Roster::Person(p) => Some(*p),
                _ => None,
            })
            .collect()
    }

    fn roster_move(&mut self, by: isize) {
        let n = self.roster_people().len();
        if n > 0 {
            self.sidebar.roster_at = (self.sidebar.roster_at as isize + by).clamp(0, n as isize - 1) as usize;
        }
    }

    fn roster_selected(&self) -> Option<RosterRow> {
        let people = self.roster_people();
        people.get(self.sidebar.roster_at.min(people.len().saturating_sub(1))).cloned()
    }

    /// The footer's keys while the sidebar has them.
    pub(super) fn sidebar_hints(&self) -> Vec<(&'static str, &'static str, bool)> {
        let fold = if self.wide() { "folds it" } else { "closes it" };
        match self.sidebar.keys {
            Some(Part::Roster) => vec![
                ("↑↓", "move", true),
                ("⏎", "opens their chat", true),
                ("m", "files them", true),
                ("tab", "folders", true),
                ("esc", "back", true),
            ],
            _ => vec![
                ("↑↓", "move", true),
                ("⏎", "shows its chats", true),
                ("→←", "opens, closes", true),
                ("tab", "the roster", true),
                ("esc", "back", true),
                ("ctrl-b", fold, true),
            ],
        }
    }

    /// The sidebar into `area`: the tree, then the roster of the folder picked.
    pub(super) fn draw_sidebar(&mut self, f: &mut Frame, area: Rect) {
        let t = self.theme;
        let focused = self.sidebar.keys == Some(Part::Folders) && self.overlay.is_none();
        f.render_widget(Clear, area);
        let block = self
            .panel(if focused { t.focus } else { t.gutter })
            .title(self.title("folders", vec![]))
            .padding(ratatui::widgets::Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);
        self.areas.sidebar = area;
        let w = inner.width as usize;
        let tree = self.tree();
        let mut lines: Vec<(Line<'static>, bool, Option<Hit>)> = vec![];
        for (i, row) in tree.iter().enumerate() {
            let Tree::Folder { id, depth, children } = row else {
                lines.push((Line::default(), false, None));
                continue;
            };
            let Some(folder) = self.folder(id) else { continue };
            let picked = *id == self.folder;
            let mark = match (folder.smart.is_some(), children, self.sidebar.open.contains(id)) {
                (true, _, _) => "◇ ",
                (false, true, true) => "▾ ",
                (false, true, false) => "▸ ",
                _ => "  ",
            };
            let mut right = vec![];
            if let Some(slot) = folder.favourite {
                right.push(Span::styled(format!("{slot} "), Style::new().fg(t.gutter)));
            }
            if folder.mentions > 0 {
                right.push(Span::styled(
                    format!("@{} ", folder.mentions),
                    Style::new().fg(t.mention).add_modifier(Modifier::BOLD),
                ));
            }
            if folder.unread > 0 && folder.id != "archived" {
                let style = if picked {
                    Style::new().fg(t.unread).add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(t.muted)
                };
                right.push(Span::styled(folder.unread.to_string(), style));
            }
            let right_w: usize = right.iter().map(|s| s.width()).sum();
            let lead = format!("{}{mark}", "  ".repeat(*depth));
            let name_w = w.saturating_sub(lead.chars().count() + right_w + 1);
            let name_style =
                if picked { Style::new().fg(t.fg).add_modifier(Modifier::BOLD) } else { Style::new().fg(t.text) };
            let mut spans = vec![
                Span::styled(lead.clone(), Style::new().fg(t.comment)),
                Span::styled(fmt::fit(&folder.name, name_w), name_style),
            ];
            let used: usize = spans.iter().map(|s| s.width()).sum();
            spans.push(Span::raw(" ".repeat(w.saturating_sub(used + right_w))));
            spans.extend(right);
            let band = if self.sidebar.keys == Some(Part::Folders) { i == self.sidebar.at } else { picked };
            let mark = children.then_some(2 * *depth as u16);
            lines.push((Line::from(spans), band, Some(Hit::Folder(id.clone(), mark))));
        }
        let roster_title = format!("roster · {}", self.folder_name(&self.folder));
        lines.push((Line::default(), false, None));
        lines.push((Line::from(Span::styled(fmt::fit(&roster_title, w), Style::new().fg(t.comment))), false, None));
        let now = self.now();
        let selected = self.roster_selected().map(|p| p.id);
        let roster_keys = self.sidebar.keys == Some(Part::Roster);
        for line in self.roster_lines() {
            let (spans, band, hit) = match line {
                Roster::Heading { name, online, of } => {
                    let said = format!("{online} of {of}");
                    let mut spans =
                        vec![Span::styled(fmt::fit(&name, w.saturating_sub(said.len() + 1)), Style::new().fg(t.muted))];
                    let used: usize = spans.iter().map(|s| s.width()).sum();
                    spans.push(Span::raw(" ".repeat(w.saturating_sub(used + said.len()))));
                    spans.push(Span::styled(said, Style::new().fg(t.gutter)));
                    (spans, false, None)
                }
                Roster::Offline(n) => {
                    (vec![Span::styled(format!("  {n} offline"), Style::new().fg(t.gutter))], false, None)
                }
                Roster::Person(p) => {
                    let faint = !p.presence.known;
                    let (dot, said, said_style) = match p.presence.state {
                        PresenceState::Typing => ("●", "typing…".to_string(), Style::new().fg(t.online)),
                        PresenceState::Recording => ("●", "recording…".to_string(), Style::new().fg(t.online)),
                        PresenceState::Online => ("●", "online".to_string(), Style::new().fg(t.comment)),
                        PresenceState::Seen => (
                            "○",
                            p.presence.at.map_or(String::new(), |at| fmt::ago(at, now, self.tz)),
                            Style::new().fg(t.comment),
                        ),
                        PresenceState::Hidden => (" ", String::new(), Style::new().fg(t.comment)),
                    };
                    let dot_style = if p.presence.around() && !faint {
                        Style::new().fg(t.online)
                    } else {
                        Style::new().fg(t.comment)
                    };
                    let name_style = Style::new().fg(if faint { t.comment } else { t.text });
                    let said_style = if faint { Style::new().fg(t.gutter) } else { said_style };
                    let name_w = w.saturating_sub(2 + said.chars().count() + 1);
                    let mut spans = vec![
                        Span::styled(dot, dot_style),
                        Span::raw(" "),
                        Span::styled(fmt::fit(&p.name, name_w), name_style),
                    ];
                    let used: usize = spans.iter().map(|s| s.width()).sum();
                    spans.push(Span::raw(" ".repeat(w.saturating_sub(used + said.chars().count()))));
                    spans.push(Span::styled(said, said_style));
                    let band = roster_keys && selected.as_deref() == Some(p.id.as_str());
                    (spans, band, Some(Hit::Person(p.id.clone())))
                }
            };
            lines.push((Line::from(spans), band, hit));
        }
        let buf = f.buffer_mut();
        for (i, (line, band, hit)) in lines.into_iter().take(inner.height as usize).enumerate() {
            let y = inner.y + i as u16;
            buf.set_line(inner.x, y, &line, inner.width);
            if band {
                buf.set_style(Rect::new(area.x + 1, y, area.width - 2, 1), Style::new().bg(t.selection));
            }
            match hit {
                Some(Hit::Folder(id, mark)) => {
                    if let Some(x) = mark {
                        self.areas.folder_marks.push((Rect::new(inner.x + x, y, 2, 1), id.clone()));
                    }
                    self.areas.folder_rows.push((y, id));
                }
                Some(Hit::Person(id)) => self.areas.people_rows.push((y, id)),
                None => {}
            }
        }
    }

    /// The chat list's rows in the order they're drawn: a community's groups hidden under it
    /// until it's opened, then right after it.
    pub(super) fn list_order(&self) -> Vec<String> {
        let rows: Vec<&crate::data::Chat> = self.chats.rows().collect();
        let held: HashSet<&str> = rows.iter().map(|c| c.id.as_str()).collect();
        let mut out = vec![];
        for c in &rows {
            if c.community.as_deref().is_some_and(|p| held.contains(p)) {
                continue;
            }
            out.push(c.id.clone());
            if c.kind == crate::data::ChatKind::Community && self.communities_open.contains(&c.id) {
                let mut groups: Vec<&&crate::data::Chat> =
                    rows.iter().filter(|g| g.community.as_deref() == Some(&c.id)).collect();
                groups.sort_by_key(|g| g.kind != crate::data::ChatKind::Announcement);
                out.extend(groups.into_iter().map(|g| g.id.clone()));
            }
        }
        out
    }

    /// Opens a community to show its groups, or closes it; says whether `id` was one.
    pub(super) fn toggle_community(&mut self, id: &str) -> bool {
        if self.chats.get(id).is_none_or(|c| c.kind != crate::data::ChatKind::Community) {
            return false;
        }
        if !self.communities_open.remove(id) {
            self.communities_open.insert(id.to_string());
        }
        self.selected = Some(id.to_string());
        self.dirty = true;
        true
    }

    /// The selected chat or person to file: the open chat, or the one picked in the list.
    pub(super) fn file_this(&mut self) {
        let id = if self.narrow() && self.picking { self.selected.clone() } else { self.open.clone() };
        let Some(chat) = id.and_then(|id| self.chat_known(&id).cloned()) else {
            self.say("open a chat to file it");
            return;
        };
        self.focus = Focus::Typing;
        self.palette_file(&chat.id, &chat.name, chat.folders.clone());
    }
}
