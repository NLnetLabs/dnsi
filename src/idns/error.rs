//! Error handling.

use std::{error, fmt, io};
use std::borrow::Cow;
use domain::base::wire::ParseError;
use domain::net::client::request;


//------------ Error ---------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Error {
    message: Cow<'static, str>,
}

impl From<&'static str> for Error {
    fn from(message: &'static str) -> Self {
        Self { message: Cow::Borrowed(message) }
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self { message: Cow::Owned(message) }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::from(err.to_string())
    }
}

impl From<ParseError> for Error {
    fn from(_err: ParseError) -> Self {
        Self::from("message parse error")
    }
}

impl From<request::Error> for Error {
    fn from(err: request::Error) -> Self {
        Self::from(err.to_string())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.message, f)
    }
}

impl error::Error for Error { }
