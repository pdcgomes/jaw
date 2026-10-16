//! jaw: WhatsApp in a terminal. It draws what jawd, the daemon, prepares; the terminal interface
//! comes with M1, and until then `jaw status` asks jawd how it is.

use anyhow::Result;
use clap::{Parser, Subcommand};

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Status) | None => status(),
    }
}

fn status() -> Result<()> {
    let socket = jaw_proto::local::socket(&jaw_proto::local::home());
    let (mut connection, _) = jaw_client::Connection::open(&socket, "jaw", env!("CARGO_PKG_VERSION"))?;
    let status: jaw_proto::Status = serde_json::from_value(connection.call("status", serde_json::Value::Null)?)?;
    println!("{}", jaw_client::describe(&status));
    Ok(())
}
