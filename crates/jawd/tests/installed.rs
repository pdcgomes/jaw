//! jawd as `make install` and Homebrew lay it out: run through a link in a shared bin/ to the jawd
//! in its own prefix, with nothing saying where its engine is, it starts the one in libexec/jaw/
//! beside its real folder.

use std::os::unix::fs::PermissionsExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The jawd this test started, stopped with SIGTERM, as launchd and systemd do, however the test
/// ends.
struct Started(Child);

impl Drop for Started {
    fn drop(&mut self) {
        // SAFETY: kill only sends a signal to the jawd this test started.
        unsafe { libc::kill(self.0.id() as i32, libc::SIGTERM) };
        let _ = self.0.wait();
    }
}

#[test]
fn jawd_linked_into_a_shared_bin_starts_the_engine_in_libexec_beside_its_real_folder() {
    let dir = tempfile::tempdir().unwrap();
    let keg = dir.path().join("Cellar/jaw/0.1.0");
    for folder in [keg.join("bin"), keg.join("libexec/jaw"), dir.path().join("bin")] {
        std::fs::create_dir_all(folder).unwrap();
    }
    std::fs::copy(env!("CARGO_BIN_EXE_jawd"), keg.join("bin/jawd")).unwrap();
    std::os::unix::fs::symlink("../Cellar/jaw/0.1.0/bin/jawd", dir.path().join("bin/jawd")).unwrap();
    // An engine that writes down the path it was started by, and then says nothing.
    let engine = keg.join("libexec/jaw/jaw-engine");
    std::fs::write(&engine, "#!/bin/sh\necho \"$0\" > \"$JAW_HOME/engine-started\"\nexec sleep 600\n").unwrap();
    std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755)).unwrap();

    let home = dir.path().join("home");
    let jawd = Command::new(dir.path().join("bin/jawd"))
        .env("JAW_HOME", &home)
        .env_remove("JAW_ENGINE")
        .env("PATH", "/usr/bin:/bin")
        .env("JAW_NOTIFY", "off")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("jawd starts");
    let jawd = Started(jawd);
    let started = home.join("engine-started");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !std::fs::read_to_string(&started).is_ok_and(|said| said.ends_with('\n')) {
        assert!(Instant::now() < deadline, "jawd never started the engine in {}", engine.display());
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(jawd);
    let by = std::fs::read_to_string(&started).unwrap();
    let by = std::path::Path::new(by.trim());
    assert_eq!(by.canonicalize().unwrap(), engine.canonicalize().unwrap(), "started as {}", by.display());
}
