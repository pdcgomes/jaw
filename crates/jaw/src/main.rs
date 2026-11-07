//! jaw: WhatsApp in a terminal. It draws what jawd, the daemon, prepares: `jaw` on its own runs
//! the interface on jawd, starting jawd first if it isn't running, `jaw demo` runs it on a
//! made-up account, `jaw status` asks jawd how it is, `jaw link` links an account and `jaw unlink`
//! logs it out.

mod agent;
mod art;
mod bench;
mod composer;
mod data;
mod demo;
mod experiments;
mod find;
mod fmt;
mod graphics;
mod layout;
mod run;
mod theme;
mod view;
mod wire;

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, Subcommand};
use jaw_proto::settings::TERMINAL;
use jaw_proto::{AccountState, LinkStart, LinkStep, code};
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;

#[derive(Parser)]
#[command(name = "jaw", version = jaw_version::VERSION, about = "WhatsApp in a terminal, fast, from the keyboard")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// What jawd is connected to, how its engine is, and what's missing
    Status,
    /// Links your WhatsApp account to jaw, as a linked device
    Link {
        /// Link by typing a code on the phone instead of scanning a QR code: the phone's number in
        /// international form, digits only, such as 351912345678
        #[arg(long)]
        phone: Option<String>,
    },
    /// Logs jaw out of WhatsApp, as Linked devices on the phone does: jawd forgets the session and
    /// keeps your messages, and only the phone can link jaw again
    Unlink {
        /// Logs out without asking first
        #[arg(long)]
        yes: bool,
    },
    /// Prints the terminal's settings, or changes them: jaw settings theme=gruvbox messages=irc
    Settings {
        /// Settings to change, each as name=value
        #[arg(value_name = "NAME=VALUE", value_parser = pair)]
        changes: Vec<(String, String)>,
    },
    /// The terminal interface on a made-up account, to try jaw without one
    Demo {
        /// How things stand: linked and online, linking, logged out, jawd out of reach…
        #[arg(long, value_enum, default_value_t = demo::State::Online)]
        state: demo::State,
        /// Messages a second to pour into the account, half of them into the open chat
        #[arg(long)]
        flood: Option<u32>,
    },
    /// One frame of the terminal interface as text, after some keys
    Snapshot {
        #[arg(long, default_value_t = 140)]
        width: u16,
        #[arg(long, default_value_t = 40)]
        height: u16,
        /// The made-up account, rather than the jawd on jaw's folder
        #[arg(long)]
        demo: bool,
        /// Starts jawd first if nothing answers on jaw's folder, as jaw does, and waits for it to
        /// answer or stop
        #[arg(long, conflicts_with = "demo")]
        start: bool,
        #[arg(long, value_enum, default_value_t = demo::State::Online)]
        state: demo::State,
        /// Keys to press first: text types itself, \n is Enter, and <up>, <esc>, <a-down>,
        /// <a-enter> or <c-u> are those keys
        #[arg(long, default_value = "")]
        keys: String,
        /// In 24-bit colour, as escape codes
        #[arg(long)]
        ansi: bool,
    },
    /// Floods the made-up account and measures how long keypresses take to be drawn
    Bench {
        #[arg(long, default_value_t = 2_000)]
        rate: u32,
        #[arg(long, default_value_t = 10.0)]
        seconds: f64,
        #[arg(long, default_value_t = 140)]
        width: u16,
        #[arg(long, default_value_t = 40)]
        height: u16,
        /// What the keys do: type and act, find in the palette, walk the chat list, or nothing
        #[arg(long, value_enum, default_value_t = bench::Keys::Typing)]
        keys: bench::Keys,
        /// Milliseconds between two keys
        #[arg(long, default_value_t = 12)]
        every: u64,
        /// Seconds before the keys begin, so they're measured once things are steady
        #[arg(long, default_value_t = 1.0)]
        warm: f64,
        /// Floods a real jawd, `jawd bench feed`'s made-up account on a folder of its own
        #[arg(long)]
        jawd: bool,
        /// Draws the jawd on this socket as it is, such as `jawd demo`'s, rather than flooding one
        #[arg(long)]
        socket: Option<std::path::PathBuf>,
        /// Stops at the first frame with a conversation, timing jaw's start
        #[arg(long)]
        first: bool,
        /// Writes the report to this file as JSON as well
        #[arg(long)]
        json: Option<std::path::PathBuf>,
        /// What pictures are drawn with: kitty, iterm, sixel, blocks or none
        #[arg(long, default_value = "blocks", value_parser = graphics::Protocol::parse)]
        graphics: graphics::Protocol,
        /// Plays a voice message all along, its playhead moving ten times a second
        #[arg(long)]
        voice: bool,
        /// Shows the library's grid of photos across all chats, the flood's photos coming into it
        #[arg(long)]
        library: bool,
        /// Has a fourth of the open chat's messages disappear a second after they come
        #[arg(long)]
        disappearing: bool,
    },
    /// Starts jawd at login, as a LaunchAgent on macOS or a systemd user unit on Linux
    #[command(subcommand)]
    Agent(agent::Action),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    experiments::Experiments::start(experiments::Experiments::from_env());
    match cli.command {
        None => run::run(|tx| view::View::new(Box::new(wire::Wire::start(&jaw_proto::local::home(), tx)))),
        Some(Command::Status) => status(),
        Some(Command::Link { phone }) => {
            link(&jaw_proto::local::socket(&jaw_proto::local::home()), phone, &mut std::io::stdout())
        }
        Some(Command::Unlink { yes }) => {
            let socket = jaw_proto::local::socket(&jaw_proto::local::home());
            unlink(&socket, yes, &mut std::io::stdin().lock(), &mut std::io::stdout())
        }
        Some(Command::Settings { changes }) => {
            if let Err(twice) = asked_settings(&changes) {
                let mut jaw = Cli::command();
                jaw.build();
                let command = jaw.find_subcommand_mut("settings").expect("jaw has a settings command");
                command.error(ErrorKind::ArgumentConflict, twice).exit();
            }
            println!("{}", settings(&jaw_proto::local::socket(&jaw_proto::local::home()), &changes)?);
            Ok(())
        }
        Some(Command::Demo { state, flood }) => run::run(|tx| {
            let demo = demo::Demo::start(tx, demo::Clock::Wall, state, true);
            demo.answer_on_a_thread();
            if let Some(rate) = flood {
                demo.flood(rate, f64::INFINITY);
            }
            view::View::new(Box::new(demo))
        }),
        Some(Command::Snapshot { width, height, demo, start, state, keys, ansi }) => {
            let (tx, rx) = std::sync::mpsc::channel();
            let now = chrono::Utc::now().timestamp_millis();
            let home = jaw_proto::local::home();
            let mut view = if demo {
                let account = demo::Demo::start(tx, demo::Clock::Fixed(now), state, false);
                view::View::new(Box::new(account))
            } else if start {
                view::View::new(Box::new(wire::Wire::start(&home, tx)))
            } else {
                view::View::new(Box::new(wire::Wire::connect(jaw_proto::local::socket(&home), tx)))
            };
            view.clock = Some(now);
            view.images = graphics::Images::here(graphics::Protocol::Blocks, (8, 16));
            view.resize(width, height);
            if demo {
                run::settle(&mut view, &rx);
                run::press(&mut view, &rx, &keys)?;
            } else {
                // A jawd being started is waited for until it answers or stops, or a while before
                // jaw would stop waiting for it.
                let most = if start { wire::PATIENCE - Duration::from_secs(3) } else { Duration::from_secs(5) };
                run::settle_live(&mut view, &rx, most);
                run::press_live(&mut view, &rx, &keys)?;
            }
            let frame = if ansi {
                run::snapshot_ansi(&mut view, width, height)
            } else {
                run::snapshot(&mut view, width, height)
            };
            print!("{frame}");
            Ok(())
        }
        Some(Command::Bench {
            rate,
            seconds,
            width,
            height,
            keys,
            every,
            warm,
            jawd,
            socket,
            first,
            json,
            graphics,
            voice,
            library,
            disappearing,
        }) => {
            let flood = bench::Flood {
                width,
                height,
                rate,
                seconds,
                every: Duration::from_millis(every.max(1)),
                warm: Duration::from_secs_f64(warm),
                keys,
                jawd: jawd || socket.is_some(),
                socket,
                until_first: first,
                pictures: graphics,
                voice,
                library,
                disappearing,
            };
            let report = bench::flood(&flood)?;
            println!("{}", report.describe());
            if let Some(path) = json {
                std::fs::write(&path, serde_json::to_vec_pretty(&report.json())?)?;
            }
            Ok(())
        }
        Some(Command::Agent(action)) => agent::run(action),
    }
}

