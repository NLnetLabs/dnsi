//! Message output formats.

mod dig;
mod human;

use super::client::Answer;
use clap::{Parser, ValueEnum};
use std::io;

//------------ OutputFormat --------------------------------------------------

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    /// Similar to dig.
    Dig,
    /// Easily readable, formatted with ANSI codes and whitespace
    Human,
}

#[derive(Clone, Debug, Parser)]
pub struct OutputOptions {
    #[arg(long = "format", default_value = "dig")]
    pub format: OutputFormat,
    #[arg(short, long)]
    pub long: bool,
}

impl OutputFormat {
    pub fn write(
        self,
        msg: &Answer,
        target: &mut impl io::Write,
        options: &OutputOptions,
    ) -> Result<(), io::Error> {
        match self {
            Self::Dig => self::dig::write(msg, target, options),
            Self::Human => self::human::write(msg, target, options),
        }
    }

    pub fn print(self, msg: &Answer, options: &OutputOptions) -> Result<(), io::Error> {
        self.write(msg, &mut io::stdout().lock(), options)
    }
}
