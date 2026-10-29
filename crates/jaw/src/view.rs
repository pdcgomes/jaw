//! The terminal interface's state: what jawd has sent, which chat is open, where the conversation
//! is anchored, the message box, and what's selected. Updates apply here, keys drive it
//! (`keys`), and it draws itself (`draw`).

mod actions;
mod composer;
mod draw;
mod focus_mode;
mod keys;
mod library;
mod palette;
mod panes;
mod quick_look;
mod settings;
mod sidebar;
mod status_line;
mod voice;

use crate::composer::Composer;
use crate::data::{
    Chat, Jawd, Message, Place, Presence, PresenceState, Typing, Typist, TypistState, Typists, Update, key,
};
use crate::fmt::{self, MINUTE};
use crate::graphics::{Images, Protocol};
use crate::layout::{self, Shape};
use crate::theme::Theme;
use chrono::FixedOffset;
use jaw_client::{Chats, Messages};
use jaw_proto::settings::{Settings, SettingsSet};
use jaw_proto::{Connection, ConnectionState, LinkStep};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Below this width the chat list folds, and takes the screen until a chat is picked.
pub const NARROW: u16 = 90;
/// The chat list's width beside the conversation.
pub const LIST: u16 = 34;
/// How long a notice stays in the footer.
const NOTICE: i64 = 4_000;
/// How many of the messages jawd has taken are remembered by key.
const SENT_KEPT: usize = 256;
/// The chat list keeps its order this long after the last move through it.
const HOLD: i64 = 1_500;
/// A draft goes to jawd once typing has paused this long.
const DRAFT: i64 = 1_500;
/// Typing is said again this often while it goes on.
const TYPING: i64 = 5_000;
/// Read receipts go at most this often while messages keep arriving.
const READ: i64 = 1_000;
/// The Mimic chomps for this long when an empty chat's first message comes.
const CHOMP: i64 = 1_200;
/// Chats the list's window keeps.
const LIST_ROWS: u32 = 100;
/// Messages kept each side of the anchor in the open chat's window.
pub const WINDOW: usize = 100;
/// Ask for the window to move once the screen is this close to its edge.
const EDGE: usize = 20;
/// How long the first frame waits for jawd's hello, input or not, so it's drawn in the theme jawd
/// keeps rather than in the default first.
const FIRST_FRAME_WAIT: Duration = Duration::from_millis(250);

/// How many sender colours jawd chooses among for people's messages.
const SENDER_COLOURS: i64 = 8;

/// The colour jawd gives the messages of the person `id` names (`person:` and a number): their
/// number among its sender colours.
fn colour_of(id: &str) -> u8 {
    id.strip_prefix("person:").and_then(|n| n.parse::<i64>().ok()).map_or(0, |n| n.rem_euclid(SENDER_COLOURS) as u8)
}

/// Whether jawd answers.
#[derive(Clone, Debug, PartialEq)]
pub enum Reach {
    /// No hello yet.
    Waiting,
    Starting {
        failed: Option<String>,
    },
    Unreachable {
        why: String,
        retry_at: i64,
        since: i64,
    },
    Up,
}

/// Where the keys go: the message box, or a selected message.
#[derive(Clone, Debug, PartialEq)]
pub enum Focus {
    Typing,
    Message(String),
}

/// Where the conversation is: following the newest at the bottom, or anchored on a message at a
/// row of the screen, so what arrives above or below doesn't move it.
#[derive(Clone, Debug, PartialEq)]
pub enum Scroll {
    Bottom,
    Anchor { id: String, sort: String, top: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    Keys,
    /// The selected message's info: when it was sent, and each recipient's receipts.
    Info,
}

/// What the loop does for the view outside the terminal's frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Puts text on the clipboard, through the terminal.
    Copy(String),
    /// Opens a link in the browser, or a file in the app the system has for it.
    Open(String),
    /// Bytes for the terminal beside the frame: pictures sent, deleted and drawn.
    Write(Vec<u8>),
}

/// A message as last drawn: its block's first row on screen and how tall it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub top: i32,
    pub height: i32,
}

struct Laid {
    v: u64,
    shape: Shape,
    lines: Rc<layout::Laidout>,
}

/// A chat's rows in the list as last drawn, and what they depended on: its version, whether it
/// was open, the width, the minute its time was written in, whether its community was open, and
/// who was typing in it.
struct ChatRows {
    key: (u64, bool, usize, i64, bool, u64),
    rows: Rc<(Line<'static>, Line<'static>)>,
}

/// Where things were last drawn, for the mouse and for keys that depend on the layout.
#[derive(Debug, Default)]
struct Areas {
    list: Rect,
    /// Each chat row's first line and its chat.
    chat_rows: Vec<(u16, String)>,
    messages: Rect,
    /// Each message's rows on screen.
    message_rows: Vec<(u16, u16, String)>,
    composer: Rect,
    composer_width: usize,
    /// The sidebar, each folder's row and the open or closed mark of those with folders in them,
    /// each with its folder, and each person's row in the roster with theirs.
    sidebar: Rect,
    folder_rows: Vec<(u16, String)>,
    folder_marks: Vec<(Rect, String)>,
    people_rows: Vec<(u16, String)>,
    width: u16,
    height: u16,
}

pub struct View {
    pub theme: Theme,
    pub no_color: bool,
    /// A clock that stands still, for frames that must come out the same.
    pub clock: Option<i64>,
    pub tz: FixedOffset,
    /// jawd's settings, as hello, a push or an answer last said; what was last asked of it while
    /// its answer is on the way; and how many changes asked are still to be answered.
    settings: Settings,
    settings_asked: Option<Settings>,
    settings_owed: usize,
    /// When the view was made, so the first frame waits for jawd at most `FIRST_FRAME_WAIT`.
    born: Instant,
    jawd: Box<dyn Jawd>,