fn connect(socket: &Path) -> Result<(jaw_client::Connection, jaw_proto::Welcome)> {
    jaw_client::Connection::open(socket, TERMINAL, jaw_version::VERSION)
}

fn status() -> Result<()> {
    let socket = jaw_proto::local::socket(&jaw_proto::local::home());
    let (mut connection, _) = connect(&socket)?;
    let status: jaw_proto::Status = serde_json::from_value(connection.call("status", serde_json::Value::Null)?)?;
    println!("{}", jaw_client::describe(&status));
    if status.jawd != jaw_version::VERSION {
        let other = data::OtherJawd { version: status.jawd, pid: jaw_client::jawd_pid(&socket) };
        println!("jaw       {}, another build than jawd's: {}", jaw_version::VERSION, other.remedy());
    }
    Ok(())
}

/// `jaw link`. Whether an account is linked is the engine's to say, which refuses to link over one:
/// a hello saying linked can come before the engine has found the device gone, or from a link the
/// phone paired that didn't finish, and then linking starts afresh.
fn link(socket: &Path, phone: Option<String>, out: &mut impl std::io::Write) -> Result<()> {
    let (mut connection, welcome) = connect(socket)?;
    let by_code = phone.is_some();
    if let Err(e) = connection.call("link.start", serde_json::to_value(LinkStart { phone })?) {
        let refused = e.downcast_ref::<jaw_proto::Error>().is_some_and(|e| e.code == code::NOT_ALLOWED);
        if refused && welcome.account.state == AccountState::Linked {
            let who = welcome.account.me.map(|me| me.phone).unwrap_or_default();
            writeln!(out, "jaw is already linked to {who}; jaw unlink logs it out")?;
            return Ok(());
        }
        return Err(e);
    }
    loop {
        let push = connection.push()?;
        if push.method != "link" {
            continue;
        }
        match serde_json::from_value::<LinkStep>(push.params)? {
            LinkStep::Qr { code, expires } if !by_code => {
                let seconds = (expires - now_ms()).max(0) / 1000;
                write!(out, "\x1b[H\x1b[2J")?;
                writeln!(out, "{}", qr(&code)?)?;
                writeln!(out, "On the phone: WhatsApp › Settings › Linked devices › Link a device, then scan this.")?;
                writeln!(out, "It changes in {seconds} s; Ctrl-C stops.")?;
                out.flush()?;
            }
            LinkStep::Qr { .. } => {}
            LinkStep::Code { code } => {
                writeln!(
                    out,
                    "On the phone: WhatsApp › Settings › Linked devices › Link a device › Link with phone number instead,"
                )?;
                writeln!(out, "then type {code}")?;
                out.flush()?;
            }
            LinkStep::Done { phone } => {
                writeln!(out, "Linked {phone}. jawd is fetching your history; jaw status says how far it has got.")?;
                return Ok(());
            }
            LinkStep::Failed { belongs_to: Some(number), .. } => bail!(data::kept_for(&number)),
            LinkStep::Failed { reason, .. } => bail!(reason),
        }
    }
}

