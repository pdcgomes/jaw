//! Where notifications go: Notification Center on macOS through osascript, as ooze's do, and
//! notify-send on Linux.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, bail};

use super::{Notice, Notifier};

const OSASCRIPT: &str = "/usr/bin/osascript";

/// The desktop's notifier, or why there's none.
pub fn find() -> Result<Box<dyn Notifier>, String> {
    if cfg!(target_os = "macos") {
        return match Path::new(OSASCRIPT).is_file() {
            true => Ok(Box::new(NotificationCenter)),
            false => Err(format!("off, since {OSASCRIPT} isn't there to reach Notification Center")),
        };
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    match std::env::split_paths(&path).map(|folder| folder.join("notify-send")).find(|program| program.is_file()) {
        Some(program) => Ok(Box::new(NotifySend(program))),
        None => Err("off, since notify-send isn't on PATH (it comes with libnotify)".into()),
    }
}

struct NotificationCenter;

impl Notifier for NotificationCenter {
    fn name(&self) -> &str {
        "to Notification Center"
    }

    fn notify(&self, notice: &Notice) -> Result<()> {
        let mut script = format!("display notification {} with title {}", quoted(&notice.body), quoted(&notice.title));
        if let Some(subtitle) = &notice.subtitle {
            script.push_str(&format!(" subtitle {}", quoted(subtitle)));
        }
        run(Command::new(OSASCRIPT).args(["-e", &script]))
    }
}

struct NotifySend(PathBuf);

impl Notifier for NotifySend {
    fn name(&self) -> &str {
        "through notify-send"
    }

    fn notify(&self, notice: &Notice) -> Result<()> {
        let body = match &notice.subtitle {
            Some(subtitle) => format!("{subtitle}: {}", notice.body),
            None => notice.body.clone(),
        };
        run(Command::new(&self.0).args(["--app-name=jaw", &notice.title, &body]))
    }
}

/// An AppleScript string.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn run(command: &mut Command) -> Result<()> {
    let out = command.output()?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_and_backslashes_stay_inside_the_applescript_string() {
        assert_eq!(quoted(r#"say "hi" \o/"#), r#""say \"hi\" \\o/""#);
    }
}
