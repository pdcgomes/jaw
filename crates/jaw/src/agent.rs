//! `jaw agent`: jawd started at login and again if it stops, as a LaunchAgent on macOS or a systemd
//! user unit on Linux, as ooze's watch is. Both stop jawd with SIGTERM, sent to jawd alone, so it
//! stops in its own order before they'd kill it.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Subcommand;

pub const LABEL: &str = "dev.badcoffee.jaw.jawd";
const UNIT: &str = "jawd.service";
/// How long launchd and systemd wait for jawd to stop before killing it; it takes 3 s at most.
const STOP_SECONDS: u32 = 10;
/// How soon jawd is started again after it stops, or after it couldn't start.
const RESTART_SECONDS: u32 = 10;

#[derive(Clone, Copy, Subcommand)]
pub enum Action {
    /// Starts jawd at login, and now
    Install,
    /// Stops jawd and stops starting it at login
    Uninstall,
    /// Whether jawd starts at login, is loaded, and answers
    Status,
}

/// Where the agent goes and what it runs.
pub struct Places {
    /// The user's home folder.
    pub home: PathBuf,
    /// jaw's folder, where jawd's log goes.
    pub jaw: PathBuf,
    /// `JAW_HOME` as it's set now, which the agent's jawd keeps.
    pub jaw_home: Option<PathBuf>,
}

impl Places {
    pub fn here() -> Places {
        Places {
            home: std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp")),
            jaw: jaw_proto::local::home(),
            jaw_home: std::env::var_os("JAW_HOME").map(PathBuf::from),
        }
    }

    fn log(&self) -> PathBuf {
        self.jaw.join("jawd.log")
    }

    /// The agent's file: the LaunchAgent's plist on macOS, the unit elsewhere.
    pub fn agent(&self) -> PathBuf {
        if cfg!(target_os = "macos") {
            self.home.join("Library/LaunchAgents").join(format!("{LABEL}.plist"))
        } else {
            self.home.join(".config/systemd/user").join(UNIT)
        }
    }

    /// Writes the agent's file to run `jawd`, and jaw's folder for its log, and says where.
    pub fn write(&self, jawd: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.jaw).with_context(|| format!("can't make {}", self.jaw.display()))?;
        std::fs::set_permissions(&self.jaw, std::fs::Permissions::from_mode(0o700))?;
        let path = self.agent();
        std::fs::create_dir_all(path.parent().expect("the agent's folder"))?;
        let text = if cfg!(target_os = "macos") {
            plist(jawd, &self.log(), self.jaw_home.as_deref())
        } else {
            unit(jawd, &self.log(), self.jaw_home.as_deref())
        };
        std::fs::write(&path, text).with_context(|| format!("can't write {}", path.display()))?;
        Ok(path)
    }
}

