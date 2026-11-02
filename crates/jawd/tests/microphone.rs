//! jawd's binary carries its Info.plist on macOS, so macOS asks for the microphone in jaw's words
//! however jawd was built, and never refuses it for want of a reason to give.

#[cfg(target_os = "macos")]
#[test]
fn jawds_binary_carries_its_info_plist_with_why_it_asks_for_the_microphone() {
    let binary = std::fs::read(env!("CARGO_BIN_EXE_jawd")).unwrap();
    let plist =
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mac/JawApp/Helpers/jawd-Info.plist")).unwrap();
    let has = |needle: &[u8]| binary.windows(needle.len()).any(|w| w == needle);
    assert!(has(b"__info_plist"), "the binary has an __info_plist section");
    assert!(has(&plist), "holding jawd-Info.plist as it is");
    assert!(has(b"NSMicrophoneUsageDescription"), "which says why jawd asks for the microphone");
}
