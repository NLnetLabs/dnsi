//! Message output formats.

mod ansi;
mod dig;
mod json;
mod rfc8427;

use std::io;
use clap::ValueEnum;
mod error;
mod human;
mod table;
mod table_writer;
mod ttl;

use super::client::Answer;
use clap::{Parser, ValueEnum};
use error::OutputError;
use std::io;

//------------ OutputFormat --------------------------------------------------

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    /// Similar to dig.
    Dig,
    /// Easily readable, formatted with ANSI codes and whitespace
    Human,
    /// Short readable format
    Table,
    /// Simple JSON format
    Json,
    /// JSON based on RFC 8427
    RFC8427,
}

#[derive(Clone, Debug, Parser)]
pub struct OutputOptions {
    #[arg(long = "format", default_value = "dig")]
    pub format: OutputFormat,
}

impl OutputFormat {
    pub fn write(self, msg: &Answer, target: &mut impl io::Write) -> Result<(), io::Error> {
        let res = match self {
            Self::Dig => self::dig::write(msg, target),
            Self::Human => self::human::write(msg, target),
            Self::Table => self::table::write(msg, target),
            Self::Json => self::json::write(msg, target),
            Self::RFC8427 => self::rfc8427::write(msg, target),
        };
        match res {
            Ok(()) => Ok(()),
            Err(OutputError::Io(e)) => Err(e),
            Err(OutputError::BadRecord(e)) => {
                writeln!(target, "ERROR: malformed message: {e}")?;
                Ok(())
            }
        }
    }

    pub fn print(self, msg: &Answer) -> Result<(), io::Error> {
        self.write(msg, &mut io::stdout().lock())
    }
}