    reach: Reach,
    linked: Option<bool>,
    me: Vec<String>,
    connection: Connection,
    link: Option<LinkStep>,
    link_asked: bool,
    /// Once jawd has said the account was logged out, why, if it said: kept until a connection
    /// says the account is linked again, whatever the connection does meanwhile.
    logged_out: Option<Option<String>>,
    /// The number being typed for a phone code.
    phone: Option<String>,
    /// The chat and oldest message jaw last asked the phone for older messages before.
    asked_older: Option<(String, String)>,

    chats: Chats<Chat>,
    messages: Option<Messages<Message>>,
    /// Who's typing in the chats the windows show, as jawd last said.
    typing: Typists,
    /// The presence of the person of each one-to-one chat a conversation shows, as jawd last said.
    presence: BTreeMap<String, Presence>,
    /// The open chat, kept by ID however the list moves.
    open: Option<String>,
    /// The chat the list has selected, which is the open one except while picking on a narrow
    /// screen.
    selected: Option<String>,
    list_top: usize,
    list_rows: usize,
    /// Below 90 columns: whether the chat list has the screen.
    picking: bool,

    focus: Focus,
    scroll: Scroll,
    /// The newest message seen on screen in the open chat, by `sort`.
    seen: Option<String>,
    /// Unread when the chat was opened, to anchor its window on the first of them.
    unread_at_open: u32,
    placed: HashMap<String, Placed>,
    laid: HashMap<String, Laid>,
    /// Each chat's rows in the list, kept per version, whether it's open, width and minute.
    rows_laid: HashMap<String, ChatRows>,
    unfolded: HashSet<String>,
    /// Where the open chat's window was last asked to be, so it isn't asked again each frame.
    asked_place: Option<Place>,
    /// Whether the windows have been watched, which a connection's later hellos don't do again.
    watching: bool,

    composer: Composer,
    draft_sent: String,
    draft_due: Option<i64>,
    typing_at: i64,
    read_upto: Option<String>,
    read_at: i64,
    read_due: Option<i64>,

    overlay: Option<Overlay>,
    /// Photos, stickers and posters: what the terminal draws them with, and those ready.
    pub images: Images,
    /// Messages none of whose previews could be read, laid out as their summaries.
    pictureless: HashSet<String>,
    /// The size the last frame was drawn at, for drawing every picture again after a resize.
    drawn_size: (u16, u16),
    /// The library, while it's open, and the width its items were last laid out at.
    library: Option<library::Library>,
    library_width: u16,
    /// The folder the chat list and roster show, the tree, the roster, and the sidebar's state.
    pub folder: String,
    folders: Vec<crate::data::FolderRow>,
    roster: Vec<crate::data::RosterRow>,
    /// What Alt-1 to Alt-9 go to.
    favourites: Vec<crate::data::Favourite>,
    sidebar: sidebar::Sidebar,
    /// Communities opened in the chat list to show their groups.
    communities_open: HashSet<String>,
    /// Messages and actions whose answer a dropped connection lost, by chat and key, until jawd
    /// answers for them again.
    unsure: Vec<(String, String)>,
    /// The key of the message being composed in each chat, made once for it and sent with it.
    keys: HashMap<String, String>,
    /// The newest messages jawd has taken, by key, with their IDs.
    sent: std::collections::VecDeque<(String, String)>,
    /// What the box replies to or edits, and the names taken into it from the completion.
    replying: Option<actions::Replying>,
    editing: Option<actions::Editing>,
    taken: Vec<actions::Taken>,
    /// An `@` whose names Esc put away, and which of the names offered is selected.
    dismissed: Option<usize>,
    naming_selected: usize,
    /// Each chat's members as jawd said them, and the chats they've been asked for.
    members: HashMap<String, Vec<crate::data::Member>>,
    members_asked: HashSet<String>,
    /// A card being made of someone whose chat jawd keeps under their LID, waiting for that chat's
    /// members to say their number.
    carding: Option<actions::Carding>,
    /// The message whose info the sheet shows, and jawd's answer once it comes.
    info: Option<(String, Option<Result<crate::data::MessageInfo, String>>)>,
    /// The voice message playing or paused, or the recording being listened to, as jawd last
    /// said or a key just asked.
    pub playing: Option<crate::data::Playback>,
    /// What a key last asked to play, and when, so pushes about the one before don't undo it.
    voice_asked: Option<(Option<String>, i64)>,
    /// The speed voice messages play at.
    speed: f64,
    /// The voice message being recorded in the box, or stopped to listen to.
    recorder: Option<voice::Recorder>,
    /// Why the last message wasn't sent, said on the box it went back to until it's typed in.
    unsent: Option<String>,
    /// Ctrl-K's palette, while it's open.
    palette: Option<palette::Palette>,
    /// The newest request number the interface has given a `find`.
    asked: u64,
    /// The open chat's row when it isn't in the chat list, as an archived one found by the palette.
    outside: Option<crate::data::Chat>,
    /// The person a chat is being started with, to open it when it comes.
    starting: Option<String>,
    /// A message to anchor on and select once it's in the window.
    jump: Option<String>,
    notice: Option<(String, i64)>,
    /// The jawd that last answered, while it's from another build than this jaw.
    other_jawd: Option<crate::data::OtherJawd>,
    hold_until: Option<i64>,
    chomp_until: Option<i64>,
    last_click: Option<(i64, String)>,
    /// The creature, decoded once.
    mimic: crate::art::Sprite,
    areas: Areas,
    /// The size snapshots draw at.
    size: Option<(u16, u16)>,
    /// The second the last frame was drawn in, so clocks and countdowns redraw as it passes.
    drawn_at: i64,
    /// The experimental switches, each off unless turned on; whether the open chat has the screen
    /// to itself, and the quick look while it's open.
    pub experiments: crate::experiments::Experiments,
    focus_mode: bool,
    quick_look: Option<quick_look::QuickLook>,
    /// The settings panel, while it's open.
    settings_panel: Option<settings::Panel>,
    /// The panes once the conversation is split, each other than the one with the keys holding
    /// its chat's state.
    panes: panes::Panes,
    /// Their layout, as jawd keeps it for jaw.
    layout_kept: panes::LayoutKept,
    effects: Vec<Effect>,
    /// Something on screen changed since the last frame.
    pub dirty: bool,
    /// What changed is something the screen was waiting for, which the loop draws at once.
    pub awaited: bool,
    pub quit: bool,
}

