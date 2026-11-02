//! jawd's Info.plist, embedded in its binary on macOS (`__TEXT,__info_plist`), so macOS asks for the
//! microphone in jaw's words however jawd was built: by `make install`, Homebrew or `make mac`. The
//! binary alone takes it, not its tests.

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo says where jawd's manifest is");
    let plist = std::path::Path::new(&manifest).join("../../mac/JawApp/Helpers/jawd-Info.plist");
    println!("cargo:rerun-if-changed={}", plist.display());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}", plist.display());
    }
}
