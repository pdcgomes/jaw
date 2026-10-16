use std::fmt;
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// A tool this step needs isn't installed, and what it was needed for.
    Missing { tool: &'static str, needed_for: String },
    /// The file couldn't be read as what it is, and why.
    Unreadable { path: PathBuf, why: String },
    /// The file is bigger than jaw will work on, and what the limit is.
    TooLarge { path: PathBuf, why: String },
    /// ffmpeg or ffprobe tried and failed, with the last of what it said.
    Tool { tool: &'static str, path: PathBuf, why: String },
    /// Reading or writing a file failed.
    Io { path: PathBuf, error: std::io::Error },
    /// Something is already under the name a file was to go to, and was left as it was: a file
    /// a call that succeeded made, or one a crash left whole. Where names come from the content,
    /// the file there is the one wanted.
    Exists { path: PathBuf },
    /// Another call is writing the name a file was to go to now; once it returns, the name holds
    /// its file or nothing.
    Busy { path: PathBuf },
    /// The job was cancelled; nothing was left under a final name.
    Cancelled,
}

impl Error {
    pub(crate) fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
        move |error| Error::Io { path: path.to_path_buf(), error }
    }

    pub(crate) fn unreadable(path: &Path, why: impl fmt::Display) -> Error {
        Error::Unreadable { path: path.to_path_buf(), why: why.to_string() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Missing { tool, needed_for } if tool.starts_with("ff") => write!(
                f,
                "{tool} isn't installed, and jaw needs it {needed_for}: install ffmpeg (brew install ffmpeg on macOS, \
                 ffmpeg 7.1 or newer on Linux)"
            ),
            Error::Missing { tool, needed_for } => {
                write!(f, "{tool} isn't available here, and jaw needs it {needed_for}")
            }
            Error::Unreadable { path, why } => write!(f, "jaw can't read {} as media: {why}", path.display()),
            Error::TooLarge { path, why } => write!(f, "{} is too large for jaw to work on: {why}", path.display()),
            Error::Tool { tool, path, why } => write!(f, "{tool} couldn't work on {}: {why}", path.display()),
            Error::Io { path, error } => write!(f, "jaw couldn't read or write {}: {error}", path.display()),
            Error::Exists { path } => write!(
                f,
                "{} is already there, and jaw never writes over a file, since a message may point to it",
                path.display()
            ),
            Error::Busy { path } => {
                write!(f, "{} is being written by another job; try again once it's done", path.display())
            }
            Error::Cancelled => write!(f, "the job was cancelled"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { error, .. } => Some(error),
            _ => None,
        }
    }
}