/// `jaw unlink`: logs the account out once `answers` says yes to the question it writes to `out`,
/// or at once when `yes`, since nothing in jaw can link it again: only the phone can.
fn unlink(socket: &Path, yes: bool, answers: &mut impl std::io::BufRead, out: &mut impl std::io::Write) -> Result<()> {
    let (mut connection, welcome) = connect(socket)?;
    if welcome.account.state != AccountState::Linked {
        writeln!(out, "jaw isn't linked to WhatsApp; jaw link links it")?;
        return Ok(());
    }
    let whose = welcome.account.me.map_or("your".to_string(), |me| format!("{}'s", me.phone));
    if !yes {
        writeln!(out, "jaw stops being one of {whose} linked devices, and only the phone can link it again;")?;
        write!(out, "jawd keeps your messages. Log out? [y/N] ")?;
        out.flush()?;
        let mut answer = String::new();
        answers.read_line(&mut answer)?;
        if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
            writeln!(out, "jaw is still linked.")?;
            return Ok(());
        }
    }
    connection.call("link.logout", serde_json::Value::Null)?;
    writeln!(out, "Logged out. jawd has deleted the session and kept your messages; jaw link links jaw again.")?;
    Ok(())
}

/// The terminal's settings as `jaw settings` prints them: those hello brings, or with `changes`,
/// jawd's answer to one `settings.set` asking for them all. A setting named twice is refused before
/// jawd is asked, and a jawd whose hello brings no settings, one from before them, keeps none.
fn settings(socket: &Path, changes: &[(String, String)]) -> Result<String> {
    let asked = asked_settings(changes)?;
    let (mut connection, welcome) = connect(socket)?;
    let Some(said) = welcome.settings else { bail!(wire::OLDER) };
    let now = if changes.is_empty() {
        said
    } else {
        let kept = connection.call("settings.set", asked)?;
        serde_json::from_value::<jaw_proto::settings::SettingsKept>(kept)
            .context("jawd's answer to settings.set")?
            .settings
    };
    Ok(describe_settings(&now))
}

