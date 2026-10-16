//! jaw: WhatsApp in a terminal. It draws what jawd, the daemon, prepares; the terminal interface
//! comes with M1. Until then `jaw status` asks jawd how it is, and `jaw link` links an account.

use std::time::{SystemTime, UNIX_EPOCH};

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Status) | None => status(),
        Some(Command::Link { phone }) => link(phone),
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
