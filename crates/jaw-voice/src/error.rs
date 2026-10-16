use std::fmt;
use std::path::{Path, PathBuf};

/// What went wrong, in words a person can act on. Events carry it, so it's cloneable and keeps
/// I/O errors as their message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A file couldn't be read, written, synced or renamed.
    File { path: PathBuf, action: &'static str, message: String },
    /// A file that isn't an Ogg Opus voice message jaw can play.
    NotVoice { path: PathBuf, why: String },
    /// libopus refused something.
    Codec(String),
    /// The output or input device couldn't be opened or stopped working.
    Device(String),
    /// A recording couldn't start or had nothing in it.
    Recording(String),
    /// The audio thread couldn't take a command.
    Busy(String),
}

impl Error {
    pub(crate) fn file(path: &Path, action: &'static str, err: std::io::Error) -> Error {
        Error::File { path: path.to_path_buf(), action, message: err.to_string() }
    }

    pub(crate) fn not_voice(path: &Path, why: impl Into<String>) -> Error {
        Error::NotVoice { path: path.to_path_buf(), why: why.into() }
    }

    pub(crate) fn codec(what: &str, err: opus::Error) -> Error {
        Error::Codec(format!("libopus couldn't {what}: {err}"))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::File { path, action, message } => write!(f, "couldn't {action} {}: {message}", path.display()),
            Error::NotVoice { path, why } => write!(f, "{} isn't a voice message jaw can play: {why}", path.display()),
            Error::Codec(message) | Error::Device(message) | Error::Recording(message) | Error::Busy(message) => {
                f.write_str(message)
            }
        }
    }
}

impl std::error::Error for Error {}