impl View {
    pub fn new(jawd: Box<dyn Jawd>) -> View {
        let truecolor = crate::theme::truecolor();
        View {
            theme: Theme::with(truecolor),
            no_color: crate::theme::no_color(),
            clock: None,
            tz: *chrono::Local::now().offset(),
            settings: Settings::default(),
            settings_asked: None,
            settings_owed: 0,
            born: Instant::now(),
            jawd,
            reach: Reach::Waiting,
            linked: None,
            me: vec![],
            connection: Connection::new(ConnectionState::Connecting),
            link: None,
            link_asked: false,
            logged_out: None,
            phone: None,
            asked_older: None,
            chats: Chats::new("all", Place::top(LIST_ROWS)),
            messages: None,
            typing: Typists::new(),
            presence: BTreeMap::new(),
            open: None,
            selected: None,
            list_top: 0,
            list_rows: 0,
            picking: true,
            focus: Focus::Typing,
            scroll: Scroll::Bottom,
            seen: None,
            unread_at_open: 0,
            placed: HashMap::new(),
            laid: HashMap::new(),
            rows_laid: HashMap::new(),
            unfolded: HashSet::new(),
            asked_place: None,
            watching: false,
            composer: Composer::default(),
            draft_sent: String::new(),
            draft_due: None,
            typing_at: 0,
            read_upto: None,
            read_at: 0,
            read_due: None,
            overlay: None,
            images: Images::here(Protocol::None, (8, 16)),
            pictureless: HashSet::new(),
            drawn_size: (0, 0),
            library: None,
            library_width: 76,
            folder: "all".into(),
            folders: sidebar::built_in(),
            roster: vec![],
            favourites: vec![],
            sidebar: sidebar::Sidebar::default(),
            communities_open: HashSet::new(),
            unsure: vec![],
            keys: HashMap::new(),
            sent: std::collections::VecDeque::new(),
            replying: None,
            editing: None,
            taken: vec![],
            dismissed: None,
            naming_selected: 0,
            members: HashMap::new(),
            members_asked: HashSet::new(),
            carding: None,
            info: None,
            playing: None,
            voice_asked: None,
            speed: 1.0,
            recorder: None,
            unsent: None,
            palette: None,
            asked: 0,
            outside: None,
            starting: None,
            jump: None,
            notice: None,
            other_jawd: None,
            hold_until: None,
            chomp_until: None,
            last_click: None,
            mimic: crate::art::Sprite::mimic(),
            areas: Areas::default(),
            size: None,
            drawn_at: 0,
            experiments: crate::experiments::Experiments::at_start(),
            focus_mode: false,
            quick_look: None,
            settings_panel: None,
            panes: panes::Panes::default(),
            layout_kept: panes::LayoutKept::default(),
            effects: vec![],
            dirty: false,
            awaited: false,
            quit: false,
        }
    }

    pub fn now(&self) -> i64 {
        self.clock.unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
    }

