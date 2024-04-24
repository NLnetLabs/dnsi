//! Error handling.

use std::{fmt, io};


//------------ Error ---------------------------------------------------------

pub struct Error {
    message: String,
}

impl<'a> From<&'a str> for Error {
    fn from(message: &'a str) -> Self {
        Self { message: message.into() }
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self { message: err.to_string() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.message, f)
    }
}

