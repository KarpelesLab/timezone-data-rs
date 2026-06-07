//! Error type for timezone data parsing and lookup.

use core::fmt;

/// Errors produced when loading or parsing timezone data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The requested zone was not found in the embedded database.
    NotFound,
    /// A POSIX TZ string could not be parsed. The payload is a short reason.
    BadPosixTz(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound => f.write_str("timezone not found in embedded data"),
            Error::BadPosixTz(why) => write!(f, "malformed POSIX TZ string: {why}"),
        }
    }
}

impl core::error::Error for Error {}