/// One of `jaw settings`'s arguments as a setting's name and its value, each trimmed. Only its form
/// is checked here, as clap reads it; jawd checks the name and the value.
fn pair(arg: &str) -> Result<(String, String), String> {
    let Some((name, value)) = arg.split_once('=') else {
        return Err("it isn't a name=value pair, such as theme=gruvbox".into());
    };
    if name.trim().is_empty() {
        return Err("it names no setting before =, such as theme=gruvbox".into());
    }
    Ok((name.trim().to_string(), value.trim().to_string()))
}

/// The changes as `settings.set`'s params, a setting named twice refused; jawd checks the names and
/// the values.
fn asked_settings(changes: &[(String, String)]) -> Result<serde_json::Value> {
    let mut asked = serde_json::Map::new();
    for (name, value) in changes {
        if asked.insert(name.clone(), serde_json::Value::from(value.as_str())).is_some() {
            bail!("{name} is asked for twice; give each setting once");
        }
    }
    Ok(serde_json::Value::Object(asked))
}

/// The settings as `jaw settings` prints them, each with the values it takes.
fn describe_settings(settings: &jaw_proto::settings::Settings) -> String {
    let themes: Vec<&str> = jaw_proto::themes::all().iter().map(|t| t.id.as_str()).collect();
    let messages = jaw_proto::settings::Messages::ALL.map(|m| m.name());
    let name = jaw_proto::themes::get(&settings.theme).map_or("not one this jaw knows", |t| t.name.as_str());
    [
        format!("theme     {} ({name})", settings.theme),
        one_of(&themes),
        format!("messages  {}", settings.messages.name()),
        one_of(&messages),
    ]
    .join("\n")
}

/// The values a setting takes, after "one of" and under the names' indent, wrapped at 80 columns,
/// so a terminal of any width and a script read the same lines.
fn one_of(values: &[&str]) -> String {
    let mut lines = vec!["          one of".to_string()];
    for (n, value) in values.iter().enumerate() {
        let value = if n + 1 < values.len() { format!("{value},") } else { value.to_string() };
        match lines.last_mut() {
            Some(line) if line.len() + 1 + value.len() <= 80 => {
                line.push(' ');
                line.push_str(&value);
            }
            _ => lines.push(format!("          {value}")),
        }
    }
    lines.join("\n")
}

