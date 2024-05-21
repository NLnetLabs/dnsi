//! Message output formats.

mod dig;
mod human;


use std::io;
use clap::ValueEnum;
use super::client::Answer;

//------------ OutputFormat --------------------------------------------------

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    /// Similar to dig.
    Dig,
    /// Easily readable, formatted with ANSI codes and whitespace
    Human,
}

impl OutputFormat {
    pub fn write(
        self, msg: &Answer, target: &mut impl io::Write
    ) -> Result<(), io::Error> {
        match self {
            Self::Dig => self::dig::write(msg, target),
            Self::Human => self::human::write(msg, target),
        }
    }

    pub fn print(
        self, msg: &Answer,
    ) -> Result<(), io::Error> {
        self.write(msg, &mut io::stdout().lock())
    }
}