pub fn run(action: Action) -> Result<()> {
    let places = Places::here();
    match action {
        Action::Install => install(&places),
        Action::Uninstall => uninstall(&places),
        Action::Status => {
            println!("{}", describe(&places, &status(&places)));
            Ok(())
        }
    }
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The LaunchAgent: jawd, started at login and again if it stops, logging into jaw's folder.
pub fn plist(jawd: &Path, log: &Path, jaw_home: Option<&Path>) -> String {
    let log = xml(&log.display().to_string());
    let jaw_home = match jaw_home {
        Some(home) => format!("\n\t\t<key>JAW_HOME</key>\n\t\t<string>{}</string>", xml(&home.display().to_string())),
        None => String::new(),
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<true/>
	<key>ThrottleInterval</key>
	<integer>{RESTART_SECONDS}</integer>
	<key>ExitTimeOut</key>
	<integer>{STOP_SECONDS}</integer>
	<key>EnvironmentVariables</key>
	<dict>
		<key>PATH</key>
		<string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>{jaw_home}
	</dict>
	<key>StandardOutPath</key>
	<string>{log}</string>
	<key>StandardErrorPath</key>
	<string>{log}</string>
</dict>
</plist>
"#,
        xml(&jawd.display().to_string())
    )
}

/// The systemd user unit, for Linux. `KillMode=mixed` sends SIGTERM to jawd alone, so the engine
/// isn't stopped before jawd is done with it.
pub fn unit(jawd: &Path, log: &Path, jaw_home: Option<&Path>) -> String {
    let environment = match jaw_home {
        Some(home) => format!("Environment=\"JAW_HOME={}\"\n", home.display()),
        None => String::new(),
    };
    format!(
        "[Unit]\nDescription=jawd: jaw's daemon, which keeps your WhatsApp\n\n[Service]\nExecStart={}\n{environment}Restart=always\nRestartSec={RESTART_SECONDS}\nKillMode=mixed\nKillSignal=SIGTERM\nTimeoutStopSec={STOP_SECONDS}\nStandardOutput=append:{log}\nStandardError=append:{log}\n\n[Install]\nWantedBy=default.target\n",
        jawd.display(),
        log = log.display(),
    )
}

/// jawd, from beside the jaw that's running.
fn jawd() -> Result<PathBuf> {
    let jaw = std::env::current_exe()?.canonicalize()?;
    let jawd = jaw.with_file_name("jawd");
    if !jawd.is_file() {
        bail!("jawd isn't beside jaw in {}; make build puts them together", jaw.parent().unwrap_or(&jaw).display());
    }
    Ok(jawd)
}

fn domain() -> String {
    format!("gui/{}", unsafe { libc::getuid() })
}

fn install(places: &Places) -> Result<()> {
    let jawd = jawd()?;
    let elsewhere = answering(places).is_some() && !loaded();
    let path = places.write(&jawd)?;
    if cfg!(target_os = "macos") {
        // Booting out an agent already loaded stops its jawd with SIGTERM, then the new one starts.
        let _ = Command::new("/bin/launchctl").args(["bootout", &format!("{}/{LABEL}", domain())]).output();
        let out = Command::new("/bin/launchctl").args(["bootstrap", &domain()]).arg(&path).output()?;
        if !out.status.success() {
            bail!(
                "wrote {} but launchctl couldn't load it: {}\nRun this in Terminal: launchctl bootstrap {} {}",
                path.display(),
                String::from_utf8_lossy(&out.stderr).trim(),
                domain(),
                path.display()
            );
        }
        println!("Installed {LABEL}: jawd starts at login, and again if it stops.");
    } else {
        let _ = Command::new("systemctl").args(["--user", "daemon-reload"]).output();
        match Command::new("systemctl").args(["--user", "enable", "--now", UNIT]).output() {
            Ok(out) if out.status.success() => {
                println!("Installed {UNIT}: jawd starts at login, and again if it stops.")
            }
            _ => println!("Wrote {}. Start it with: systemctl --user enable --now {UNIT}", path.display()),
        }
    }
    println!("Agent: {}\nLog:   {}", path.display(), places.log().display());
    if elsewhere {
        println!("A jawd started some other way is running; the agent's takes over once it stops.");
    }
    Ok(())
}

fn uninstall(places: &Places) -> Result<()> {
    if cfg!(target_os = "macos") {
        let _ = Command::new("/bin/launchctl").args(["bootout", &format!("{}/{LABEL}", domain())]).output();
    } else {
        let _ = Command::new("systemctl").args(["--user", "disable", "--now", UNIT]).output();
    }
    let path = places.agent();
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("can't remove {}", path.display()))?;
        println!("Removed {}; jawd has stopped and won't start at login.", path.display());
    } else {
        println!("No agent was installed.");
    }
    Ok(())
}

/// How the agent stands.
pub struct Status {
    pub installed: bool,
    pub loaded: bool,
    /// jawd's version, if it answers.
    pub answering: Option<String>,
}

fn status(places: &Places) -> Status {
    Status { installed: places.agent().exists(), loaded: loaded(), answering: answering(places) }
}

/// Whether launchd or systemd has the agent loaded.
fn loaded() -> bool {
    if cfg!(target_os = "macos") {
        Command::new("/bin/launchctl")
            .args(["print", &format!("{}/{LABEL}", domain())])
            .output()
            .is_ok_and(|out| out.status.success())
    } else {
        Command::new("systemctl").args(["--user", "is-active", "--quiet", UNIT]).status().is_ok_and(|s| s.success())
    }
}

/// jawd's version, if a jawd answers on jaw's socket.
fn answering(places: &Places) -> Option<String> {
    let socket = jaw_proto::local::socket(&places.jaw);
    jaw_client::Connection::open(&socket, "jaw agent", env!("CARGO_PKG_VERSION")).ok().map(|(_, welcome)| welcome.jawd)
}

