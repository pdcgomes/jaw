//! jaw's folder, left out of Time Machine with the exclusion `tmutil addexclusion` sets, an
//! attribute that goes wherever the folder goes, and given the `.metadata_never_index` the design
//! keeps Spotlight out with, though Spotlight on macOS 26 and 27 indexes a folder holding one all
//! the same. Each is set only when it isn't already; one that can't be is said once, and jawd starts
//! all the same.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// The attribute Time Machine reads, and the value `tmutil addexclusion` gives it: the string
/// "com.apple.backupd" as a binary property list.
const EXCLUDED: &str = "com.apple.metadata:com_apple_backup_excludeItem";
const BY_BACKUPD: &[u8] =
    b"bplist00\x5f\x10\x11com.apple.backupd\x08\0\0\0\0\0\0\x01\x01\0\0\0\0\0\0\0\x01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x1c";

/// The file that tells Spotlight to leave the folder it's in alone.
const NEVER_INDEX: &str = ".metadata_never_index";

/// Leaves `home` out of Time Machine and gives it Spotlight's marker, saying for each that can't be
/// why not.
pub fn keep_out(home: &Path) {
    if let Err(why) = out_of_time_machine(home) {
        eprintln!("jawd: can't leave {} out of Time Machine: {why}", home.display());
    }
    if let Err(why) = out_of_spotlight(home) {
        eprintln!("jawd: can't put Spotlight's {NEVER_INDEX} in {}: {why}", home.display());
    }
}

fn out_of_time_machine(home: &Path) -> std::io::Result<()> {
    let path = CString::new(home.as_os_str().as_bytes())?;
    let name = CString::new(EXCLUDED)?;
    let mut held = [0u8; 64];
    // SAFETY: both names end in NUL and outlive the call, and `held` is as long as it's said to be.
    let len = unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), held.as_mut_ptr().cast(), held.len(), 0, 0) };
    if usize::try_from(len).is_ok_and(|len| held[..len] == *BY_BACKUPD) {
        return Ok(());
    }
    // SAFETY: both names end in NUL and outlive the call, and the value is as long as it's said to be.
    let set =
        unsafe { libc::setxattr(path.as_ptr(), name.as_ptr(), BY_BACKUPD.as_ptr().cast(), BY_BACKUPD.len(), 0, 0) };
    if set != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

fn out_of_spotlight(home: &Path) -> std::io::Result<()> {
    let marker = home.join(NEVER_INDEX);
    if !marker.is_file() {
        std::fs::OpenOptions::new().append(true).create(true).open(&marker)?;
    }
    Ok(())
}
