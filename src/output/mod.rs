//! Message output formats.

mod dig;
mod human;
mod table;

use super::client::Answer;
use clap::{Parser, ValueEnum};
use std::io;

//------------ ANSI codes ----------------------------------------------------

static BOLD: &str = "\x1B[1m";
static UNDERLINE: &str = "\x1B[4m";
static ITALIC: &str = "\x1B[3m";
static RESET: &str = "\x1B[m";

//------------ OutputFormat --------------------------------------------------

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    /// Similar to dig.
    Dig,
    /// Easily readable, formatted with ANSI codes and whitespace
    Human,
    /// Short readable format
    Table,
}

#[derive(Clone, Debug, Parser)]
pub struct OutputOptions {
    #[arg(long = "format", default_value = "dig")]
    pub format: OutputFormat,
}

impl OutputFormat {
    pub fn write(
        self,
        msg: &Answer,
        target: &mut impl io::Write,
    ) -> Result<(), io::Error> {
        match self {
            Self::Dig => self::dig::write(msg, target),
            Self::Human => self::human::write(msg, target),
            Self::Table => self::table::write(msg, target),
        }
    }

    pub fn print(self, msg: &Answer) -> Result<(), io::Error> {
        self.write(msg, &mut io::stdout().lock())
    }
}
