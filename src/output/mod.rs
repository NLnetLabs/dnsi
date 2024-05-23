//! Message output formats.

mod dig;
mod json;

use std::io;
use clap::ValueEnum;
use super::client::Answer;

//------------ OutputFormat --------------------------------------------------

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    /// Similar to dig.
    Dig,
    Json,
}

impl OutputFormat {
    pub fn write(
        self, msg: &Answer, target: &mut impl io::Write
    ) -> Result<(), io::Error> {
        match self {
            Self::Dig => self::dig::write(msg, target),
            Self::Json => self::json::write(msg, target),
        }
    }

    pub fn print(
        self, msg: &Answer,
    ) -> Result<(), io::Error> {
        self.write(msg, &mut io::stdout().lock())
    }
}

