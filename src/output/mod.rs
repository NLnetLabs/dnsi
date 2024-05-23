//! Message output formats.

mod dig;
mod human;
mod table;
mod table_writer;

use super::client::Answer;
use clap::{Parser, ValueEnum};
use domain::base::Ttl;
use std::fmt::Write as _;
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
    pub fn write(self, msg: &Answer, target: &mut impl io::Write) -> Result<(), io::Error> {
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

fn chunk_ttl(ttl: Ttl) -> (u32, u32, u32, u32) {
    const DAY: u32 = Ttl::DAY.as_secs();
    const HOUR: u32 = Ttl::HOUR.as_secs();
    const MINUTE: u32 = Ttl::MINUTE.as_secs();

    let ttl = ttl.as_secs();
    let (days, ttl) = (ttl / DAY, ttl % DAY);
    let (hours, ttl) = (ttl / HOUR, ttl % HOUR);
    let (minutes, seconds) = (ttl / MINUTE, ttl % MINUTE);
    (days, hours, minutes, seconds)
}

pub fn format_ttl(ttl: Ttl) -> String {
    let (days, hours, minutes, seconds) = chunk_ttl(ttl);

    let mut s = String::new();

    for (n, unit) in [(days, "d"), (hours, "h"), (minutes, "m"), (seconds, "s")] {
        if !s.is_empty() {
            write!(s, " {n:>2}{unit}").unwrap();
        } else if n > 0 {
            write!(s, "{n}{unit}").unwrap();
        }
    }

    s
}
