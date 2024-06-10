use std::io;

use domain::base::wire::ParseError;

pub enum OutputError {
    Io(io::Error),
    BadRecord(ParseError),
}

impl From<io::Error> for OutputError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ParseError> for OutputError {
    fn from(value: ParseError) -> Self {
        Self::BadRecord(value)
    }
}
