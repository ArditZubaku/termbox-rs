//! Errors.

use std::error::Error as StdError;
use std::fmt;
use std::io;

/// Anything that can go wrong while starting or driving a terminal.
#[derive(Debug)]
pub enum Error {
    /// A terminal is already running in this process.
    AlreadyOpen,
    /// The terminal type is not supported by termbox.
    UnsupportedTerminal,
    /// The tty could not be opened.
    FailedToOpenTty,
    /// termbox failed to set up its signal pipe.
    PipeTrap,
    /// `buffer_stderr` was requested but stderr could not be redirected.
    BufferStderr(io::Error),
    /// termbox reported a failure while reading an event.
    Event(isize),
    /// The terminal is not running, so the call did nothing.
    NotRunning,
    /// An error code termbox does not document.
    Unknown(isize),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::AlreadyOpen => f.write_str("a terminal is already open in this process"),
            Error::UnsupportedTerminal => f.write_str("unsupported terminal"),
            Error::FailedToOpenTty => f.write_str("failed to open tty"),
            Error::PipeTrap => f.write_str("failed to set up the signal pipe"),
            Error::BufferStderr(e) => write!(f, "could not redirect stderr: {e}"),
            Error::Event(code) => write!(f, "failed to read an event (code {code})"),
            Error::NotRunning => f.write_str("the terminal is not running"),
            Error::Unknown(code) => write!(f, "unknown termbox error (code {code})"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::BufferStderr(e) => Some(e),
            _ => None,
        }
    }
}

impl From<rustbox::InitError> for Error {
    fn from(err: rustbox::InitError) -> Error {
        use rustbox::InitError::*;
        match err {
            BufferStderrFailed(e) => Error::BufferStderr(e),
            AlreadyOpen => Error::AlreadyOpen,
            UnsupportedTerminal => Error::UnsupportedTerminal,
            FailedToOpenTTy => Error::FailedToOpenTty,
            PipeTrapError => Error::PipeTrap,
            Unknown(code) => Error::Unknown(code),
        }
    }
}

/// The result type used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