pub fn describe(places: &Places, status: &Status) -> String {
    let agent = places.agent();
    let mut lines = vec![match (status.installed, status.loaded) {
        (true, true) => format!("agent     installed and loaded: {}", agent.display()),
        (true, false) => format!("agent     installed but not loaded: {}; jaw agent install loads it", agent.display()),
        (false, true) => "agent     loaded, but its file is gone; jaw agent uninstall unloads it".to_string(),
        (false, false) => "agent     not installed; jaw agent install starts jawd at login".to_string(),
    }];
    lines.push(match &status.answering {
        Some(version) if status.loaded => format!("jawd      {version}, answering"),
        Some(version) => format!("jawd      {version}, answering, started some other way"),
        None => format!("jawd      not answering on {}", jaw_proto::local::socket(&places.jaw).display()),
    });
    lines.push(format!("log       {}", places.log().display()));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    /// A folder of the test's own standing in for a home, removed when it's dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Scratch {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("jaw-agent-test-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn places(dir: &Path) -> Places {
        Places { home: dir.join("home"), jaw: dir.join("home/jaw"), jaw_home: Some(dir.join("home/jaw")) }
    }

    #[test]
    fn the_launch_agent_runs_jawd_alone_and_logs_into_jaws_folder() {
        let p = plist(
            Path::new("/Users/u/src/jaw/target/release/jawd"),
            Path::new("/Users/u/Library/Application Support/jaw/jawd.log"),
            None,
        );
        assert!(p.contains("<array>\n\t\t<string>/Users/u/src/jaw/target/release/jawd</string>\n\t</array>"), "{p}");
        assert!(p.contains(&format!("<string>{LABEL}</string>")));
        assert!(p.contains("<key>KeepAlive</key>\n\t<true/>"));
        assert!(
            p.contains("<key>StandardErrorPath</key>\n\t<string>/Users/u/Library/Application Support/jaw/jawd.log")
        );
        assert!(!p.contains("JAW_HOME") && !p.contains("ProcessType"), "jawd runs at normal priority: {p}");
        if Path::new("/usr/bin/plutil").exists() {
            let dir = Scratch::new();
            let file = dir.path().join("agent.plist");
            std::fs::write(&file, &p).unwrap();
            let lint = Command::new("/usr/bin/plutil").arg("-lint").arg(&file).output().unwrap();
            assert!(lint.status.success(), "{}", String::from_utf8_lossy(&lint.stdout));
        }
    }

    #[test]
    fn the_unit_stops_jawd_alone_with_sigterm() {
        let u = unit(Path::new("/home/u/bin/jawd"), Path::new("/home/u/.local/share/jaw/jawd.log"), None);
        assert!(u.contains("ExecStart=/home/u/bin/jawd\n") && u.contains("Restart=always"), "{u}");
        assert!(u.contains("KillMode=mixed\nKillSignal=SIGTERM\nTimeoutStopSec=10"), "{u}");
        assert!(u.contains("StandardError=append:/home/u/.local/share/jaw/jawd.log"), "{u}");
        assert!(!u.contains("JAW_HOME"));
    }

    #[test]
    fn installing_writes_the_agent_into_the_home_it_is_given_with_jaws_folder_kept() {
        let dir = Scratch::new();
        let places = places(dir.path());
        let path = places.write(Path::new("/opt/jaw/jawd")).unwrap();
        assert!(path.starts_with(dir.path().join("home")), "{}", path.display());
        let text = std::fs::read_to_string(&path).unwrap();
        let jaw = dir.path().join("home/jaw");
        assert!(text.contains("/opt/jaw/jawd"), "{text}");
        assert!(text.contains(&jaw.join("jawd.log").display().to_string()), "{text}");
        assert!(text.contains("JAW_HOME"), "the agent's jawd keeps the JAW_HOME it was installed with: {text}");
        let mode = std::fs::metadata(&jaw).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn status_says_what_to_do_next() {
        let dir = Scratch::new();
        let places = places(dir.path());
        let not = describe(&places, &Status { installed: false, loaded: false, answering: None });
        assert!(not.contains("not installed; jaw agent install") && not.contains("not answering on"), "{not}");
        let running = describe(&places, &Status { installed: true, loaded: true, answering: Some("0.1.0".into()) });
        assert!(running.contains("installed and loaded") && running.contains("0.1.0, answering\n"), "{running}");
        let other = describe(&places, &Status { installed: false, loaded: false, answering: Some("0.1.0".into()) });
        assert!(other.contains("started some other way"), "{other}");
    }
}
