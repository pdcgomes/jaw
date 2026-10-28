//! Where jaw keeps its files, so jawd and every interface find the same ones: `JAW_HOME` if it's
//! set, otherwise `~/Library/Application Support/jaw` on macOS and `$XDG_DATA_HOME/jaw` (or
//! `~/.local/share/jaw`) elsewhere. And where its programs are, relative to one another.

use std::path::{Path, PathBuf};

pub fn home() -> PathBuf {
    if let Some(home) = std::env::var_os("JAW_HOME") {
        return PathBuf::from(home);
    }
    platform()
}

/// Where jaw keeps its files when `JAW_HOME` doesn't say: the folder that holds your account.
pub fn platform() -> PathBuf {
    let user = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
    if cfg!(target_os = "macos") {
        return user.join("Library/Application Support/jaw");
    }
    match std::env::var_os("XDG_DATA_HOME") {
        Some(data) if !data.is_empty() => PathBuf::from(data).join("jaw"),
        _ => user.join(".local/share/jaw"),
    }
}

/// The socket interfaces talk to jawd on.
pub fn socket(home: &Path) -> PathBuf {
    home.join("jaw.sock")
}

/// The first of `paths`, each relative to the folder of the program running, that's a file: from
/// where the program really is, once links such as Homebrew's bin/ are followed, then from where
/// it was started. It's how jaw finds jawd and jawd its engine, as `make install`, cargo and Jaw.app's
/// Contents/Helpers/ lay them out.
pub fn beside(paths: &[&str]) -> Option<PathBuf> {
    beside_program(&std::env::current_exe().ok()?, paths)
}

fn beside_program(program: &Path, paths: &[&str]) -> Option<PathBuf> {
    let real = program.canonicalize().ok();
    [real.as_deref(), Some(program)]
        .into_iter()
        .flatten()
        .filter_map(Path::parent)
        .flat_map(|folder| paths.iter().map(move |path| folder.join(path)))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGINE: [&str; 3] = ["jaw-engine", "../libexec/jaw/jaw-engine", "../engine/jaw-engine"];

    fn file(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    fn real(path: Option<PathBuf>) -> Option<PathBuf> {
        path.map(|p| p.canonicalize().unwrap())
    }

    #[test]
    fn jawd_linked_into_a_shared_bin_finds_the_engine_beside_its_real_folder_as_homebrew_lays_them_out() {
        let dir = tempfile::tempdir().unwrap();
        let keg = dir.path().join("Cellar/jaw/0.1.0");
        file(&keg.join("bin/jawd"));
        file(&keg.join("libexec/jaw/jaw-engine"));
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        let linked = dir.path().join("bin/jawd");
        std::os::unix::fs::symlink("../Cellar/jaw/0.1.0/bin/jawd", &linked).unwrap();
        assert_eq!(real(beside_program(&linked, &ENGINE)), real(Some(keg.join("libexec/jaw/jaw-engine"))));
    }

    #[test]
    fn jawd_in_a_checkouts_target_finds_the_engine_make_builds_and_one_beside_it_first() {
        let dir = tempfile::tempdir().unwrap();
        let jawd = dir.path().join("target/debug/jawd");
        file(&jawd);
        assert_eq!(beside_program(&jawd, &ENGINE), None, "no engine built yet");
        file(&dir.path().join("target/engine/jaw-engine"));
        assert_eq!(real(beside_program(&jawd, &ENGINE)), real(Some(dir.path().join("target/engine/jaw-engine"))));
        file(&dir.path().join("target/debug/jaw-engine"));
        assert_eq!(real(beside_program(&jawd, &ENGINE)), real(Some(dir.path().join("target/debug/jaw-engine"))));
    }

    #[test]
    fn in_the_mac_apps_helpers_jaw_finds_jawd_and_jawd_the_engine_beside_them_as_make_mac_bundles_them() {
        let dir = tempfile::tempdir().unwrap();
        let helpers = dir.path().join("Jaw.app/Contents/Helpers");
        for program in ["jaw", "jawd", "jaw-engine"] {
            file(&helpers.join(program));
        }
        assert_eq!(real(beside_program(&helpers.join("jawd"), &ENGINE)), real(Some(helpers.join("jaw-engine"))));
        assert_eq!(real(beside_program(&helpers.join("jaw"), &["jawd"])), real(Some(helpers.join("jawd"))));
    }

    #[test]
    fn a_program_linked_alone_still_finds_what_is_beside_its_link() {
        let dir = tempfile::tempdir().unwrap();
        file(&dir.path().join("src/jaw/target/release/jaw"));
        file(&dir.path().join("bin/jawd"));
        let linked = dir.path().join("bin/jaw");
        std::os::unix::fs::symlink(dir.path().join("src/jaw/target/release/jaw"), &linked).unwrap();
        assert_eq!(real(beside_program(&linked, &["jawd"])), real(Some(dir.path().join("bin/jawd"))));
        assert_eq!(beside_program(&dir.path().join("src/jaw/target/release/jaw"), &["jawd"]), None);
    }
}
