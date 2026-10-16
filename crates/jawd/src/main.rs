//! jawd, jaw's daemon: it keeps every chat and message, does the work interfaces would otherwise
//! repeat, and serves them on a Unix socket. docs/plans/2026-10-16-jaw-design.md is its design.

mod sock;
mod state;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "jawd",
    version,
    about = "jaw's daemon: it keeps your WhatsApp and does the work for jaw's interfaces"
)]
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
    let home = jaw_proto::local::home();
    match cli.command {
        Some(Command::Status) => status(&home),
        None => run(&home),
    }
}

fn run(home: &Path) -> Result<()> {
    std::fs::create_dir_all(home).with_context(|| format!("can't make {}", home.display()))?;
    std::fs::set_permissions(home, std::fs::Permissions::from_mode(0o700))?;
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build()?;
    runtime.block_on(async {
        let socket = jaw_proto::local::socket(home);
        let listener = sock::listen(&socket)?;
        let state = Arc::new(state::State::new(home)?);
        let outcome = tokio::select! {
            served = sock::serve(state, listener) => served,
            _ = shutdown() => Ok(()),
        };
        let _ = std::fs::remove_file(&socket);
        outcome
    })
}

/// Waits for Ctrl-C or SIGTERM, which launchd and systemd send to stop jawd.
async fn shutdown() {
    let mut term =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("a SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}

fn status(home: &Path) -> Result<()> {
    let socket = jaw_proto::local::socket(home);
    let (mut connection, _) = jaw_client::Connection::open(&socket, "jawd status", env!("CARGO_PKG_VERSION"))?;
    let status: jaw_proto::Status = serde_json::from_value(connection.call("status", serde_json::Value::Null)?)?;
    println!("{}", jaw_client::describe(&status));
    Ok(())
}
