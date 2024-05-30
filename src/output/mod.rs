//! Message output formats.

mod dig;
mod json;
mod rfc8427;

use std::io;
use clap::ValueEnum;
use super::client::Answer;

//------------ OutputFormat --------------------------------------------------

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    /// Similar to dig.
    Dig,
    Json,
    /// JSON based on RFC 8427
    RFC8427,
}

impl OutputFormat {
    pub fn write(
        self, msg: &Answer, target: &mut impl io::Write
    ) -> Result<(), io::Error> {
        match self {
            Self::Dig => self::dig::write(msg, target),
            Self::Json => self::json::write(msg, target),
            Self::RFC8427 => self::rfc8427::write(msg, target),
        }
    }

    pub fn print(
        self, msg: &Answer,
    ) -> Result<(), io::Error> {
        self.write(msg, &mut io::stdout().lock())
    }
}