    fn say(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), self.now() + NOTICE));
        self.dirty = true;
    }

    /// A picture a worker has made ready.
    pub fn picture(&mut self, done: crate::graphics::Done) {
        self.images.take(done);
        self.dirty = true;
    }

    /// What's to be written to the terminal beside the frame: pictures sent, deleted and drawn.
    pub fn take_writes(&mut self) -> Vec<u8> {
        self.images.take_writes()
    }

    /// What jawd sent since the last frame.
    pub fn drain_jawd(&mut self) -> Vec<Update> {
        self.jawd.drain()
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.size = Some((width, height));
    }

    pub fn size(&self) -> Option<(u16, u16)> {
        self.size
    }

    /// Whether jawd has said how things stand, or that it can't be reached, so a frame of it
    /// says something.
    pub fn answered(&self) -> bool {
        !matches!(self.reach, Reach::Waiting | Reach::Starting { failed: None })
            && (self.reach != Reach::Up || self.linked == Some(false) || self.chats.loaded)
    }

    /// The chat open in the conversation.
    pub fn opened(&self) -> Option<&str> {
        self.open.as_deref()
    }

    /// Whether the open chat's messages are on screen, or the chat list has come with no chat to
    /// open, which is when the first frame is done.
    pub fn showing_messages(&self) -> bool {
        self.messages.as_ref().is_some_and(|w| w.loaded)
            || (self.reach == Reach::Up && self.chats.loaded && self.chats.is_empty())
    }

    /// When the first frame is drawn whatever jawd says, while nothing has been drawn yet and jawd
    /// hasn't said hello or that it can't answer.
    fn first_frame_due(&self) -> Option<Instant> {
        (self.reach == Reach::Waiting && self.drawn_at == 0).then(|| self.born + FIRST_FRAME_WAIT)
    }

    /// Whether the first frame is still waiting for jawd, so nothing is drawn yet, whatever the
    /// input.
    pub fn first_frame_waits(&self) -> bool {
        self.first_frame_due().is_some_and(|due| Instant::now() < due)
    }

    /// Whether something on screen counts seconds: a QR code's or a retry's.
    fn counting(&self) -> bool {
        matches!(self.reach, Reach::Unreachable { .. })
            || (self.linking() && matches!(self.link, Some(LinkStep::Qr { .. })))
    }

    fn narrow(&self) -> bool {
        self.areas.width > 0 && self.areas.width < NARROW
    }

    pub fn apply(&mut self, update: Update) {
        self.dirty = true;
        self.awaited |= update.awaited();
        match update {
            Update::Hello { root, linked, me, connection, settings, other } => {
                self.images.set_root(&root);
                self.reach = Reach::Up;
                self.linked = Some(linked);
                self.me = me;
                self.other_jawd = other;
                if connection.state == ConnectionState::LoggedOut {
                    self.told_logged_out(connection.reason.clone());
                } else if linked {
                    self.logged_out = None;
                }
                self.settings_came(settings);
                self.connection = connection;
                // Connecting again, jawd's client watches every window again itself, where it was,
                // so nothing moves.
                if linked && !self.watching {
                    self.start_watching();
                } else if !linked && self.logged_out.is_none() {
                    self.close_settings();
                    self.start_linking();
                }
            }
            Update::Connection(c) => {
                if c.state == ConnectionState::LoggedOut {
                    self.told_logged_out(c.reason.clone());
                }
                // A jawd starting says hello before its engine has said whose account it has, so
                // a connection coming up says the account was linked all along, or is again after
                // a logout.
                let up = matches!(c.state, ConnectionState::Online | ConnectionState::CatchingUp);
                if up && (self.linked == Some(false) || self.logged_out.is_some()) {
                    self.linked = Some(true);
                    self.logged_out = None;
                    self.link = None;
                    if !self.watching {
                        self.start_watching();
                    }
                }
                self.connection = c;
            }
            Update::Settings(settings) => self.settings_came(settings),
            Update::SettingsKept(settings) => self.settings_answered(Some(settings)),
            Update::SettingsRefused(why) => {
                self.settings_answered(None);
                self.say(format!("not kept: {why}"));
            }
            Update::SettingsUnconfirmed(why) => {
                self.settings_answered(None);
                self.say(format!("not confirmed: {why}"));
            }
            Update::Link(step) => {
                if let LinkStep::Done { phone } = &step {
                    self.say(format!("Linked {phone}. jawd is fetching your history."));
                    self.linked = Some(true);
                    self.logged_out = None;
                    self.connection = Connection::new(ConnectionState::CatchingUp);
                    if !self.watching {
                        self.start_watching();
                    }
                }
                self.link = Some(step);
            }
            Update::Chats(update) => {
                let left = !update.remove.is_empty();
                if let Some(open) = self.open.as_ref().filter(|o| update.remove.contains(o)) {
                    self.outside = self.chats.get(open).cloned().or(self.outside.take());
                }
                if let Some(back) = update.upsert.iter().find(|c| self.outside.as_ref().is_some_and(|o| o.id == c.id)) {
                    self.outside = (back.archived).then(|| back.clone());
                }
                self.keep_selected_row(|v| {
                    v.chats.apply(update);
                });
                if left {
                    let chats = &self.chats;
                    self.rows_laid.retain(|id, _| chats.get(id).is_some());
                }
                if self.open.is_none() {
                    let first = self.list_order().into_iter().next();
                    if let Some(first) = first {
                        self.open_chat(&first);
                        self.picking = true;
                    }
                }
            }
            Update::Messages { chat, update } => {
                let Some(window) = self.messages.as_mut().filter(|w| w.chat == chat) else {
                    return self.parked_messages_came(&chat, update);
                };
                let first = !window.loaded;
                let was_empty = window.loaded && window.is_empty();
                if update.moved.is_some() {
                    self.asked_place = None;
                }
                let gone = window.apply(update);
                let now_empty = window.is_empty();
                for id in gone {
                    self.laid.remove(&id);
                    self.placed.remove(&id);
                }
                if was_empty && !now_empty {
                    self.chomp_until = Some(self.now() + CHOMP);
                }
                if first {
                    self.anchor_on_unread();
                }
                self.jump_if_here();
                if let Focus::Message(id) = &self.focus
                    && self.messages.as_ref().is_some_and(|w| w.get(id).is_none())
                {
                    self.focus = Focus::Typing;
                }
            }
            Update::Typing(typing) => self.typing = typing,
            Update::Presence(presence) => self.presence = presence,
            Update::QuickLook { chat, update } => self.quick_look_came(&chat, update),
            Update::Found { id, results, counts } => self.found(id, results, counts),
            Update::FindFailed { id, cancelled, why } => self.find_failed(id, cancelled, why),
            Update::ChatStarted { chat } => {
                if self.starting.take().is_some() {
                    let id = chat.id.clone();
                    if self.chats.get(&id).is_none() {
                        self.outside = Some(*chat);
                    }
                    self.open_chat(&id);
                    self.jawd.opened(&id);
                }
            }
            Update::Unsure { chat, key } => {
                if !self.unsure.iter().any(|(_, k)| *k == key) {
                    self.unsure.push((chat, key));
                }
            }
            Update::Sent { key, message } => {
                self.unsure.retain(|(_, k)| *k != key);
                self.sent.push_back((key, message));
                if self.sent.len() > SENT_KEPT {
                    self.sent.pop_front();
                }
            }
            Update::Unsent { chat, key, text, why } => {
                self.unsure.retain(|(_, k)| *k != key);
                self.give_back(chat, key, text, why);
            }
            Update::Acted { key, said } => {
                self.unsure.retain(|(_, k)| *k != key);
                if let Some(said) = said {
                    self.say(said);
                }
            }
            Update::NotStarted(why) => {
                self.starting = None;
                self.say(why);
            }
            Update::Notice(text) => self.say(text),
            Update::Playback(p) => self.on_playback(p),
            Update::Recorded(r) => self.on_recorded(r),
            Update::VoiceRefused(why) => self.voice_refused(why),
            Update::Members { chat, members } => self.on_members(chat, members),
            Update::Folders { upsert, remove, whole, favourites } => {
                self.on_folders(upsert, remove, whole);
                if let Some(favourites) = favourites {
                    self.favourites = favourites;
                }
            }
            Update::Roster { folder, upsert, remove, whole } => self.on_roster(folder, upsert, remove, whole),
            Update::FolderMade(row) => self.folder_made(*row),
            Update::Library { asked, window } => self.on_library(asked, window),
            Update::LibraryRefused(why) => self.library_refused(why),
            Update::FolderRefused { folder, why } => {
                if self.folder == folder {
                    self.folder = "all".into();
                    self.chats = Chats::new("all", Place::top(LIST_ROWS));
                    self.rows_laid.clear();
                    self.jawd.watch_roster("all");
                }
                self.say(why);
            }
            Update::Info { message, info } => self.on_info(message, info),
            Update::Layout(saved) => self.layout_came(*saved),
            Update::LayoutKept(saved) => self.layout_kept_now(*saved),
            Update::Starting { failed } => self.reach = Reach::Starting { failed },
            Update::Unreachable { why, retry_in } => {
                let retry_at = self.now() + retry_in;
                let since = match &self.reach {
                    Reach::Unreachable { since, .. } => *since,
                    _ => self.now(),
                };
                self.reach = Reach::Unreachable { why, retry_at, since };
            }
        }
    }

    /// Runs `change` on the chat list keeping the selected chat on the row of the screen it was
    /// on, so it holds its place while others move.
    fn keep_selected_row(&mut self, change: impl FnOnce(&mut View)) {
        let row = self.selected_index().map(|i| i as isize - self.list_top as isize);
        change(self);
        if let (Some(row), Some(i)) = (row, self.selected_index())
            && (0..self.list_rows.max(1) as isize).contains(&row)
        {
            self.list_top = (i as isize - row).max(0) as usize;
        }
        self.clamp_list();
    }

    fn selected_index(&self) -> Option<usize> {
        let selected = self.selected.as_deref()?;
        self.list_order().iter().position(|id| id == selected)
    }

    fn clamp_list(&mut self) {
        let most = self.list_order().len().saturating_sub(self.list_rows.max(1));
        self.list_top = self.list_top.min(most);
    }

    fn reveal_selected_chat(&mut self) {
        if let Some(i) = self.selected_index() {
            let rows = self.list_rows.max(1);
            if i < self.list_top {
                self.list_top = i;
            } else if i >= self.list_top + rows {
                self.list_top = i + 1 - rows;
            }
        }
    }

    /// The account is linked: the chat list, the folders and the roster are watched, and the
    /// panes' layout asked for.
    fn start_watching(&mut self) {
        self.watching = true;
        self.jawd.watch_chats(&self.folder, Place::top(LIST_ROWS));
        self.jawd.watch_folders();
        self.jawd.watch_roster(&self.folder);
        self.ask_layout();
    }

    fn start_linking(&mut self) {
        if !self.link_asked {
            self.link_asked = true;
            self.jawd.link_start(None);
        }
    }

    /// jawd says the account is logged out: the screen says why and offers to link again, without
    /// linking until asked. The settings panel goes with the screen it was on.
    fn told_logged_out(&mut self, reason: Option<String>) {
        self.link = None;
        self.link_asked = false;
        self.logged_out = Some(reason);
        self.close_settings();
    }

    /// The settings the screen shows: what was last asked of jawd while its answer is on the way,
    /// or what jawd last said.
    pub fn settings_shown(&self) -> &Settings {
        self.settings_asked.as_ref().unwrap_or(&self.settings)
    }

    /// jawd's settings came, in hello or a push, from this interface's change or another's: what
    /// jawd keeps now, which the screen shows unless a change asked here is still on its way.
    fn settings_came(&mut self, settings: Settings) {
        self.settings = settings;
        self.restyle();
    }

    /// jawd answered a change asked here: kept, with all its settings as they now are, or refused
    /// or unconfirmed. What was last asked stays shown until every change on its way is answered,
    /// so a first answer doesn't draw the first change over a second, and goes at once when one
    /// isn't kept or confirmed.
    fn settings_answered(&mut self, kept: Option<Settings>) {
        self.settings_owed = self.settings_owed.saturating_sub(1);
        match kept {
            Some(settings) => self.settings = settings,
            None => self.settings_asked = None,
        }
        if self.settings_owed == 0 {
            self.settings_asked = None;
        }
        self.restyle();
    }

    /// Asks jawd to keep `change` for the terminal, showing it meanwhile.
    fn ask_settings(&mut self, change: SettingsSet) {
        let mut asked = self.settings_shown().clone();
        if let Some(theme) = &change.theme {
            asked.theme = theme.clone();
        }
        if let Some(messages) = change.messages {
            asked.messages = messages;
        }
        self.settings_asked = Some(asked);
        self.settings_owed += 1;
        self.jawd.set_settings(&change);
        self.restyle();
    }

    /// Draws the screen in the theme it shows, or the one the settings panel previews; a theme
    /// drawn differently lays every message and chat row out again, in its colours, and draws
    /// every picture again on its ground.
    fn restyle(&mut self) {
        let theme = Theme::named(self.previewed().unwrap_or(&self.settings_shown().theme), self.theme.truecolor);
        if theme != self.theme {
            self.theme = theme;
            self.laid.clear();
            self.rows_laid.clear();
            self.restyle_panes();
            self.images.new_ground();
        }
        self.dirty = true;
    }

    /// Opens a chat: its draft into the box, its window from the newest, typing in the box.
    pub fn open_chat(&mut self, id: &str) {
        self.open_chat_at(id, None);
    }

    /// Opens a chat at a message, or at its newest.
    pub fn open_chat_at(&mut self, id: &str, at: Option<String>) {
        if self.open_in_its_pane(id, at.as_deref()) {
            return;
        }
        if self.open.as_deref() == Some(id) {
            self.selected = Some(id.to_string());
            self.picking = false;
            return;
        }
        self.leave_chat();
        let draft = self.chats.get(id).and_then(|c| c.draft.clone()).unwrap_or_default();
        self.unread_at_open = self.chats.get(id).map_or(0, |c| c.unread);
        self.composer.set(&draft);
        self.draft_sent = draft;
        self.open = Some(id.to_string());
        self.selected = Some(id.to_string());
        self.messages = Some(Messages::new(id, Place::newest(WINDOW as u32)));
        self.focus = Focus::Typing;
        self.scroll = Scroll::Bottom;
        self.seen = None;
        self.placed.clear();
        self.laid.clear();
        self.unfolded.clear();
        self.read_upto = None;
        self.chomp_until = None;
        self.picking = false;
        if self.outside.as_ref().is_some_and(|o| o.id != id) {
            self.outside = None;
        }
        self.asked_place = None;
        self.watch(id, at.map_or_else(View::newest, |m| View::around(&m)));
        self.reveal_selected_chat();
        self.dirty = true;
    }

    /// The open chat's draft and typing, settled before another opens.
    fn leave_chat(&mut self) {
        let Some(chat) = self.open.clone() else { return };
        self.drop_actions();
        self.save_draft(&chat);
        if self.typing_at > 0 {
            self.jawd.typing(&chat, Typing::Paused);
            self.typing_at = 0;
        }
        if let Some(due) = self.read_due.take()
            && due > 0
        {
            self.send_read();
        }
    }

    fn save_draft(&mut self, chat: &str) {
        self.draft_due = None;
        if self.composer.text() != self.draft_sent {
            self.draft_sent = self.composer.text().to_string();
            self.jawd.draft(chat, &self.draft_sent);
        }
    }

    /// Asks for the open chat's window somewhere, unless that's where it was last asked for.
    fn watch(&mut self, chat: &str, place: Place) {
        if self.asked_place.as_ref() == Some(&place) || self.panes.drawing {
            return;
        }
        self.asked_place = Some(place.clone());
        self.jawd.watch_messages(chat, place);
    }

    /// The open chat's window around a message.
    fn around(id: &str) -> Place {
        Place::around(id, WINDOW as u32, WINDOW as u32)
    }

    fn newest() -> Place {
        Place::newest(WINDOW as u32)
    }

    /// The message a jump is for, once it's in the window: anchored a third of the way down,
    /// and selected.
    fn jump_if_here(&mut self) {
        let Some(id) = self.jump.clone() else { return };
        let Some(m) = self.messages.as_ref().and_then(|w| w.get(&id)) else { return };
        let top = self.areas_messages_height() / 3;
        self.scroll = Scroll::Anchor { id: id.clone(), sort: m.sort.clone(), top };
        self.focus = Focus::Message(id);
        self.jump = None;
    }

    fn areas_messages_height(&self) -> i32 {
        self.areas.messages.height.max(12) as i32
    }

    /// A chat opened with unread messages starts at the first of them, the rest counted below.
    fn anchor_on_unread(&mut self) {
        let n = self.unread_at_open as usize;
        let Some(w) = self.messages.as_ref() else { return };
        if n == 0 {
            return;
        }
        if let Some(first) = w.rows().rev().filter(|m| !m.mine).nth(n - 1) {
            self.seen = w.before(&first.sort).map(|m| m.sort.clone());
            self.scroll = Scroll::Anchor { id: first.id.clone(), sort: first.sort.clone(), top: 1 };
        }
    }

    /// The box's text changed: typing is said, and the draft saved once it pauses.
    fn typed(&mut self) {
        let Some(chat) = self.open.clone() else { return };
        let now = self.now();
        if self.composer.is_empty() {
            if self.typing_at > 0 {
                self.jawd.typing(&chat, Typing::Paused);
                self.typing_at = 0;
            }
        } else if now - self.typing_at >= TYPING {
            self.jawd.typing(&chat, Typing::Composing);
            self.typing_at = now;
        }
        if self.editing.is_none() {
            self.draft_due = Some(now + DRAFT);
            if !self.composer.is_empty() {
                self.keys.entry(chat).or_insert_with(key);
            }
        }
        self.unsent = None;
    }

    /// A message jawd didn't take goes back where it was composed, with its key, so its text is
    /// never lost: into the box if its chat is open, or its chat's draft, ahead of anything typed
    /// since. One jawd has taken stays where it is, in the window.
    fn give_back(&mut self, chat: String, key: String, text: String, why: String) {
        if self.sent.iter().any(|(k, _)| *k == key) || self.give_back_in_pane(&chat, &key, &text, &why) {
            return;
        }
        let before = |typed: &str| if typed.is_empty() { text.clone() } else { format!("{text}\n{typed}") };
        if self.open.as_deref() == Some(chat.as_str()) {
            let was_empty = match self.editing.as_mut() {
                Some(e) => {
                    let was_empty = e.draft.is_empty();
                    e.draft = before(&e.draft);
                    was_empty
                }
                None => {
                    let was_empty = self.composer.is_empty();
                    self.composer.set(&before(self.composer.text()));
                    self.draft_due = Some(self.now() + DRAFT);
                    was_empty
                }
            };
            if was_empty {
                self.keys.insert(chat, key);
            }
            self.unsent = Some(why);
        } else {
            let draft = self.chat_known(&chat).and_then(|c| c.draft.clone()).unwrap_or_default();
            if draft.is_empty() {
                self.keys.insert(chat.clone(), key);
            }
            self.jawd.draft(&chat, &before(&draft));
            let name = self.chat_known(&chat).map_or_else(|| chat.clone(), |c| c.name.clone());
            self.say(format!("not sent to {name}, back in its box: {why}"));
        }
    }

    fn send(&mut self) {
        let Some(chat) = self.open.clone() else { return };
        let text = self.composer.text().trim_end().to_string();
        if text.trim().is_empty() {
            return;
        }
        if let Some(words) = text.strip_prefix("/search").filter(|w| w.is_empty() || w.starts_with(' ')) {
            self.composer.take();
            let name = self.chat_known(&chat).map_or_else(|| chat.clone(), |c| c.name.clone());
            self.palette_search(Some((chat, name)), words.trim());
            return;
        }
        if text == "/poll" || text.starts_with("/poll ") {
            self.say("polls come with M2");
            return;
        }
        self.composer.take();
        if let Some(e) = self.editing.take() {
            self.jawd.edit(&chat, &e.id, &text);
            self.composer.set(&e.draft);
            return;
        }
        let reply = self.replying.take().map(|r| r.id);
        let mentions = self.mentions_in(&text);
        self.taken.clear();
        self.dismissed = None;
        let key = self.keys.remove(&chat).unwrap_or_else(key);
        self.jawd.send_text(&chat, &text, &key, reply.as_deref(), &mentions);
        self.typing_at = 0;
        self.draft_sent.clear();
        self.draft_due = None;
        if !self.messages.as_ref().is_some_and(|w| w.following()) {
            self.watch(&chat, View::newest());
        }
        self.scroll = Scroll::Bottom;
    }

    fn send_read(&mut self) {
        let (Some(chat), Some(upto)) = (self.open.clone(), self.read_upto.clone()) else { return };
        self.jawd.read(&chat, &upto);
        self.read_at = self.now();
        self.read_due = None;
    }

    /// The newest message is on screen: what's unread in the chat has been seen, said at most
    /// once a second while messages keep coming.
    fn seen_newest(&mut self, newest: &Message) {
        let unread = self.open.as_deref().and_then(|id| self.chats.get(id)).is_some_and(|c| c.unread > 0);
        if !unread
            || self.panes.drawing
            || self.read_upto.as_deref() == Some(newest.id.as_str()) && self.read_due.is_none()
        {
            return;
        }
        self.read_upto = Some(newest.id.clone());
        if self.now() - self.read_at >= READ {
            self.send_read();
        } else {
            self.read_due = Some(self.read_at + READ);
        }
    }

    /// Whatever has come due: the chat list's order let go, a draft saved, a notice gone.
    pub fn tick(&mut self) {
        let now = self.now();
        let (second, minute) = (now / 1_000, now / MINUTE);
        if let Some(due) = self.first_frame_due() {
            self.dirty |= Instant::now() >= due;
        } else if minute != self.drawn_at / 60 || (self.counting() && second != self.drawn_at) {
            self.dirty = true;
        }
        if self.hold_until.is_some_and(|t| t <= now) {
            self.hold_until = None;
            self.keep_selected_row(|v| {
                v.chats.release();
            });
            self.dirty = true;
        }
        if self.draft_due.is_some_and(|t| t <= now)
            && let Some(chat) = self.open.clone()
        {
            self.save_draft(&chat);
        }
        if self.read_due.is_some_and(|t| t <= now) {
            self.send_read();
        }
        self.keep_layout_when_due(now);
        if self.notice.as_ref().is_some_and(|(_, until)| *until <= now) {
            self.notice = None;
            self.dirty = true;
        }
        if self.chomp_until.is_some_and(|t| t <= now) {
            self.chomp_until = None;
            self.dirty = true;
        }
        if self.images.next_wake().is_some_and(|t| t <= Instant::now()) {
            self.dirty = true;
        }
    }

    /// When the view next needs to wake with nothing arriving: the clock's next minute, and
    /// anything sooner that's due or moving on screen.
    pub fn next_wake(&self) -> i64 {
        let now = self.now();
        let mut wake = (now / MINUTE + 1) * MINUTE;
        let mut soon = |t: Option<i64>| {
            if let Some(t) = t {
                wake = wake.min(t);
            }
        };
        soon(self.hold_until);
        soon(self.draft_due);
        soon(self.read_due);
        soon(self.layout_due());
        soon(self.notice.as_ref().map(|(_, t)| *t));
        if self.chomp_until.is_some() {
            soon(Some(now + 100));
        }
        // Rounded up, so the loop never wakes just before the moment and turns until it comes.
        let in_view_time =
            |at: Instant| now + at.saturating_duration_since(Instant::now()).as_micros().div_ceil(1_000) as i64;
        soon(self.images.next_wake().map(in_view_time));
        if self.counting() {
            soon(Some((now / 1_000 + 1) * 1_000));
        }
        soon(self.first_frame_due().map(in_view_time));
        wake
    }

    fn linking(&self) -> bool {
        self.reach == Reach::Up && (self.linked == Some(false) || self.logged_out.is_some())
    }

    /// Who's typing in `chat` now, oldest first, as jawd last said.
    fn typists(&self, chat: &str) -> &[Typist] {
        self.typing.get(chat).map_or(&[], Vec::as_slice)
    }

    /// Who's typing in `chat`, in a word, for what's drawn of it to be kept by: 0 while nobody is.
    fn typing_key(&self, chat: &str) -> u64 {
        let typists = self.typists(chat);
        if typists.is_empty() {
            return 0;
        }
        let mut hasher = DefaultHasher::new();
        typists.hash(&mut hasher);
        hasher.finish() | 1
    }

    /// Who's typing in a chat, as its row and title say it: "typing…", or "recording…" when that's
    /// what they're all doing; in a group with their names first, as their messages name them.
    /// None while nobody is.
    fn who_types(&self, chat: &Chat) -> Option<String> {
        let typists = self.typists(&chat.id);
        if typists.is_empty() {
            return None;
        }
        let doing =
            if typists.iter().all(|t| t.state == TypistState::Recording) { "recording…" } else { "typing…" };
        if !chat.group() {
            return Some(doing.into());
        }
        let names: Vec<&str> = typists.iter().map(|t| t.name.as_str()).collect();
        Some(format!("{} {doing}", names.join(", ")))
    }

    /// How the person of a one-to-one chat is, in a word, as its header and the status line say it:
    /// online, or when they were last seen, from what jawd told its conversation, or its roster row
    /// until then; nothing when they hide it. It's drawn faintly while jawd isn't following them,
    /// as the roster draws them. Typing, which shows from the chat's own, is said before it.
    fn presence_said(&self, chat: &str) -> Option<(String, Style)> {
        let roster = || self.roster.iter().find(|p| p.chat.as_deref() == Some(chat)).map(|p| p.presence);
        let presence = self.presence.get(chat).copied().or_else(roster)?;
        let t = &self.theme;
        let (said, style) = match presence.state {
            PresenceState::Online => ("online".to_string(), Style::new().fg(t.online)),
            PresenceState::Seen => {
                (format!("seen {}", fmt::ago(presence.at?, self.now(), self.tz)), Style::new().fg(t.comment))
            }
            PresenceState::Typing | PresenceState::Recording | PresenceState::Hidden => return None,
        };
        Some((said, if presence.known { style } else { Style::new().fg(t.gutter) }))
    }

    /// What a message's lines depend on, from the one before it.
    fn shape(&self, m: &Message, prev: Option<&Message>, width: usize) -> Shape {
        fn who(m: &Message) -> Option<&str> {
            if m.mine { None } else { m.from.as_ref().map(|f| f.id.as_str()) }
        }
        let system = |m: &Message| matches!(m.kind, crate::data::Kind::System | crate::data::Kind::Call);
        let new_day = prev.is_none_or(|p| !fmt::same_day(p.at, m.at, self.tz));
        let changed = prev.is_none_or(|p| who(p) != who(m) || p.mine != m.mine || system(p)) || new_day;
        Shape {
            width,
            name: changed || self.gap(m, prev),
            time: changed || prev.is_some_and(|p| !fmt::same_minute(p.at, m.at, self.tz)),
            unfolded: self.unfolded.contains(&m.id),
            pictured: !self.pictureless.contains(&m.id),
            play: self.play_of(&m.id),
            irc: self.settings_shown().messages == jaw_proto::settings::Messages::Irc,
        }
    }

    /// A blank line before a message: a new sender in a group, or half an hour since the last.
    fn gap(&self, m: &Message, prev: Option<&Message>) -> bool {
        let Some(p) = prev else { return false };
        if !fmt::same_day(p.at, m.at, self.tz) {
            return false;
        }
        let group = self.open.as_deref().and_then(|id| self.chats.get(id)).is_some_and(|c| c.group());
        let sender_changed = p.mine != m.mine || p.from.as_ref().map(|f| &f.id) != m.from.as_ref().map(|f| &f.id);
        m.at - p.at >= 30 * MINUTE || (group && sender_changed)
    }

    /// A message's lines, laid out once per version and shape.
    fn lines(&mut self, m: &Message, shape: Shape) -> Rc<layout::Laidout> {
        if let Some(l) = self.laid.get(&m.id)
            && l.v == m.v
            && l.shape == shape
        {
            return l.lines.clone();
        }
        let pictures = self.images.protocol != Protocol::None && !self.no_color;
        let sharp = matches!(self.images.protocol, Protocol::Kitty | Protocol::Iterm | Protocol::Sixel);
        let ctx =
            layout::Ctx { theme: &self.theme, tz: self.tz, me: &self.me, pictures, cell: self.images.cell, sharp };
        let lines = Rc::new(layout::laid(m, shape, &ctx));
        self.laid.insert(m.id.clone(), Laid { v: m.v, shape, lines: lines.clone() });
        lines
    }
}

#[cfg(test)]
mod panes_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod spike_tests;
#[cfg(test)]
pub mod tests;
