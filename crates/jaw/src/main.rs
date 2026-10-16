//! jaw: WhatsApp in a terminal. It draws what jawd, the daemon, prepares. `jaw demo` shows the
//! terminal interface on a made-up account; until it talks to jawd, `jaw status` asks jawd how it
//! is, and `jaw link` links an account.

mod art;
mod bench;
mod composer;
mod data;
mod demo;
mod fmt;
mod layout;
mod run;
mod theme;
mod view;
mod window;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use jaw_proto::{AccountState, LinkStart, LinkStep};
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;

#[derive(Parser)]
#[command(name = "jaw", version, about = "WhatsApp in a terminal, fast, from the keyboard")]
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
        /// The made-up account; the only one until jaw talks to jawd
        #[arg(long)]
        demo: bool,
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
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Status) | None => status(),
        Some(Command::Link { phone }) => link(phone),
        Some(Command::Demo { state, flood }) => run::run(|tx| {
            let demo = demo::Demo::start(tx, demo::Clock::Wall, state, true);
            if let Some(rate) = flood {
                demo.flood(rate, f64::INFINITY);
            }
            view::View::new(Box::new(demo))
        }),
        Some(Command::Snapshot { width, height, demo, state, keys, ansi }) => {
            if !demo {
                bail!("jaw snapshot draws the made-up account with --demo; jawd's comes when jaw talks to it");
            }
            let (tx, rx) = std::sync::mpsc::channel();
            let now = chrono::Utc::now().timestamp_millis();
            let account = demo::Demo::start(tx, demo::Clock::Fixed(now), state, false);
            let mut view = view::View::new(Box::new(account));
            view.clock = Some(now);
            view.resize(width, height);
            run::settle(&mut view, &rx);
            run::press(&mut view, &rx, &keys)?;
            let frame = if ansi {
                run::snapshot_ansi(&mut view, width, height)
            } else {
                run::snapshot(&mut view, width, height)
            };
            print!("{frame}");
            Ok(())
        }
        Some(Command::Bench { rate, seconds, width, height }) => {
            let flood = bench::Flood {
                width,
                height,
                rate,
                seconds,
                every: Duration::from_millis(12),
                warm: Duration::from_secs(1),
            };
            println!("{}", bench::flood(&flood)?.describe());
            Ok(())
        }
    }
}

fn connect() -> Result<(jaw_client::Connection, jaw_proto::Welcome)> {
    let socket = jaw_proto::local::socket(&jaw_proto::local::home());
    jaw_client::Connection::open(&socket, "jaw", env!("CARGO_PKG_VERSION"))
}

fn status() -> Result<()> {
    let (mut connection, _) = connect()?;
    let status: jaw_proto::Status = serde_json::from_value(connection.call("status", serde_json::Value::Null)?)?;
    println!("{}", jaw_client::describe(&status));
    Ok(())
}

fn link(phone: Option<String>) -> Result<()> {
    let (mut connection, welcome) = connect()?;
    if welcome.account.state == AccountState::Linked {
        let who = welcome.account.me.map(|me| me.phone).unwrap_or_default();
        println!("jaw is already linked to {who}; jaw unlink comes later, or remove it on the phone");
        return Ok(());
    }
    let by_code = phone.is_some();
    connection.call("link.start", serde_json::to_value(LinkStart { phone })?)?;
    loop {
        let push = connection.push()?;
        if push.method != "link" {
            continue;
        }
        match serde_json::from_value::<LinkStep>(push.params)? {
            LinkStep::Qr { code, expires } if !by_code => {
                let seconds = (expires - now_ms()).max(0) / 1000;
                print!("\x1b[H\x1b[2J");
                println!("{}", qr(&code)?);
                println!("On the phone: WhatsApp › Settings › Linked devices › Link a device, then scan this.");
                println!("It changes in {seconds} s; Ctrl-C stops.");
            }
            LinkStep::Qr { .. } => {}
            LinkStep::Code { code } => {
                println!(
                    "On the phone: WhatsApp › Settings › Linked devices › Link a device › Link with phone number instead,"
                );
                println!("then type {code}");
            }
            LinkStep::Done { phone } => {
                println!("Linked {phone}. jawd is fetching your history; jaw status says how far it has got.");
                return Ok(());
            }
            LinkStep::Failed { reason } => bail!(reason),
        }
    }
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
