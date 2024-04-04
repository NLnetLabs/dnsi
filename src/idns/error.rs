//! Error handling.

use std::fmt;


//------------ Error ---------------------------------------------------------

pub struct Error {
    message: String,
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.message, f)
    }
}

