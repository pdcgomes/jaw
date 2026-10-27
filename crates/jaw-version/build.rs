//! Gives the crate scripts/version's answer as JAW_VERSION, and has Cargo ask again only when the
//! answer can change: a commit or checkout here, a new tag, VERSION, or JAW_VERSION itself.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let root =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo says where the crate is")).join("../..");
    let script = root.join("scripts/version");
    let out = Command::new("sh").arg(&script).output().expect("sh runs scripts/version");
    assert!(out.status.success(), "scripts/version failed: {}", String::from_utf8_lossy(&out.stderr));
    let version = String::from_utf8(out.stdout).expect("scripts/version says a version in UTF-8");
    println!("cargo:rustc-env=JAW_VERSION={}", version.trim());
    println!("cargo:rerun-if-env-changed=JAW_VERSION");
    for path in [script, root.join("VERSION")].into_iter().chain(moves(&root)) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

/// The files git changes when describe's answer can: HEAD, and its log, which each commit, checkout,
/// reset or rebase in this worktree adds to, and the tags. Each one only if it's there, since Cargo
/// runs a build script every time for a path that isn't, and nothing that other worktrees' commits
/// change, such as the branches' own files, or every worktree would build jaw and jawd again.
fn moves(root: &Path) -> Vec<PathBuf> {
    if !root.join(".git").exists() {
        return vec![];
    }
    let asked = ["HEAD", "logs/HEAD", "refs/tags"].into_iter().flat_map(|path| ["--git-path", path]);
    let Ok(out) = Command::new("git").arg("-C").arg(root).arg("rev-parse").args(asked).output() else {
        return vec![];
    };
    if !out.status.success() {
        return vec![];
    }
    String::from_utf8_lossy(&out.stdout).lines().map(|path| root.join(path)).filter(|path| path.exists()).collect()
}