/// The code as half-block characters, light on dark, so a terminal with a dark background shows a
/// QR code the phone can read.
fn qr(code: &str) -> Result<String> {
    let qr = QrCode::new(code.as_bytes())?;
    Ok(qr.render::<Dense1x2>().dark_color(Dense1x2::Light).light_color(Dense1x2::Dark).quiet_zone(true).build())
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::sync::{Arc, Mutex};

    use super::*;
    use jaw_proto::settings::{Messages, Settings};
    use serde_json::{Value, json};

    /// A jawd on a socket of its own for one connection: its hello says `account`, and it answers
    /// each other request as `answer` says, its outcome then the pushes after it. It keeps the
    /// methods it was asked.
    struct Answering {
        _folder: tempfile::TempDir,
        socket: std::path::PathBuf,
        asked: Arc<Mutex<Vec<String>>>,
    }

    impl Answering {
        fn new(account: Value, answer: fn(&str) -> (Value, Vec<Value>)) -> Answering {
            let folder = tempfile::tempdir().unwrap();
            let socket = folder.path().join("jaw.sock");
            let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
            let asked = Arc::new(Mutex::new(Vec::new()));
            let keeping = asked.clone();
            let hello = concat!(env!("CARGO_MANIFEST_DIR"), "/../../protocol/fixtures/hello.result.json");
            let mut welcome: Value = serde_json::from_str(&std::fs::read_to_string(hello).unwrap()).unwrap();
            welcome["result"]["account"] = account;
            std::thread::spawn(move || {
                let Ok((stream, _)) = listener.accept() else { return };
                let mut out = stream.try_clone().unwrap();
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { return };
                    let request: Value = serde_json::from_str(&line).unwrap();
                    let method = request["method"].as_str().unwrap_or_default().to_string();
                    keeping.lock().unwrap().push(method.clone());
                    let (mut outcome, pushes) =
                        if method == "hello" { (welcome["result"].clone(), vec![]) } else { answer(&method) };
                    let reply = if outcome.get("error").is_some() {
                        outcome["id"] = request["id"].clone();
                        outcome
                    } else {
                        json!({ "id": request["id"], "result": outcome })
                    };
                    writeln!(out, "{reply}").unwrap();
                    for push in pushes {
                        writeln!(out, "{push}").unwrap();
                    }
                }
            });
            Answering { _folder: folder, socket, asked }
        }

        fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }
    }

    fn linked() -> Value {
        json!({ "state": "linked", "me": { "id": "person:1", "name": "Pedro", "phone": "+351 912 000 000" } })
    }

    fn unlinked() -> Value {
        json!({ "state": "unlinked" })
    }

    fn link_push(step: Value) -> Value {
        json!({ "method": "link", "params": step })
    }

    fn refusal(code: i32, message: &str) -> Value {
        json!({ "error": { "code": code, "message": message } })
    }

    #[test]
    fn jaw_link_asks_jawd_even_when_its_hello_says_linked_and_says_already_linked_only_when_the_engine_refuses() {
        let linking = |method: &str| match method {
            "link.start" => {
                let qr = link_push(json!({ "state": "qr", "code": "2@made-up", "expires": now_ms() + 20_000 }));
                (json!({}), vec![qr, link_push(json!({ "state": "done", "phone": "+351 912 000 000" }))])
            }
            _ => (refusal(code::UNKNOWN_METHOD, "not here"), vec![]),
        };
        let jawd = Answering::new(linked(), linking);
        let mut out = Vec::new();
        link(&jawd.socket, None, &mut out).unwrap();
        let said = String::from_utf8(out).unwrap();
        assert!(said.contains("████") && said.contains("Linked +351 912 000 000."), "{said}");
        assert_eq!(jawd.asked(), ["hello", "link.start"], "a hello saying linked can be from before the engine knew");

        let refusing = |_: &str| (refusal(code::NOT_ALLOWED, "an account is already linked; log out first"), vec![]);
        let jawd = Answering::new(linked(), refusing);
        let mut out = Vec::new();
        link(&jawd.socket, None, &mut out).unwrap();
        let said = String::from_utf8(out).unwrap();
        assert_eq!(said, "jaw is already linked to +351 912 000 000; jaw unlink logs it out\n");

        let jawd = Answering::new(unlinked(), refusing);
        let failed = link(&jawd.socket, None, &mut Vec::new()).unwrap_err();
        assert_eq!(failed.to_string(), "an account is already linked; log out first", "jawd's own words");

        let failing = |_: &str| {
            let failed = link_push(json!({ "state": "failed", "reason": "WhatsApp refused the new device: 401" }));
            (json!({}), vec![failed])
        };
        let jawd = Answering::new(unlinked(), failing);
        let failed = link(&jawd.socket, None, &mut Vec::new()).unwrap_err();
        assert_eq!(failed.to_string(), "WhatsApp refused the new device: 401");
    }

    #[test]
    fn jaw_link_says_which_number_the_folder_belongs_to_when_the_phone_linked_another() {
        let refusing = |_: &str| {
            let reason = "the phone linked +351919999999, and this folder keeps +351910000001's messages";
            let code = link_push(json!({ "state": "code", "code": "ABCD-EFGH" }));
            let failed = link_push(json!({ "state": "failed", "reason": reason, "belongs_to": "+351910000001" }));
            (json!({}), vec![code, failed])
        };
        let jawd = Answering::new(unlinked(), refusing);
        let mut out = Vec::new();
        let failed = link(&jawd.socket, Some("351919999999".into()), &mut out).unwrap_err();
        assert_eq!(
            failed.to_string(),
            "this folder belongs to +351910000001, whose messages jaw keeps in it, so only +351910000001 links here: \
            link it, or start jaw on a folder of its own for the other account, with JAW_HOME=<folder> jaw"
        );
        assert!(String::from_utf8(out).unwrap().contains("then type ABCD-EFGH"), "the code shown first");
    }

    #[test]
    fn jaw_unlink_logs_out_only_once_told_yes_and_says_so_when_nothing_is_linked() {
        let logging_out = |method: &str| match method {
            "link.logout" => {
                let out = json!({ "method": "connection", "params": { "state": "logged_out", "reason": "logged out from jaw" } });
                (json!({}), vec![out])
            }
            _ => (refusal(code::UNKNOWN_METHOD, "not here"), vec![]),
        };
        for answer in ["n\n", "\n", "", "maybe\n"] {
            let jawd = Answering::new(linked(), logging_out);
            let mut out = Vec::new();
            unlink(&jawd.socket, false, &mut answer.as_bytes(), &mut out).unwrap();
            let said = String::from_utf8(out).unwrap();
            assert!(said.contains("only the phone can link it again") && said.contains("Log out? [y/N] "), "{said}");
            assert!(said.ends_with("jaw is still linked.\n"), "{answer:?}: {said}");
            assert_eq!(jawd.asked(), ["hello"], "{answer:?}: nothing asked of jawd");
        }
        for (yes, answer) in [(false, "y\n"), (false, " Yes \n"), (true, "")] {
            let jawd = Answering::new(linked(), logging_out);
            let mut out = Vec::new();
            unlink(&jawd.socket, yes, &mut answer.as_bytes(), &mut out).unwrap();
            let said = String::from_utf8(out).unwrap();
            assert!(said.ends_with(
                "Logged out. jawd has deleted the session and kept your messages; jaw link links jaw again.\n"
            ));
            assert_eq!(said.contains("Log out?"), !yes, "--yes asks nothing: {said}");
            assert_eq!(jawd.asked(), ["hello", "link.logout"], "{yes} {answer:?}");
        }
        let jawd = Answering::new(unlinked(), logging_out);
        let mut out = Vec::new();
        unlink(&jawd.socket, false, &mut "y\n".as_bytes(), &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "jaw isn't linked to WhatsApp; jaw link links it\n");
        assert_eq!(jawd.asked(), ["hello"]);

        let unreachable = |_: &str| {
            let why = "can't log out: error sending logout request: websocket not connected";
            (refusal(code::ENGINE_UNAVAILABLE, why), vec![])
        };
        let jawd = Answering::new(linked(), unreachable);
        let failed = unlink(&jawd.socket, true, &mut "".as_bytes(), &mut Vec::new()).unwrap_err();
        assert_eq!(failed.to_string(), "can't log out: error sending logout request: websocket not connected");
    }

    /// The changes as clap reads them from `jaw settings`'s arguments.
    pub fn changes(args: &[&str]) -> Vec<(String, String)> {
        args.iter().map(|arg| pair(arg).unwrap()).collect()
    }

    #[test]
    fn clap_reads_each_change_as_a_name_and_a_value_and_refuses_one_without_a_name_as_it_reads_it() {
        let Some(Command::Settings { changes }) =
            Cli::try_parse_from(["jaw", "settings", "theme=gruvbox", " messages = irc "]).unwrap().command
        else {
            panic!("jaw settings is the settings command");
        };
        assert_eq!(changes, [("theme".into(), "gruvbox".into()), ("messages".into(), "irc".into())]);
        for (arg, why) in [
            ("gruvbox", "it isn't a name=value pair"),
            ("=gruvbox", "it names no setting"),
            (" =x", "it names no setting"),
        ] {
            let Err(refused) = Cli::try_parse_from(["jaw", "settings", arg]) else { panic!("{arg} is refused") };
            assert_eq!((refused.kind(), refused.exit_code()), (ErrorKind::ValueValidation, 2), "{arg}");
            let said = refused.to_string();
            assert!(said.contains(&format!("invalid value '{arg}' for '[NAME=VALUE]...': {why}")), "{said}");
        }
        let Err(help) = Cli::try_parse_from(["jaw", "settings", "--help"]) else { panic!("--help is its help") };
        let help = help.to_string();
        assert!(
            help.starts_with("Prints the terminal's settings, or changes them: jaw settings theme=gruvbox"),
            "{help}"
        );
        assert!(help.contains("Usage: jaw settings [NAME=VALUE]..."), "{help}");
    }

    #[test]
    fn the_changes_are_asked_as_given_for_jawd_to_check_and_a_setting_named_twice_is_refused() {
        let asked = asked_settings(&changes(&["theme=gruvbox", "messages=irc"])).unwrap();
        assert_eq!(asked, json!({"theme": "gruvbox", "messages": "irc"}));
        assert_eq!(asked_settings(&changes(&["colour=blue"])).unwrap(), json!({"colour": "blue"}), "jawd checks them");
        let twice = asked_settings(&changes(&["theme=nord", "theme=gruvbox"])).unwrap_err();
        assert_eq!(twice.to_string(), "theme is asked for twice; give each setting once");
    }

    #[test]
    fn settings_are_said_with_the_values_each_takes_wrapped_at_80_columns_under_the_indent() {
        let takes = "          one of catppuccin, catppuccin-latte, ethereal, everforest,
          flexoki-light, gruvbox, hackerman, kanagawa, last-horizon, lumon,
          lupine, matte-black, miasma, nord, osaka-jade, retro-82, ristretto,
          rose-pine, solitude, starship, tokyo-night, vantablack, white";
        let defaults = describe_settings(&Settings::default());
        let said =
            format!("theme     tokyo-night (Tokyo Night)\n{takes}\nmessages  bubbles\n          one of bubbles, irc");
        assert_eq!(defaults, said);
        let nord = describe_settings(&Settings { theme: "nord".into(), messages: Messages::Irc });
        assert_eq!(nord, format!("theme     nord (Nord)\n{takes}\nmessages  irc\n          one of bubbles, irc"));
        assert!(defaults.lines().chain(nord.lines()).all(|line| line.len() <= 80), "{defaults}");
        let unknown = describe_settings(&Settings { theme: "rose-pine-moon".into(), messages: Messages::Bubbles });
        assert!(
            unknown.starts_with("theme     rose-pine-moon (not one this jaw knows)\n"),
            "no name made up:\n{unknown}"
        );
    }
}
